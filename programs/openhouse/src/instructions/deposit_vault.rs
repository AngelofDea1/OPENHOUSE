use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

use crate::errors::OpenHouseError;
use crate::state::{Vault, VaultShare};

#[derive(Accounts)]
pub struct DepositVault<'info> {
    #[account(mut, seeds = [b"vault"], bump = vault.bump)]
    pub vault: Account<'info, Vault>,
    #[account(init_if_needed, payer = depositor, space = 8 + VaultShare::INIT_SPACE, seeds = [b"vault_share", depositor.key().as_ref()], bump)]
    pub vault_share: Account<'info, VaultShare>,
    #[account(mut)]
    pub depositor: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<DepositVault>, amount: u64) -> Result<()> {
    require!(amount > 0, OpenHouseError::AmountTooSmall);
    let vault = &mut ctx.accounts.vault;
    // Shares outstanding with nothing behind them would hand the new money to old holders.
    require!(!(vault.total_deposited == 0 && vault.total_shares > 0), OpenHouseError::VaultDepleted);
    let minted: u64 = if vault.total_shares == 0 {
        amount
    } else {
        u64::try_from((amount as u128).checked_mul(vault.total_shares as u128).ok_or(OpenHouseError::MathOverflow)? / vault.total_deposited as u128)
            .map_err(|_| error!(OpenHouseError::MathOverflow))?
    };
    require!(minted > 0, OpenHouseError::AmountTooSmall);
    vault.total_shares = vault.total_shares.checked_add(minted).ok_or(OpenHouseError::MathOverflow)?;
    vault.total_deposited = vault.total_deposited.checked_add(amount).ok_or(OpenHouseError::MathOverflow)?;
    let s = &mut ctx.accounts.vault_share;
    s.owner = ctx.accounts.depositor.key();
    s.shares = s.shares.checked_add(minted).ok_or(OpenHouseError::MathOverflow)?;
    s.bump = ctx.bumps.vault_share;
    transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer { from: ctx.accounts.depositor.to_account_info(), to: ctx.accounts.vault.to_account_info() },
        ),
        amount,
    )
}
