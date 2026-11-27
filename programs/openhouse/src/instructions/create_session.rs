use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::state::Player;

/// A session key can place bets inside a spend cap until it expires. It can never withdraw.
#[derive(Accounts)]
pub struct CreateSession<'info> {
    #[account(mut, seeds = [b"player", user.key().as_ref()], bump = player.bump)]
    pub player: Account<'info, Player>,
    pub user: Signer<'info>,
}

pub fn handler(ctx: Context<CreateSession>, session_key: Pubkey, duration_seconds: i64, max_spend: u64) -> Result<()> {
    require!((60..=7 * 86_400).contains(&duration_seconds), OpenHouseError::InvalidGameParams);
    require!(max_spend > 0, OpenHouseError::InvalidGameParams);
    require!(session_key != ctx.accounts.user.key() && session_key != Pubkey::default(), OpenHouseError::InvalidGameParams);
    let p = &mut ctx.accounts.player;
    p.session_key = session_key;
    p.session_expires = Clock::get()?.unix_timestamp.checked_add(duration_seconds).ok_or(OpenHouseError::MathOverflow)?;
    p.session_max_spend = max_spend;
    p.session_spent = 0;
    Ok(())
}
