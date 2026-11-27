use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hash;

use crate::errors::OpenHouseError;
use crate::events::BetSettled;
use crate::games::{is_multistep, replay};
use crate::instructions::play_action::ROUND_TIMEOUT;
use crate::instructions::refund_bet::BET_REFUND_TIMEOUT_SECONDS;
use crate::state::{Config, Player, SeedCommitment, Vault};

/// Anyone may call force_settle when a multi-step round has not seen any action
/// for ROUND_TIMEOUT (600s) seconds. Because it still needs the server seed,
/// it stays settler-signed and is the happy path for abandoned rounds.
/// If the settler never acts, refund_bet after ROUND_REFUND_TIMEOUT (3600s) is the safety net.
#[derive(Accounts)]
pub struct ForceSettle<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [b"vault"], bump = vault.bump)]
    pub vault: Account<'info, Vault>,
    #[account(mut, seeds = [b"player", player.wallet.as_ref()], bump = player.bump)]
    pub player: Account<'info, Player>,
    /// Closed on success; rent goes back to the settler.
    #[account(mut, close = settler, seeds = [b"seed_commitment", player.open.server_seed_hash.as_ref()], bump = commitment.bump)]
    pub commitment: Account<'info, SeedCommitment>,
    #[account(mut)]
    pub settler: Signer<'info>,
    /// Anyone can be the caller but the settler must sign (they provide the seed).
    pub caller: Signer<'info>,
}

pub fn handler(ctx: Context<ForceSettle>, server_seed: [u8; 32]) -> Result<()> {
    let cfg = &ctx.accounts.config;
    require!(ctx.accounts.settler.key() == cfg.settler, OpenHouseError::Unauthorized);

    let now = Clock::get()?.unix_timestamp;
    let vault = &mut ctx.accounts.vault;
    let player = &mut ctx.accounts.player;
    let o = player.open;

    require!(o.active, OpenHouseError::NoOpenBet);
    require!(is_multistep(&o.game_id), OpenHouseError::InvalidGameParams);
    require!(hash(&server_seed).to_bytes() == o.server_seed_hash, OpenHouseError::InvalidServerSeedHash);

    // Timeout measured from last_action_at (or created_at if no actions logged yet).
    let last = if o.action_count == 0 { o.created_at } else { o.last_action_at };
    // Also allow force_settle after BET_REFUND_TIMEOUT_SECONDS if no actions (same as refund but settler reveals seed).
    let timeout = if o.action_count == 0 { BET_REFUND_TIMEOUT_SECONDS } else { ROUND_TIMEOUT };
    require!(
        now >= last.checked_add(timeout).ok_or(OpenHouseError::MathOverflow)?,
        OpenHouseError::ForceSettleTimeoutNotReached
    );

    // Replay with the auto-cashout / auto-stand timeout rule.
    // append_cashout = true so the replay knows to auto-terminate.
    let outcome = replay(
        &o.game_id, &o.params, o.amount,
        &server_seed, &o.client_seed, o.nonce,
        &o.actions, o.action_count, o.extra_stake,
        true, // timeout auto-cashout
    )?;
    let payout = outcome.payout;
    require!(payout <= o.max_payout, OpenHouseError::InvalidGameParams);

    // If the double was done the reserved_payouts was updated at play_action time.
    let risk = o.max_payout - (o.amount + o.extra_stake);
    vault.reserved_payouts = vault.reserved_payouts.checked_sub(risk).ok_or(OpenHouseError::MathOverflow)?;
    player.balance = player.balance.checked_add(payout).ok_or(OpenHouseError::MathOverflow)?;

    let total_stake = o.amount + o.extra_stake;
    let delta = total_stake as i128 - payout as i128;
    let td = vault.total_deposited as i128 + delta;
    require!(td >= 0, OpenHouseError::VaultUnderfunded);
    vault.total_deposited = td as u64;
    vault.net_pnl = i64::try_from(vault.net_pnl as i128 + delta).map_err(|_| error!(OpenHouseError::MathOverflow))?;

    if vault.net_pnl > vault.fee_hwm {
        let gain = (vault.net_pnl - vault.fee_hwm) as u128;
        let fee = ((gain * cfg.protocol_fee_bps as u128 / 10_000) as u64).min(vault.total_deposited);
        vault.total_deposited -= fee;
        vault.accrued_fees = vault.accrued_fees.checked_add(fee).ok_or(OpenHouseError::MathOverflow)?;
        vault.fee_hwm = vault.net_pnl;
    }

    if delta > 0 && player.daily_loss_limit > 0 {
        player.daily_loss_accumulated = player.daily_loss_accumulated.saturating_add(delta as u64);
    }
    player.open = Default::default();

    emit!(BetSettled {
        player: player.wallet,
        nonce: o.nonce,
        game_id: o.game_id,
        amount: o.amount,
        payout,
        params: o.params,
        client_seed: o.client_seed,
        server_seed,
        server_seed_hash: o.server_seed_hash,
        verified_on_chain: true,
    });
    Ok(())
}
