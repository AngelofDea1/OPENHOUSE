use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::state::Config;

#[derive(Accounts)]
pub struct SetConfig<'info> {
    #[account(mut, seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    pub authority: Signer<'info>,
}

pub fn handler(ctx: Context<SetConfig>, k_bps: u16, protocol_fee_bps: u16, min_bet: u64, fee_wallet: Pubkey, settler: Pubkey) -> Result<()> {
    let c = &mut ctx.accounts.config;
    require!(ctx.accounts.authority.key() == c.authority, OpenHouseError::Unauthorized);
    require!(k_bps > 0 && k_bps <= 1000, OpenHouseError::InvalidGameParams);
    require!(protocol_fee_bps <= 5000, OpenHouseError::InvalidGameParams);
    require!(min_bet > 0, OpenHouseError::InvalidGameParams);
    c.k_bps = k_bps;
    c.protocol_fee_bps = protocol_fee_bps;
    c.min_bet_lamports = min_bet;
    c.fee_wallet = fee_wallet;
    c.settler = settler;
    Ok(())
}
