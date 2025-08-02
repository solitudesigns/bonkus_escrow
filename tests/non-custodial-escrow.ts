import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { BonkEscrowFinal } from "../target/types/bonk_escrow_final";
import {
  TOKEN_PROGRAM_ID,
  createMint,
  getAssociatedTokenAddress,
  mintTo,
  createAccount,
} from "@solana/spl-token";
import { Keypair, SystemProgram, PublicKey } from "@solana/web3.js";
import { assert } from "chai";

describe("bonk_escrow_final", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.BonkEscrowFinal as Program<BonkEscrowFinal>;
  const owner = provider.wallet as anchor.Wallet;

  let mint: PublicKey;
  let vaultAta: PublicKey;
  let vaultAuthPda: PublicKey;
  let vaultAuthBump: number;

  const contributors: Keypair[] = [];
  const contributorAtas: PublicKey[] = [];
  const escrowName = "escrow-game";

  let escrowPda: PublicKey;
  let escrowBump: number;

  // 🟢 Utility to airdrop SOL
  async function airdrop(pubkey: PublicKey) {
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(pubkey, 2e9)
    );
  }

  it("🟢 Setup: Create Mint, ATAs, Contributors", async () => {
    // ✅ Create Mint
    mint = await createMint(
      provider.connection,
      (owner as any).payer, // owner payer
      owner.publicKey,
      null,
      9
    );

    // ✅ Create contributors & fund tokens
    for (let i = 0; i < 5; i++) {
      const kp = Keypair.generate();
      contributors.push(kp);

      await airdrop(kp.publicKey);

      const ata = await createAccount(
        provider.connection,
        (owner as any).payer,
        mint,
        kp.publicKey
      );
      contributorAtas.push(ata);

      await mintTo(
        provider.connection,
        (owner as any).payer,
        mint,
        ata,
        owner.publicKey,
        10_000_000_000n
      );
    }

    // ✅ Derive Escrow PDA
    [escrowPda, escrowBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("escrow"), owner.publicKey.toBuffer(), Buffer.from(escrowName)],
      program.programId
    );

    // ✅ Vault Auth PDA
    [vaultAuthPda, vaultAuthBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault-auth"), escrowPda.toBuffer()],
      program.programId
    );

    // ✅ Vault ATA
    vaultAta = await getAssociatedTokenAddress(mint, vaultAuthPda, true);

    // ✅ Initialize Escrow
    await program.methods
      .initialize(escrowName)
      .accounts({
        escrow: escrowPda,
        owner: owner.publicKey,
        mint,
        vaultAuth: vaultAuthPda,
        vaultAta,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: anchor.utils.token.ASSOCIATED_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    console.log("✅ Escrow Initialized:", escrowPda.toBase58());
  });

  it("🟢 Deposit: 5 tokens each from contributors", async () => {
    for (let i = 0; i < contributors.length; i++) {
      await program.methods
        .deposit(escrowName, new anchor.BN(5)) // 5 tokens
        .accounts({
          escrow: escrowPda,
          contributor: contributors[i].publicKey,
          contributorAta: contributorAtas[i],
          vaultAta,
          vaultAuth: vaultAuthPda,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([contributors[i]])
        .rpc();

     
    }

    // ✅ Verify escrow state has 5 contributors
    const state = await program.account.escrowState.fetch(escrowPda);
    assert.equal(state.contributors.length, 5, "All contributors should have deposited");
  });

  it("🟢 Distribute Mode 0: All funds to one contributor", async () => {
    const target = contributors[0].publicKey; // First contributor


    
    await program.methods
      .distribute(escrowName, 0, target)
      .accounts({
        escrow: escrowPda,
        vaultAta,
        vaultAuth: vaultAuthPda,
        owner: owner.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .remainingAccounts(
         contributorAtas
          .map((ata) => ({ pubkey: ata, isWritable: true, isSigner: false }))
      )
      .rpc();

    console.log("✅ Mode 0 distribution sent all funds to:", target.toBase58());
  });

  it("🟢 Re-init new escrow & Distribute Mode 1: Split equally except one", async () => {
    const newName = "escrow-game2";

    // ✅ Derive new PDAs
    [escrowPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("escrow"), owner.publicKey.toBuffer(), Buffer.from(newName)],
      program.programId
    );
    [vaultAuthPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault-auth"), escrowPda.toBuffer()],
      program.programId
    );
    vaultAta = await getAssociatedTokenAddress(mint, vaultAuthPda, true);

    // ✅ Init
    await program.methods
      .initialize(newName)
      .accounts({
        escrow: escrowPda,
        owner: owner.publicKey,
        mint,
        vaultAuth: vaultAuthPda,
        vaultAta,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: anchor.utils.token.ASSOCIATED_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    console.log("✅ Second escrow initialized:", escrowPda.toBase58());

    // ✅ Deposit
    for (let i = 0; i < contributors.length; i++) {
  
      await program.methods
        .deposit(newName, new anchor.BN(5))
        .accounts({
          escrow: escrowPda,
          contributor: contributors[i].publicKey,
          contributorAta: contributorAtas[i],
          vaultAta,
          vaultAuth: vaultAuthPda,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([contributors[i]])
        .rpc();
    }

    const excluded = contributors[4].publicKey;

    await program.methods
      .distribute(newName, 1, excluded)
      .accounts({
        escrow: escrowPda,
        vaultAta,
        vaultAuth: vaultAuthPda,
        owner: owner.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .remainingAccounts(
       contributorAtas
          .map((ata) => ({ pubkey: ata, isWritable: true, isSigner: false }))
      )
      .rpc();

    console.log("✅ Mode 1 distribution done, excluded:", excluded.toBase58());
  });
});
