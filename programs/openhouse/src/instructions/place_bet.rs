use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::events::BetPlaced;
use crate::games::{max_payout as exact_max_payout, top_multiplier_x100, verified_kind};
use crate::state::{Config, OpenBet, Player, SeedCommitment, Vault};

#[derive(Accounts)]
#[instruction(game_id: [u8; 16], amount: u64, max_payout: u64, params: [u8; 32], client_seed: [u8; 32], server_seed_hash: [u8; 32])]
pub struct PlaceBet<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [b"vault"], bump = vault.bump)]
    pub vault: Account<'info, Vault>,
    #[account(mut, seeds = [b"player", player.wallet.as_ref()], bump = player.bump)]
    pub player: Account<'info, Player>,
    #[account(mut, seeds = [b"seed_commitment", server_seed_hash.as_ref()], bump = commitment.bump)]
    pub commitment: Account<'info, SeedCommitment>,
    /// The player wallet, or the session key the wallet registered.
    pub signer: Signer<'info>,
}

#[allow(clippy::too_many_arguments)]
pub fn handler(
    ctx: Context<PlaceBet>,
    game_id: [u8; 16],
    amount: u64,
    max_payout: u64,
    params: [u8; 32],
    client_seed: [u8; 32],
    server_seed_hash: [u8; 32],
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let cfg = &ctx.accounts.config;
    let vault = &mut ctx.accounts.vault;
    let player = &mut ctx.accounts.player;
    let commitment = &mut ctx.accounts.commitment;

    require!(!cfg.paused, OpenHouseError::BettingPaused);
    require!(!player.open.active, OpenHouseError::BetInProgress);
    require!(now >= player.excluded_until, OpenHouseError::SelfExcluded);
    require!(amount >= cfg.min_bet_lamports, OpenHouseError::AmountTooSmall);

    // The seed must be one this settler committed to and nobody has bet against yet.
    require!(commitment.settler == cfg.settler, OpenHouseError::Unauthorized);
    require!(commitment.seed_hash == server_seed_hash, OpenHouseError::InvalidServerSeedHash);
    require!(!commitment.used, OpenHouseError::SeedAlreadyUsed);
    commitment.used = true;

    // Game and payout ceiling. Verified games have an exact max payout, others only a cap.
    let top = top_multiplier_x100(&game_id).ok_or(OpenHouseError::InvalidGameParams)?;
    let cap = u64::try_from((amount as u128).checked_mul(top as u128).ok_or(OpenHouseError::MathOverflow)? / 100)
        .map_err(|_| error!(OpenHouseError::MathOverflow))?;
    let max_payout = match verified_kind(&game_id) {
        Some(kind) => {
            let exact = exact_max_payout(kind, &params, amount)?;
            require!(max_payout == exact && exact <= cap, OpenHouseError::InvalidGameParams);
            exact
        }
        None => {
            require!(max_payout >= amount && max_payout <= cap, OpenHouseError::InvalidGameParams);
            max_payout
        }
    };

    // Session keys act inside a cap, the wallet itself is unlimited.
    if ctx.accounts.signer.key() != player.wallet {
        require!(player.session_key != Pubkey::default() && player.session_key == ctx.accounts.signer.key(), OpenHouseError::Unauthorized);
        require!(now <= player.session_expires, OpenHouseError::SessionKeyExpired);
        let spent = player.session_spent.checked_add(amount).ok_or(OpenHouseError::MathOverflow)?;
        require!(spent <= player.session_max_spend, OpenHouseError::SessionKeyExpired);
        player.session_spent = spent;
    }

    // Daily loss limit.
    player.apply_pending(now);
    if player.daily_loss_limit > 0 {
        let today = now / 86_400;
        if player.daily_loss_day != today {
            player.daily_loss_day = today;
            player.daily_loss_accumulated = 0;
        }
        let worst = player.daily_loss_accumulated.checked_add(amount).ok_or(OpenHouseError::MathOverflow)?;
        require!(worst <= player.daily_loss_limit, OpenHouseError::DailyLossLimitExceeded);
    }

    require!(player.balance >= amount, OpenHouseError::InsufficientPlayerFunds);

    // Solvency: the most the vault can lose on this bet is at most k of what is free.
    let risk = max_payout - amount;
    let free = vault.total_deposited.checked_sub(vault.reserved_payouts).ok_or(OpenHouseError::VaultUnderfunded)?;
    let allowed = ((free as u128) * cfg.k_bps as u128 / 10_000) as u64;
    require!(risk <= allowed, OpenHouseError::MaxBetExceeded);

    player.balance -= amount;
    vault.reserved_payouts = vault.reserved_payouts.checked_add(risk).ok_or(OpenHouseError::MathOverflow)?;
    let nonce = player.nonce;
    player.nonce = nonce.checked_add(1).ok_or(OpenHouseError::MathOverflow)?;
    player.open = OpenBet {
        active: true, game_id, amount, max_payout, params, client_seed, server_seed_hash, nonce,
        created_at: now,
        // multi-step fields (batch3) — zeroed for all games, updated by play_action
        actions: [0u8; 32],
        action_count: 0,
        last_action_at: now,
        extra_stake: 0,
    };

    emit!(BetPlaced { player: player.wallet, nonce, game_id, amount, max_payout, params, client_seed, server_seed_hash });
    Ok(())
}
