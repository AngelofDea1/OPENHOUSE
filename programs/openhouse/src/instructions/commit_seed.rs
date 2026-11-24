use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::state::{Config, SeedCommitment};

/// The settler publishes sha256(server_seed) before any player can bet against it.
/// `index` lets a stateless settler re-derive the seed later, it reveals nothing.
#[derive(Accounts)]
#[instruction(seed_hash: [u8; 32], index: u64)]
pub struct CommitSeed<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(init, payer = settler, space = 8 + SeedCommitment::INIT_SPACE, seeds = [b"seed_commitment", seed_hash.as_ref()], bump)]
    pub commitment: Account<'info, SeedCommitment>,
    #[account(mut)]
    pub settler: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<CommitSeed>, seed_hash: [u8; 32], index: u64) -> Result<()> {
    require!(ctx.accounts.settler.key() == ctx.accounts.config.settler, OpenHouseError::Unauthorized);
    let c = &mut ctx.accounts.commitment;
    c.settler = ctx.accounts.settler.key();
    c.seed_hash = seed_hash;
    c.index = index;
    c.committed_at = Clock::get()?.unix_timestamp;
    c.used = false;
    c.bump = ctx.bumps.commitment;
    Ok(())
}
