use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::state::{Config, Vault};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = 8 + Config::INIT_SPACE, seeds = [b"config"], bump)]
    pub config: Account<'info, Config>,
    #[account(init, payer = authority, space = 8 + Vault::INIT_SPACE, seeds = [b"vault"], bump)]
    pub vault: Account<'info, Vault>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Initialize>, k_bps: u16, protocol_fee_bps: u16, min_bet: u64, settler: Pubkey) -> Result<()> {
    require!(k_bps > 0 && k_bps <= 1000, OpenHouseError::InvalidGameParams);
    require!(protocol_fee_bps <= 5000, OpenHouseError::InvalidGameParams);
    require!(min_bet > 0, OpenHouseError::InvalidGameParams);
    let c = &mut ctx.accounts.config;
    c.authority = ctx.accounts.authority.key();
    c.pending_authority = Pubkey::default();
    c.settler = settler;
    c.fee_wallet = ctx.accounts.authority.key();
    c.k_bps = k_bps;
    c.protocol_fee_bps = protocol_fee_bps;
    c.min_bet_lamports = min_bet;
    c.paused = false;
    c.bump = ctx.bumps.config;
    let v = &mut ctx.accounts.vault;
    v.total_shares = 0;
    v.total_deposited = 0;
    v.reserved_payouts = 0;
    v.accrued_fees = 0;
    v.net_pnl = 0;
    v.fee_hwm = 0;
    v.bump = ctx.bumps.vault;
    Ok(())
}
