use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::events::BetRefunded;
use crate::games::is_multistep;
use crate::state::{Config, Player, Vault};

/// If a bet is not settled in this long the player gets the stake back.
pub const BET_REFUND_TIMEOUT_SECONDS: i64 = 120;

#[derive(Accounts)]
pub struct RefundBet<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [b"vault"], bump = vault.bump)]
    pub vault: Account<'info, Vault>,
    #[account(mut, seeds = [b"player", player.wallet.as_ref()], bump = player.bump)]
    pub player: Account<'info, Player>,
    pub caller: Signer<'info>,
}

pub fn handler(ctx: Context<RefundBet>) -> Result<()> {
    let cfg = &ctx.accounts.config;
    let caller = ctx.accounts.caller.key();
    let player = &mut ctx.accounts.player;
    let vault = &mut ctx.accounts.vault;
    require!(caller == player.wallet || caller == cfg.settler || caller == cfg.authority, OpenHouseError::Unauthorized);
    let o = player.open;
    require!(o.active, OpenHouseError::NoOpenBet);

    let now = Clock::get()?.unix_timestamp;

    // For multi-step games where the player has already submitted actions (action_count > 0),
    // the refund path is closed. The player is protected by force_settle instead.
    // The timeout for force_settle is measured from last_action_at (600 s), not created_at.
    if is_multistep(&o.game_id) && o.action_count > 0 {
        // Refuse refund — use force_settle after the ROUND_TIMEOUT.
        return Err(error!(OpenHouseError::RefundTimeoutNotReached));
    }

    // For multi-step games with no actions yet, the 120 s timeout is from created_at (normal path).
    // For single-step games, same as before.
    require!(
        now >= o.created_at.checked_add(BET_REFUND_TIMEOUT_SECONDS).ok_or(OpenHouseError::MathOverflow)?,
        OpenHouseError::RefundTimeoutNotReached
    );

    vault.reserved_payouts = vault.reserved_payouts
        .checked_sub(o.max_payout - (o.amount + o.extra_stake))
        .ok_or(OpenHouseError::MathOverflow)?;
    // Refund the total stake (original + any double).
    let total_stake = o.amount + o.extra_stake;
    player.balance = player.balance.checked_add(total_stake).ok_or(OpenHouseError::MathOverflow)?;
    player.open = Default::default();
    emit!(BetRefunded { player: player.wallet, nonce: o.nonce, amount: total_stake });
    Ok(())
}
