use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;
use crate::state::Config;

/// Two step handover, so a typo cannot lock the program. Point it at a multisig.
#[derive(Accounts)]
pub struct ProposeAuthority<'info> {
    #[account(mut, seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    pub authority: Signer<'info>,
}

pub fn handler(ctx: Context<ProposeAuthority>, new_authority: Pubkey) -> Result<()> {
    require!(ctx.accounts.authority.key() == ctx.accounts.config.authority, OpenHouseError::Unauthorized);
    ctx.accounts.config.pending_authority = new_authority;
    Ok(())
}
