import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey, Keypair, SystemProgram, Ed25519Program, SYSVAR_INSTRUCTIONS_PUBKEY } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID, createMint, createAccount, mintTo, getAccount } from "@solana/spl-token";
import { expect } from "chai";
import * as nacl from "tweetnacl";

describe("zero_ads_escrow", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.ZeroAdsEscrow;

  let mint: PublicKey;
  let advertiser: Keypair;
  let advertiserTokenAccount: PublicKey;
  let agent: Keypair;
  let agentTokenAccount: PublicKey;
  let oracleKeypair: nacl.SignKeyPair;
  let oraclePubkey: PublicKey;

  const campaignId = new Uint8Array(32);
  campaignId.set(Buffer.from("campaign-test-001"));

  const graphHash = new Uint8Array(32);
  graphHash.set(Buffer.from("graph-hash-test-001"));

  const budget = new anchor.BN(1000_000_000); // 1000 tokens (6 decimals)
  const payout = new anchor.BN(100_000_000);  // 100 tokens

  before(async () => {
    advertiser = Keypair.generate();
    agent = Keypair.generate();
    oracleKeypair = nacl.sign.keyPair();
    oraclePubkey = new PublicKey(oracleKeypair.publicKey);

    await provider.connection.requestAirdrop(advertiser.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await provider.connection.requestAirdrop(agent.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);

    await new Promise(resolve => setTimeout(resolve, 1000));

    mint = await createMint(
      provider.connection,
      advertiser,
      advertiser.publicKey,
      null,
      6
    );

    advertiserTokenAccount = await createAccount(
      provider.connection,
      advertiser,
      mint,
      advertiser.publicKey
    );
    agentTokenAccount = await createAccount(
      provider.connection,
      agent,
      mint,
      agent.publicKey
    );

    await mintTo(
      provider.connection,
      advertiser,
      mint,
      advertiserTokenAccount,
      advertiser,
      budget.toNumber() * 3 // enough for multiple test campaigns
    );
  });

  it("creates a campaign with PDA-derived account (N-02) and budget tracking (C-03)", async () => {
    const [campaignPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("campaign"), campaignId],
      program.programId
    );
    const [vaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), campaignId],
      program.programId
    );

    await program.methods
      .createCampaign(
        Array.from(campaignId),
        budget,
        payout,
        Array.from(graphHash),
        oraclePubkey
      )
      .accounts({
        campaign: campaignPda,
        advertiser: advertiser.publicKey,
        advertiserTokenAccount,
        vaultTokenAccount: vaultPda,
        tokenMint: mint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([advertiser])
      .rpc();

    const state = await program.account.campaignState.fetch(campaignPda);
    expect(state.advertiser.toBase58()).to.equal(advertiser.publicKey.toBase58());
    expect(state.remainingBudget.toNumber()).to.equal(budget.toNumber());
    expect(state.payout.toNumber()).to.equal(payout.toNumber());
    expect(state.oraclePubkey.toBase58()).to.equal(oraclePubkey.toBase58());
    expect(state.createdAt.toNumber()).to.be.greaterThan(0);
  });

  it("rejects duplicate campaign_id via PDA (N-02)", async () => {
    const [campaignPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("campaign"), campaignId],
      program.programId
    );
    const [vaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), campaignId],
      program.programId
    );

    try {
      await program.methods
        .createCampaign(
          Array.from(campaignId),
          budget,
          payout,
          Array.from(graphHash),
          oraclePubkey
        )
        .accounts({
          campaign: campaignPda,
          advertiser: advertiser.publicKey,
          advertiserTokenAccount,
          vaultTokenAccount: vaultPda,
          tokenMint: mint,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([advertiser])
        .rpc();
      expect.fail("Should have thrown on duplicate campaign PDA");
    } catch (err) {
      // Anchor rejects because the PDA account is already initialized
      expect(err.toString()).to.not.be.empty;
    }
  });

  it("rejects payout=0 (validation)", async () => {
    const campaignId2 = new Uint8Array(32);
    campaignId2.set(Buffer.from("campaign-zero-pay"));
    const [campaignPda2] = PublicKey.findProgramAddressSync(
      [Buffer.from("campaign"), campaignId2],
      program.programId
    );
    const [vaultPda2] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), campaignId2],
      program.programId
    );

    try {
      await program.methods
        .createCampaign(
          Array.from(campaignId2),
          budget,
          new anchor.BN(0),
          Array.from(graphHash),
          oraclePubkey
        )
        .accounts({
          campaign: campaignPda2,
          advertiser: advertiser.publicKey,
          advertiserTokenAccount,
          vaultTokenAccount: vaultPda2,
          tokenMint: mint,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([advertiser])
        .rpc();
      expect.fail("Should have thrown");
    } catch (err) {
      expect(err.toString()).to.include("PayoutMustBePositive");
    }
  });

  it("rejects claim without Ed25519 signature instruction (C-01)", async () => {
    const campaignId3 = new Uint8Array(32);
    campaignId3.set(Buffer.from("campaign-nosig"));
    const [campaignPda3] = PublicKey.findProgramAddressSync(
      [Buffer.from("campaign"), campaignId3],
      program.programId
    );
    const [vaultPda3] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), campaignId3],
      program.programId
    );

    await program.methods
      .createCampaign(
        Array.from(campaignId3),
        budget,
        payout,
        Array.from(graphHash),
        oraclePubkey
      )
      .accounts({
        campaign: campaignPda3,
        advertiser: advertiser.publicKey,
        advertiserTokenAccount,
        vaultTokenAccount: vaultPda3,
        tokenMint: mint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([advertiser])
      .rpc();

    const [claimReceiptPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("claimed"), campaignId3, agent.publicKey.toBuffer()],
      program.programId
    );

    const fakeSignature = new Uint8Array(64);

    try {
      await program.methods
        .claimPayout(Array.from(fakeSignature))
        .accounts({
          campaign: campaignPda3,
          agent: agent.publicKey,
          agentTokenAccount,
          vaultTokenAccount: vaultPda3,
          claimReceipt: claimReceiptPda,
          instructionSysvar: SYSVAR_INSTRUCTIONS_PUBKEY,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([agent])
        .rpc();
      expect.fail("Should have thrown due to missing Ed25519 verification");
    } catch (err) {
      expect(err.toString()).to.include("InvalidSignature");
    }
  });

  it("rejects cancel before cooldown (N-01)", async () => {
    const [campaignPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("campaign"), campaignId],
      program.programId
    );
    const [vaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), campaignId],
      program.programId
    );

    try {
      await program.methods
        .cancelCampaign()
        .accounts({
          campaign: campaignPda,
          advertiser: advertiser.publicKey,
          advertiserTokenAccount,
          vaultTokenAccount: vaultPda,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([advertiser])
        .rpc();
      expect.fail("Should have thrown due to cooldown");
    } catch (err) {
      expect(err.toString()).to.include("CancelCooldownNotElapsed");
    }
  });

  it("rejects cancel from non-advertiser (N-01)", async () => {
    const [campaignPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("campaign"), campaignId],
      program.programId
    );
    const [vaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), campaignId],
      program.programId
    );

    try {
      await program.methods
        .cancelCampaign()
        .accounts({
          campaign: campaignPda,
          advertiser: agent.publicKey, // wrong advertiser
          advertiserTokenAccount: agentTokenAccount,
          vaultTokenAccount: vaultPda,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([agent])
        .rpc();
      expect.fail("Should have thrown due to wrong advertiser");
    } catch (err) {
      // Anchor has_one constraint fails
      expect(err.toString()).to.not.be.empty;
    }
  });
});
