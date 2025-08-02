use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("9obCENSCc25Fw6ca4WZNUXQfhYM9xymQGAPkNc5Udsec");

#[program]
pub mod bonk_escrow_final {
    use anchor_spl::associated_token::get_associated_token_address;

    use super::*;

    /// ✅ Initialize escrow with a unique name
    pub fn initialize(ctx: Context<Initialize>, name: String) -> Result<()> {
        require!(name.len() <= 32, EscrowError::NameTooLong);

        let esc = &mut ctx.accounts.escrow;
        esc.owner = ctx.accounts.owner.key();
        esc.token_mint = ctx.accounts.mint.key();
        esc.contributors = vec![];
        esc.distributed = false;
        esc.name = name;

        Ok(())
    }

    /// ✅ Deposit exactly 5 tokens; max 5 contributors allowed
    pub fn deposit(ctx: Context<Deposit>, name: String, amount: u64) -> Result<()> {
        let esc = &mut ctx.accounts.escrow;

        require!(esc.name == name, EscrowError::NameMismatch);
        require!(
            esc.contributors.len() < 5,
            EscrowError::MaxContributorsReached
        );
        require!(
            !esc.contributors.contains(ctx.accounts.contributor.key),
            EscrowError::AlreadyDeposited
        );
        require!(amount == 5, EscrowError::InvalidDepositAmount);

        let cpi_accounts = Transfer {
            from: ctx.accounts.contributor_ata.to_account_info(),
            to: ctx.accounts.vault_ata.to_account_info(),
            authority: ctx.accounts.contributor.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        esc.contributors.push(ctx.accounts.contributor.key());
        Ok(())
    }

    /// ✅ Distribute tokens
    /// - Mode 0: Send all to `target_pubkey`
    /// - Mode 1: Distribute equally to all except `target_pubkey`
    pub fn distribute<'c: 'info, 'info>(
        ctx: Context<'_, '_, 'c, 'info, Distribute<'info>>,
        name: String,
        mode: u8,
        target_pubkey: Pubkey,
    ) -> Result<()> {
        let esc = &mut ctx.accounts.escrow;

        require!(esc.name == name, EscrowError::NameMismatch);
        require!(
            esc.owner == ctx.accounts.owner.key(),
            EscrowError::Unauthorized
        );
        require!(!esc.distributed, EscrowError::AlreadyDistributed);
        require!(esc.contributors.len() == 5, EscrowError::NotFull);

        let vault_balance = ctx.accounts.vault_ata.amount;
        require!(vault_balance > 0, EscrowError::InvalidMode);

        match mode {
            // ✅ Mode 0: Send all funds to one contributor
            0 => {
                require!(
                    esc.contributors.contains(&target_pubkey),
                    EscrowError::InvalidTarget
                );

                let target = target_pubkey;
                let recipient_ata = get_associated_token_address(&target, &esc.token_mint);

                // ✅ Find matching AccountInfo passed in ctx.remaining_accounts
                let ata_info = ctx
                    .remaining_accounts
                    .iter()
                    .find(|acc| acc.key() == recipient_ata)
                    .ok_or(EscrowError::MissingRecipientAta)?
                    .clone();
                let cpi_accounts = Transfer {
                    from: ctx.accounts.vault_ata.to_account_info(),
                    to: ata_info,
                    authority: ctx.accounts.vault_auth.to_account_info(),
                };
                let escrow_key = esc.key();

                let seeds: &[&[u8]] =
                    &[b"vault-auth", escrow_key.as_ref(), &[ctx.bumps.vault_auth]];
                let signer: &[&[&[u8]]] = &[seeds];

                let cpi_ctx = CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    cpi_accounts,
                    signer,
                );

                token::transfer(cpi_ctx, vault_balance)?;
            }

            // ✅ Mode 1: Distribute equally to all except excluded contributor
            1 => {
                let recipients: Vec<Pubkey> = esc
                    .contributors
                    .iter()
                    .cloned()
                    .filter(|c| *c != target_pubkey)
                    .collect();

                require!(recipients.len() > 0, EscrowError::InvalidMode);
                let share = vault_balance / recipients.len() as u64;

                for recipient in recipients {
                    let recipient_ata = get_associated_token_address(&recipient, &esc.token_mint);

                    // ✅ Find matching AccountInfo passed in ctx.remaining_accounts
                    let ata_info = ctx
                        .remaining_accounts
                        .iter()
                        .find(|acc| acc.key() == recipient_ata)
                        .ok_or(EscrowError::MissingRecipientAta)?
                        .clone();

                    let cpi_accounts = Transfer {
                        from: ctx.accounts.vault_ata.to_account_info(),
                        to: ata_info,
                        authority: ctx.accounts.vault_auth.to_account_info(),
                    };
                    let escrow_key = esc.key();

                    let seeds: &[&[u8]] =
                        &[b"vault-auth", escrow_key.as_ref(), &[ctx.bumps.vault_auth]];
                    let signer: &[&[&[u8]]] = &[seeds];

                    let cpi_ctx = CpiContext::new_with_signer(
                        ctx.accounts.token_program.to_account_info(),
                        cpi_accounts,
                        signer,
                    );

                    token::transfer(cpi_ctx, share)?;
                }
            }

            _ => return Err(error!(EscrowError::InvalidMode)),
        }

