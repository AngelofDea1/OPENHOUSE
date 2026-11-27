use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::state::Config;

/// Stops new bets only. Open bets still settle and balances can always be withdrawn.
#[derive(Accounts)]
pub struct Pause<'info> {
    #[account(mut, seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    pub authority: Signer<'info>,
}

pub fn handler(ctx: Context<Pause>, paused: bool) -> Result<()> {
    require!(ctx.accounts.authority.key() == ctx.accounts.config.authority, OpenHouseError::Unauthorized);
    ctx.accounts.config.paused = paused;
    Ok(())
}
