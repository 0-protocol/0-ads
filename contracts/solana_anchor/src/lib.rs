use anchor_lang::prelude::*;
use anchor_lang::solana_program::ed25519_program;
use anchor_lang::solana_program::sysvar::instructions as ix_sysvar;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

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
        require!(payout > 0, ZeroAdsError::PayoutMustBePositive);
        require!(budget >= payout, ZeroAdsError::BudgetTooSmall);

        let campaign = &mut ctx.accounts.campaign;
        campaign.advertiser = ctx.accounts.advertiser.key();
        campaign.campaign_id = campaign_id;
        campaign.payout = payout;
        campaign.remaining_budget = budget;
        campaign.verification_graph_hash = verification_graph_hash;
        campaign.oracle_pubkey = oracle_pubkey;

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

    pub fn claim_payout(ctx: Context<ClaimPayout>, oracle_signature: [u8; 64]) -> Result<()> {
        let campaign = &mut ctx.accounts.campaign;

        require!(
            campaign.remaining_budget >= campaign.payout,
            ZeroAdsError::CampaignExhausted
        );

        verify_ed25519_signature(
            &ctx.accounts.instruction_sysvar,
            &campaign.oracle_pubkey,
            &campaign.campaign_id,
            &ctx.accounts.agent.key(),
            campaign.payout,
        )?;

        campaign.remaining_budget -= campaign.payout;

        let receipt = &mut ctx.accounts.claim_receipt;
        receipt.campaign_id = campaign.campaign_id;
        receipt.agent = ctx.accounts.agent.key();
        receipt.claimed_at = Clock::get()?.unix_timestamp;

        let seeds = &[
            b"vault".as_ref(),
            &campaign.campaign_id,
            &[ctx.bumps.vault_token_account],
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

fn verify_ed25519_signature(
    instruction_sysvar: &AccountInfo,
    oracle_pubkey: &Pubkey,
    campaign_id: &[u8; 32],
    agent: &Pubkey,
    payout: u64,
) -> Result<()> {
    let current_ix_index =
        ix_sysvar::load_current_index_checked(instruction_sysvar).map_err(|_| {
            error!(
                "Failed to load current instruction index from sysvar"
            );
            ZeroAdsError::InvalidSignature
        })?;

    require!(current_ix_index > 0, ZeroAdsError::InvalidSignature);

    let ed25519_ix = ix_sysvar::load_instruction_at_checked(
        (current_ix_index - 1) as usize,
        instruction_sysvar,
    )
    .map_err(|_| ZeroAdsError::InvalidSignature)?;

    require!(
        ed25519_ix.program_id == ed25519_program::id(),
        ZeroAdsError::InvalidSignature
    );

    // Ed25519 instruction data layout: 1 sig count + per-sig header (2+2+2+2+2+2+2+2 = 16 bytes)
    // then pubkey (32), signature (64), message (variable)
    let ix_data = &ed25519_ix.data;
    require!(ix_data.len() >= 16 + 32 + 64, ZeroAdsError::InvalidSignature);

    let num_signatures = ix_data[0];
    require!(num_signatures == 1, ZeroAdsError::InvalidSignature);

    // Extract pubkey from the instruction data
    let pubkey_offset = u16::from_le_bytes([ix_data[6], ix_data[7]]) as usize;
    require!(
        ix_data.len() >= pubkey_offset + 32,
        ZeroAdsError::InvalidSignature
    );
    let ix_pubkey = &ix_data[pubkey_offset..pubkey_offset + 32];
    require!(
        ix_pubkey == oracle_pubkey.as_ref(),
        ZeroAdsError::InvalidSignature
    );

    // Reconstruct expected message: campaign_id(32) + agent(32) + payout(8)
    let mut expected_msg = Vec::with_capacity(72);
    expected_msg.extend_from_slice(campaign_id);
    expected_msg.extend_from_slice(agent.as_ref());
    expected_msg.extend_from_slice(&payout.to_le_bytes());

    let msg_offset = u16::from_le_bytes([ix_data[10], ix_data[11]]) as usize;
    let msg_len = u16::from_le_bytes([ix_data[8], ix_data[9]]) as usize;
    require!(
        ix_data.len() >= msg_offset + msg_len,
        ZeroAdsError::InvalidSignature
    );
    let ix_msg = &ix_data[msg_offset..msg_offset + msg_len];
    require!(ix_msg == expected_msg.as_slice(), ZeroAdsError::InvalidSignature);

    Ok(())
}

#[derive(Accounts)]
#[instruction(campaign_id: [u8; 32])]
pub struct CreateCampaign<'info> {
    #[account(
        init,
        payer = advertiser,
        space = 8 + CampaignState::INIT_SPACE,
    )]
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
        token::mint = token_mint,
        token::authority = vault_token_account,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    /// CHECK: The mint for the campaign vault token. Any SPL token is supported.
    pub token_mint: AccountInfo<'info>,
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
    #[account(
        mut,
        seeds = [b"vault", campaign.campaign_id.as_ref()],
        bump,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = agent,
        space = 8 + ClaimReceipt::INIT_SPACE,
        seeds = [b"claimed", campaign.campaign_id.as_ref(), agent.key().as_ref()],
        bump,
    )]
    pub claim_receipt: Account<'info, ClaimReceipt>,
    /// CHECK: Sysvar instructions account for Ed25519 signature verification.
    #[account(address = ix_sysvar::ID)]
    pub instruction_sysvar: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[account]
#[derive(InitSpace)]
pub struct CampaignState {
    pub advertiser: Pubkey,
    pub campaign_id: [u8; 32],
    pub payout: u64,
    pub remaining_budget: u64,
    pub verification_graph_hash: [u8; 32],
    pub oracle_pubkey: Pubkey,
}

#[account]
#[derive(InitSpace)]
pub struct ClaimReceipt {
    pub campaign_id: [u8; 32],
    pub agent: Pubkey,
    pub claimed_at: i64,
}

#[error_code]
pub enum ZeroAdsError {
    #[msg("Invalid or missing Ed25519 oracle signature")]
    InvalidSignature,
    #[msg("Campaign budget exhausted")]
    CampaignExhausted,
    #[msg("Payout must be greater than zero")]
    PayoutMustBePositive,
    #[msg("Budget must be at least one payout")]
    BudgetTooSmall,
}
