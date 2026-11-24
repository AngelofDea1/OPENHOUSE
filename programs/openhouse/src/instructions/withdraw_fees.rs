use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::state::{Config, Vault};

/// Only ever pays out `accrued_fees`, never depositor value or player balances.
#[derive(Accounts)]
pub struct WithdrawFees<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [b"vault"], bump = vault.bump)]
    pub vault: Account<'info, Vault>,
    #[account(mut, address = config.fee_wallet)]
    pub fee_wallet: SystemAccount<'info>,
    pub authority: Signer<'info>,
}

pub fn handler(ctx: Context<WithdrawFees>, amount: u64) -> Result<()> {
    require!(ctx.accounts.authority.key() == ctx.accounts.config.authority, OpenHouseError::Unauthorized);
    require!(amount > 0, OpenHouseError::AmountTooSmall);
    let vault = &mut ctx.accounts.vault;
    require!(amount <= vault.accrued_fees, OpenHouseError::VaultUnderfunded);
    let rent = Rent::get()?.minimum_balance(8 + Vault::INIT_SPACE);
    require!(
        vault.to_account_info().lamports().checked_sub(amount).ok_or(OpenHouseError::VaultUnderfunded)? >= rent,
        OpenHouseError::RentExemptViolation
    );
    vault.accrued_fees -= amount;
    ctx.accounts.vault.sub_lamports(amount)?;
    ctx.accounts.fee_wallet.add_lamports(amount)?;
    Ok(())
}
