use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::state::{Vault, VaultShare};

#[derive(Accounts)]
pub struct WithdrawVault<'info> {
    #[account(mut, seeds = [b"vault"], bump = vault.bump)]
    pub vault: Account<'info, Vault>,
    #[account(mut, seeds = [b"vault_share", depositor.key().as_ref()], bump = vault_share.bump)]
    pub vault_share: Account<'info, VaultShare>,
    #[account(mut)]
    pub depositor: Signer<'info>,
}

pub fn handler(ctx: Context<WithdrawVault>, shares: u64) -> Result<()> {
    let vault = &mut ctx.accounts.vault;
    let share = &mut ctx.accounts.vault_share;
    require!(shares > 0 && vault.total_shares > 0, OpenHouseError::AmountTooSmall);
    require!(share.shares >= shares, OpenHouseError::InsufficientPlayerFunds);
    let payout = u64::try_from(
        (shares as u128).checked_mul(vault.total_deposited as u128).ok_or(OpenHouseError::MathOverflow)? / vault.total_shares as u128,
    )
    .map_err(|_| error!(OpenHouseError::MathOverflow))?;
    let free = vault.total_deposited.checked_sub(vault.reserved_payouts).ok_or(OpenHouseError::VaultUnderfunded)?;
    require!(payout <= free, OpenHouseError::VaultUnderfunded);
    let rent = Rent::get()?.minimum_balance(8 + Vault::INIT_SPACE);
    require!(
        vault.to_account_info().lamports().checked_sub(payout).ok_or(OpenHouseError::VaultUnderfunded)? >= rent,
        OpenHouseError::RentExemptViolation
    );
    share.shares -= shares;
    vault.total_shares -= shares;
    vault.total_deposited -= payout;
    ctx.accounts.vault.sub_lamports(payout)?;
    ctx.accounts.depositor.add_lamports(payout)?;
    Ok(())
}
