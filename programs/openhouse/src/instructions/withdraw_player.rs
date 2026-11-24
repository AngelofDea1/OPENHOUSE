use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::state::{Player, Vault};

/// Works while betting is paused. Players can always take their balance out.
#[derive(Accounts)]
pub struct WithdrawPlayer<'info> {
    #[account(mut, seeds = [b"vault"], bump = vault.bump)]
    pub vault: Account<'info, Vault>,
    #[account(mut, seeds = [b"player", user.key().as_ref()], bump = player.bump)]
    pub player: Account<'info, Player>,
    #[account(mut)]
    pub user: Signer<'info>,
}

pub fn handler(ctx: Context<WithdrawPlayer>, amount: u64) -> Result<()> {
    require!(amount > 0, OpenHouseError::AmountTooSmall);
    let p = &mut ctx.accounts.player;
    require!(p.balance >= amount, OpenHouseError::InsufficientPlayerFunds);
    let rent = Rent::get()?.minimum_balance(8 + Vault::INIT_SPACE);
    require!(
        ctx.accounts.vault.to_account_info().lamports().checked_sub(amount).ok_or(OpenHouseError::VaultUnderfunded)? >= rent,
        OpenHouseError::RentExemptViolation
    );
    p.balance -= amount;
    ctx.accounts.vault.sub_lamports(amount)?;
    ctx.accounts.user.add_lamports(amount)?;
    Ok(())
}
