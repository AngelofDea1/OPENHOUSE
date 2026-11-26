use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::games::{is_multistep, validate_action_shape};
use crate::state::{Config, Player, Vault};

/// Timeout after which force_settle is allowed for multi-step games.
pub const ROUND_TIMEOUT: i64 = 600; // 10 minutes

#[derive(Accounts)]
pub struct PlayAction<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [b"vault"], bump = vault.bump)]
    pub vault: Account<'info, Vault>,
    #[account(mut, seeds = [b"player", player.wallet.as_ref()], bump = player.bump)]
    pub player: Account<'info, Player>,
    /// The player wallet, or a valid session key the wallet registered.
    pub signer: Signer<'info>,
}

pub fn handler(ctx: Context<PlayAction>, action: u8) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let cfg = &ctx.accounts.config;
    let vault = &mut ctx.accounts.vault;
    let player = &mut ctx.accounts.player;

    require!(player.open.active, OpenHouseError::NoOpenBet);
    require!(is_multistep(&player.open.game_id), OpenHouseError::InvalidGameParams);
    require!(player.open.action_count < 32, OpenHouseError::ActionLogFull);

    // Auth: player wallet OR a valid session key (same rules as place_bet).
    if ctx.accounts.signer.key() != player.wallet {
        require!(
            player.session_key != Pubkey::default()
                && player.session_key == ctx.accounts.signer.key(),
            OpenHouseError::Unauthorized
        );
        require!(now <= player.session_expires, OpenHouseError::SessionKeyExpired);
        // play_action does not spend extra for session cap purposes (the bet was already debited)
    }

    // Basic action shape validation (range check only — safety checked at settle).
    validate_action_shape(&player.open.game_id, &player.open.params, action, &player.open.actions, player.open.action_count)?;

    // Blackjack double: action 2, only valid as first action.
    // Debit the extra stake from the player balance and credit the vault now.
    if crate::games::is_blackjack(&player.open.game_id) && action == 2 {
        require!(player.open.action_count == 0, OpenHouseError::InvalidAction);
        require!(player.open.extra_stake == 0, OpenHouseError::InvalidAction);
        let extra = player.open.amount;
        require!(player.balance >= extra, OpenHouseError::InsufficientPlayerFunds);
        // Session key spend cap check for the double amount.
        if ctx.accounts.signer.key() != player.wallet {
            let spent = player.session_spent.checked_add(extra).ok_or(OpenHouseError::MathOverflow)?;
            require!(spent <= player.session_max_spend, OpenHouseError::SessionKeyExpired);
            player.session_spent = spent;
        }
        player.balance -= extra;
        // Reserve the extra payout (double: max is 2*(amount+extra) = 4*amount).
        let new_max = player.open.max_payout.max((player.open.amount + extra) * 2);
        let old_risk = player.open.max_payout - player.open.amount;
        let new_risk = new_max - (player.open.amount + extra);
        if new_risk >= old_risk {
            let extra_risk = new_risk - old_risk;
            let free = vault.total_deposited.checked_sub(vault.reserved_payouts).ok_or(OpenHouseError::VaultUnderfunded)?;
            let allowed = ((free as u128) * cfg.k_bps as u128 / 10_000) as u64;
            require!(extra_risk <= allowed, OpenHouseError::MaxBetExceeded);
            vault.reserved_payouts = vault.reserved_payouts.checked_add(extra_risk).ok_or(OpenHouseError::MathOverflow)?;
        } else {
            vault.reserved_payouts = vault.reserved_payouts.checked_sub(old_risk - new_risk).ok_or(OpenHouseError::MathOverflow)?;
        }
        // Update max_payout to reflect the double.
        player.open.extra_stake = extra;
        player.open.max_payout = new_max;
    }

    let idx = player.open.action_count as usize;
    player.open.actions[idx] = action;
    player.open.action_count += 1;
    player.open.last_action_at = now;

    Ok(())
}
