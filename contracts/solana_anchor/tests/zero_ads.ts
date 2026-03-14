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

    // Airdrop SOL
    await provider.connection.requestAirdrop(advertiser.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await provider.connection.requestAirdrop(agent.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);

    // Wait for confirmation
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Create SPL token mint
    mint = await createMint(
      provider.connection,
      advertiser,
      advertiser.publicKey,
      null,
      6
    );

    // Create token accounts
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

    // Mint tokens to advertiser
    await mintTo(
      provider.connection,
      advertiser,
      mint,
      advertiserTokenAccount,
      advertiser,
      budget.toNumber()
    );
  });

  it("creates a campaign with budget tracking (C-03)", async () => {
    const [campaignPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("campaign"), campaignId],
      program.programId
    );
    const [vaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), campaignId],
      program.programId
    );

    // Note: The program uses `init` without seeds on campaign,
    // so we use a generated keypair for the campaign account.
    const campaignAccount = Keypair.generate();

    await program.methods
      .createCampaign(
        Array.from(campaignId),
        budget,
        payout,
        Array.from(graphHash),
        oraclePubkey
      )
      .accounts({
        campaign: campaignAccount.publicKey,
        advertiser: advertiser.publicKey,
        advertiserTokenAccount,
        vaultTokenAccount: vaultPda,
        tokenMint: mint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([advertiser, campaignAccount])
      .rpc();

    const state = await program.account.campaignState.fetch(campaignAccount.publicKey);
    expect(state.advertiser.toBase58()).to.equal(advertiser.publicKey.toBase58());
    expect(state.remainingBudget.toNumber()).to.equal(budget.toNumber());
    expect(state.payout.toNumber()).to.equal(payout.toNumber());
    expect(state.oraclePubkey.toBase58()).to.equal(oraclePubkey.toBase58());
  });

  it("rejects payout=0 (validation)", async () => {
    const campaignAccount = Keypair.generate();
    const campaignId2 = new Uint8Array(32);
    campaignId2.set(Buffer.from("campaign-zero-pay"));
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
          campaign: campaignAccount.publicKey,
          advertiser: advertiser.publicKey,
          advertiserTokenAccount,
          vaultTokenAccount: vaultPda2,
          tokenMint: mint,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([advertiser, campaignAccount])
        .rpc();
      expect.fail("Should have thrown");
    } catch (err) {
      expect(err.toString()).to.include("PayoutMustBePositive");
    }
  });

  it("rejects claim without Ed25519 signature instruction (C-01)", async () => {
    const campaignAccount = Keypair.generate();
    const campaignId3 = new Uint8Array(32);
    campaignId3.set(Buffer.from("campaign-nosig"));
    const [vaultPda3] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), campaignId3],
      program.programId
    );

    // First create
    await mintTo(provider.connection, advertiser, mint, advertiserTokenAccount, advertiser, budget.toNumber());
    await program.methods
      .createCampaign(
        Array.from(campaignId3),
        budget,
        payout,
        Array.from(graphHash),
        oraclePubkey
      )
      .accounts({
        campaign: campaignAccount.publicKey,
        advertiser: advertiser.publicKey,
        advertiserTokenAccount,
        vaultTokenAccount: vaultPda3,
        tokenMint: mint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([advertiser, campaignAccount])
      .rpc();

    // Try to claim without the Ed25519 pre-instruction
    const [claimReceiptPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("claimed"), campaignId3, agent.publicKey.toBuffer()],
      program.programId
    );

    const fakeSignature = new Uint8Array(64);

    try {
      await program.methods
        .claimPayout(Array.from(fakeSignature))
        .accounts({
          campaign: campaignAccount.publicKey,
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
});
