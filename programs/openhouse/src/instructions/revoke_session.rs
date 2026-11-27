use anchor_lang::prelude::*;

use crate::state::Player;

#[derive(Accounts)]
pub struct RevokeSession<'info> {
    #[account(mut, seeds = [b"player", user.key().as_ref()], bump = player.bump)]
    pub player: Account<'info, Player>,
    pub user: Signer<'info>,
}

pub fn handler(ctx: Context<RevokeSession>) -> Result<()> {
    let p = &mut ctx.accounts.player;
    p.session_key = Pubkey::default();
    p.session_expires = 0;
    p.session_max_spend = 0;
    p.session_spent = 0;
    Ok(())
}
