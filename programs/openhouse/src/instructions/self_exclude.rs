use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::state::Player;

/// Blocks new bets until the date. It can be extended, never shortened.
/// Withdrawing a balance always stays possible.
#[derive(Accounts)]
pub struct SelfExclude<'info> {
    #[account(mut, seeds = [b"player", user.key().as_ref()], bump = player.bump)]
    pub player: Account<'info, Player>,
    pub user: Signer<'info>,
}

pub fn handler(ctx: Context<SelfExclude>, days: u16) -> Result<()> {
    require!((1..=1825).contains(&days), OpenHouseError::InvalidGameParams);
    let until = Clock::get()?.unix_timestamp.checked_add(days as i64 * 86_400).ok_or(OpenHouseError::MathOverflow)?;
    let p = &mut ctx.accounts.player;
    if until > p.excluded_until {
        p.excluded_until = until;
    }
    Ok(())
}
