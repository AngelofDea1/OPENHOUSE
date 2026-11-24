use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

use crate::errors::OpenHouseError;
use crate::state::{Player, Vault};

#[derive(Accounts)]
pub struct DepositPlayer<'info> {
    #[account(mut, seeds = [b"vault"], bump = vault.bump)]
    pub vault: Account<'info, Vault>,
    #[account(init_if_needed, payer = user, space = 8 + Player::INIT_SPACE, seeds = [b"player", user.key().as_ref()], bump)]
    pub player: Account<'info, Player>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<DepositPlayer>, amount: u64) -> Result<()> {
    require!(amount > 0, OpenHouseError::AmountTooSmall);
    let p = &mut ctx.accounts.player;
    p.wallet = ctx.accounts.user.key();
    p.bump = ctx.bumps.player;
    p.balance = p.balance.checked_add(amount).ok_or(OpenHouseError::MathOverflow)?;
    transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer { from: ctx.accounts.user.to_account_info(), to: ctx.accounts.vault.to_account_info() },
        ),
        amount,
    )
}
