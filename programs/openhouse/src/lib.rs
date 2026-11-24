use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod games;
pub mod instructions;
pub mod state;

pub use instructions::*;

declare_id!("Ehd64oouV9Avs6HkkkyNcw74x8WBggLHRx8EBrXnAq91");

#[program]
pub mod openhouse {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, k_bps: u16, protocol_fee_bps: u16, min_bet: u64, settler: Pubkey) -> Result<()> {
        instructions::initialize::handler(ctx, k_bps, protocol_fee_bps, min_bet, settler)
    }
    pub fn deposit_vault(ctx: Context<DepositVault>, amount: u64) -> Result<()> {
        instructions::deposit_vault::handler(ctx, amount)
    }
    pub fn withdraw_vault(ctx: Context<WithdrawVault>, shares: u64) -> Result<()> {
        instructions::withdraw_vault::handler(ctx, shares)
    }
    pub fn deposit_player(ctx: Context<DepositPlayer>, amount: u64) -> Result<()> {
        instructions::deposit_player::handler(ctx, amount)
    }
    pub fn withdraw_player(ctx: Context<WithdrawPlayer>, amount: u64) -> Result<()> {
        instructions::withdraw_player::handler(ctx, amount)
    }
    pub fn create_session(ctx: Context<CreateSession>, session_key: Pubkey, duration_seconds: i64, max_spend: u64) -> Result<()> {
        instructions::create_session::handler(ctx, session_key, duration_seconds, max_spend)
    }
    pub fn revoke_session(ctx: Context<RevokeSession>) -> Result<()> {
        instructions::revoke_session::handler(ctx)
    }
    pub fn set_limits(ctx: Context<SetLimits>, new_limit: u64) -> Result<()> {
        instructions::set_limits::handler(ctx, new_limit)
    }
    pub fn self_exclude(ctx: Context<SelfExclude>, days: u16) -> Result<()> {
        instructions::self_exclude::handler(ctx, days)
    }
    pub fn commit_seed(ctx: Context<CommitSeed>, seed_hash: [u8; 32], index: u64) -> Result<()> {
        instructions::commit_seed::handler(ctx, seed_hash, index)
    }
    pub fn place_bet(
        ctx: Context<PlaceBet>,
        game_id: [u8; 16],
        amount: u64,
        max_payout: u64,
        params: [u8; 32],
        client_seed: [u8; 32],
        server_seed_hash: [u8; 32],
    ) -> Result<()> {
        instructions::place_bet::handler(ctx, game_id, amount, max_payout, params, client_seed, server_seed_hash)
    }
    pub fn settle_bet(ctx: Context<SettleBet>, server_seed: [u8; 32], payout_hint: u64) -> Result<()> {
        instructions::settle_bet::handler(ctx, server_seed, payout_hint)
    }
    pub fn refund_bet(ctx: Context<RefundBet>) -> Result<()> {
        instructions::refund_bet::handler(ctx)
    }
    pub fn reclaim_commitment(ctx: Context<ReclaimCommitment>) -> Result<()> {
        instructions::reclaim_commitment::handler(ctx)
    }
    pub fn pause(ctx: Context<Pause>, paused: bool) -> Result<()> {
        instructions::pause::handler(ctx, paused)
    }
    pub fn set_config(ctx: Context<SetConfig>, k_bps: u16, protocol_fee_bps: u16, min_bet: u64, fee_wallet: Pubkey, settler: Pubkey) -> Result<()> {
        instructions::set_config::handler(ctx, k_bps, protocol_fee_bps, min_bet, fee_wallet, settler)
    }
    pub fn propose_authority(ctx: Context<ProposeAuthority>, new_authority: Pubkey) -> Result<()> {
        instructions::propose_authority::handler(ctx, new_authority)
    }
    pub fn accept_authority(ctx: Context<AcceptAuthority>) -> Result<()> {
        instructions::accept_authority::handler(ctx)
    }
    pub fn withdraw_fees(ctx: Context<WithdrawFees>, amount: u64) -> Result<()> {
        instructions::withdraw_fees::handler(ctx, amount)
    }
}
