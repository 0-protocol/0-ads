use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use solana_program::ed25519_program;

// Phase 4: Solana Anchor Program for AdEscrow
// High-frequency, low-fee settlement for Agent Attention Bounties.

declare_id!("Ads1111111111111111111111111111111111111111");

#[program]
pub mod zero_ads_escrow {
    use super::*;

    pub fn create_campaign(
        ctx: Context<CreateCampaign>,
        campaign_id: [u8; 32],
        budget: u64,
        payout: u64,
        verification_graph_hash: [u8; 32],
        oracle_pubkey: Pubkey,
    ) -> Result<()> {
        let campaign = &mut ctx.accounts.campaign;
        campaign.advertiser = ctx.accounts.advertiser.key();
        campaign.campaign_id = campaign_id;
        campaign.payout = payout;
        campaign.verification_graph_hash = verification_graph_hash;
        campaign.oracle_pubkey = oracle_pubkey;

        // Transfer SPL USDC from advertiser to escrow vault
        let cpi_accounts = Transfer {
            from: ctx.accounts.advertiser_token_account.to_account_info(),
            to: ctx.accounts.vault_token_account.to_account_info(),
            authority: ctx.accounts.advertiser.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, budget)?;

        Ok(())
    }

    pub fn claim_payout(ctx: Context<ClaimPayout>, _oracle_signature: [u8; 64]) -> Result<()> {
        let campaign = &mut ctx.accounts.campaign;
        
        // In Solana, signature verification is heavily optimized by using the ed25519 
        // instruction pre-compile in the transaction before this instruction is called.
        // Here we would verify the sysvar instructions to ensure the Ed25519 signature 
        // from the `oracle_pubkey` was verified in the same transaction.

        // Transfer payout from vault to agent
        let seeds = &[
            b"vault".as_ref(),
            &campaign.campaign_id,
            &[ctx.bumps["vault_token_account"]],
        ];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_token_account.to_account_info(),
            to: ctx.accounts.agent_token_account.to_account_info(),
            authority: ctx.accounts.vault_token_account.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, campaign.payout)?;

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(campaign_id: [u8; 32])]
pub struct CreateCampaign<'info> {
    #[account(init, payer = advertiser, space = 8 + 32 + 32 + 8 + 32 + 32)]
    pub campaign: Account<'info, CampaignState>,
    #[account(mut)]
    pub advertiser: Signer<'info>,
    #[account(mut)]
    pub advertiser_token_account: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = advertiser,
        seeds = [b"vault", campaign_id.as_ref()],
        bump,
        token::mint = usdc_mint,
        token::authority = vault_token_account,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    /// CHECK: Safe
    pub usdc_mint: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimPayout<'info> {
    #[account(mut)]
    pub campaign: Account<'info, CampaignState>,
    #[account(mut)]
    pub agent: Signer<'info>,
    #[account(mut)]
    pub agent_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[account]
pub struct CampaignState {
    pub advertiser: Pubkey,
    pub campaign_id: [u8; 32],
    pub payout: u64,
    pub verification_graph_hash: [u8; 32],
    pub oracle_pubkey: Pubkey,
}
