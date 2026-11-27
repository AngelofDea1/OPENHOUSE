use anchor_lang::prelude::*;

use crate::state::Player;

pub const LIMIT_COOLING_SECONDS: i64 = 86_400;

/// Daily loss limit in lamports, 0 means no limit.
/// Tightening applies at once. Loosening or removing waits 24 hours.
#[derive(Accounts)]
pub struct SetLimits<'info> {
    #[account(mut, seeds = [b"player", user.key().as_ref()], bump = player.bump)]
    pub player: Account<'info, Player>,
    pub user: Signer<'info>,
}

pub fn handler(ctx: Context<SetLimits>, new_limit: u64) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let p = &mut ctx.accounts.player;
    p.apply_pending(now);
    let current = p.daily_loss_limit;
    let tighter = new_limit != 0 && (current == 0 || new_limit < current);
    if tighter {
        p.daily_loss_limit = new_limit;
        p.pending_limit_at = 0;
    } else if new_limit != current {
        p.pending_limit = new_limit;
        p.pending_limit_at = now + LIMIT_COOLING_SECONDS;
    }
    Ok(())
}
