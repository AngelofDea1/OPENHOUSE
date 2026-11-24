use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::state::Config;

#[derive(Accounts)]
pub struct AcceptAuthority<'info> {
    #[account(mut, seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    pub new_authority: Signer<'info>,
}

pub fn handler(ctx: Context<AcceptAuthority>) -> Result<()> {
    let c = &mut ctx.accounts.config;
    require!(c.pending_authority != Pubkey::default() && ctx.accounts.new_authority.key() == c.pending_authority, OpenHouseError::Unauthorized);
    c.authority = c.pending_authority;
    c.pending_authority = Pubkey::default();
    Ok(())
}
