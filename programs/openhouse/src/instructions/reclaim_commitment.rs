use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::state::{Config, SeedCommitment};

/// Settler takes back the rent of a commitment that was used, or never used for a day.
#[derive(Accounts)]
pub struct ReclaimCommitment<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut, close = settler, seeds = [b"seed_commitment", commitment.seed_hash.as_ref()], bump = commitment.bump)]
    pub commitment: Account<'info, SeedCommitment>,
    #[account(mut)]
    pub settler: Signer<'info>,
}

pub fn handler(ctx: Context<ReclaimCommitment>) -> Result<()> {
    require!(ctx.accounts.settler.key() == ctx.accounts.config.settler, OpenHouseError::Unauthorized);
    let c = &ctx.accounts.commitment;
    let stale = Clock::get()?.unix_timestamp >= c.committed_at.saturating_add(86_400);
    require!(c.used || stale, OpenHouseError::CommitmentNotReclaimable);
    Ok(())
}