        esc.distributed = true;
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(name: String)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = owner,
        seeds = [b"escrow", owner.key().as_ref(), name.as_bytes()],
        bump,
        space = 8 + 32 + 32 + 4 + (5 * 32) + 1 + 4 + 32
    )]
    pub escrow: Account<'info, EscrowState>,
    #[account(mut)]
    pub owner: Signer<'info>,
    pub mint: Account<'info, Mint>,

    #[account(seeds = [b"vault-auth", escrow.key().as_ref()], bump)]
    /// CHECK: PDA authority
    pub vault_auth: AccountInfo<'info>,

    #[account(
        init,
        payer = owner,
        associated_token::mint = mint,
        associated_token::authority = vault_auth
    )]
    pub vault_ata: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(name: String)]
pub struct Deposit<'info> {
    #[account(
        mut,
        seeds = [b"escrow", escrow.owner.as_ref(), name.as_bytes()],
        bump
    )]
    pub escrow: Account<'info, EscrowState>,
    #[account(mut)]
    pub contributor: Signer<'info>,
    #[account(mut, associated_token::mint = escrow.token_mint, associated_token::authority = contributor)]
    pub contributor_ata: Account<'info, TokenAccount>,
    #[account(mut)]
    pub vault_ata: Account<'info, TokenAccount>,
    #[account(seeds = [b"vault-auth", escrow.key().as_ref()], bump)]
    /// CHECK: PDA authority
    pub vault_auth: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(name: String)]
pub struct Distribute<'info> {
    #[account(
        mut,
        seeds = [b"escrow", escrow.owner.as_ref(), name.as_bytes()],
        bump
    )]
    pub escrow: Account<'info, EscrowState>,
    #[account(mut)]
    pub vault_ata: Account<'info, TokenAccount>,
    #[account(seeds = [b"vault-auth", escrow.key().as_ref()], bump)]
    /// CHECK: PDA authority
    pub vault_auth: AccountInfo<'info>,
    #[account(mut)]
    pub owner: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[account]
pub struct EscrowState {
    pub owner: Pubkey,
    pub token_mint: Pubkey,
    pub contributors: Vec<Pubkey>,
    pub distributed: bool,
    pub name: String,
}

#[error_code]
pub enum EscrowError {
    #[msg("Max 5 contributors allowed")]
    MaxContributorsReached,
    #[msg("Contributor already deposited")]
    AlreadyDeposited,
    #[msg("Deposit must be exactly 5 tokens")]
    InvalidDepositAmount,
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Not all contributors have deposited")]
    NotFull,
    #[msg("Already distributed")]
    AlreadyDistributed,
    #[msg("Invalid distribution mode")]
    InvalidMode,
    #[msg("Name too long (max 32 bytes)")]
    NameTooLong,
    #[msg("Missing recipient ATA in remaining_accounts")]
    MissingRecipientAta,
    #[msg("Escrow name does not match")]
    NameMismatch,
    #[msg("Target or excluded contributor is invalid")]
    InvalidTarget,
}
