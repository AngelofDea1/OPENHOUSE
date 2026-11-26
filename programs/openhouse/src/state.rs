use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub authority: Pubkey,
    pub pending_authority: Pubkey,
    pub settler: Pubkey,
    pub fee_wallet: Pubkey,
    pub k_bps: u16,            // max risk per bet as share of free vault, 100 = 1%
    pub protocol_fee_bps: u16, // protocol share of net edge, 2000 = 20%
    pub min_bet_lamports: u64,
    pub paused: bool,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Vault {
    pub total_shares: u64,
    pub total_deposited: u64,  // depositor value, excludes fees and player balances
    pub reserved_payouts: u64, // worst-case profit owed on open bets
    pub accrued_fees: u64,
    pub net_pnl: i64,          // cumulative house result of all bets
    pub fee_hwm: i64,          // net_pnl level fees were last charged at
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct VaultShare {
    pub owner: Pubkey,
    pub shares: u64,
    pub bump: u8,
}

/// Multi-step games append player decisions here before the seed is revealed.
/// Max 32 actions; action 255 means cash out in Mines/HiLo/Towers.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default, InitSpace)]
pub struct OpenBet {
    pub active: bool,
    pub game_id: [u8; 16],
    pub amount: u64,
    pub max_payout: u64,
    pub params: [u8; 32],
    pub client_seed: [u8; 32],
    pub server_seed_hash: [u8; 32],
    pub nonce: u64,
    pub created_at: i64,
    // --- multi-step fields (batch3) ---
    /// Logged player actions. Valid indices are 0..action_count.
    pub actions: [u8; 32],
    /// How many actions are recorded.
    pub action_count: u8,
    /// Timestamp of the last play_action call (or created_at before any action).
    pub last_action_at: i64,
    /// Extra stake added by doubling (Blackjack). 0 for all other games.
    pub extra_stake: u64,
}

#[account]
#[derive(InitSpace)]
pub struct Player {
    pub wallet: Pubkey,
    pub balance: u64,
    pub nonce: u64,
    pub session_key: Pubkey, // all zeros means no session
    pub session_expires: i64,
    pub session_max_spend: u64,
    pub session_spent: u64,
    pub daily_loss_limit: u64,
    pub daily_loss_day: i64,
    pub daily_loss_accumulated: u64,
    pub pending_limit: u64,
    pub pending_limit_at: i64,
    pub excluded_until: i64,
    pub open: OpenBet,
    pub bump: u8,
}

impl Player {
    /// Apply a queued limit change once its cooling period has passed.
    pub fn apply_pending(&mut self, now: i64) {
        if self.pending_limit_at != 0 && now >= self.pending_limit_at {
            self.daily_loss_limit = self.pending_limit;
            self.pending_limit_at = 0;
        }
    }
}

#[account]
#[derive(InitSpace)]
pub struct SeedCommitment {
    pub settler: Pubkey,
    pub seed_hash: [u8; 32],
    pub index: u64,
    pub committed_at: i64,
    pub used: bool,
    pub bump: u8,
}

#[cfg(test)]
mod layout {
    use super::*;
    // Fixed sizes the TypeScript decoders rely on. Change them together.
    #[test]
    fn sizes() {
        assert_eq!(Config::INIT_SPACE, 142);
        assert_eq!(Vault::INIT_SPACE, 49);
        assert_eq!(VaultShare::INIT_SPACE, 41);
        // OpenBet: bool(1)+[u8;16](16)+u64(8)+u64(8)+[u8;32](32)+[u8;32](32)+[u8;32](32)+u64(8)+i64(8)
        //        + [u8;32](32)+u8(1)+i64(8)+u64(8) = 194
        assert_eq!(OpenBet::INIT_SPACE, 194);
        // Player: Pubkey(32)+u64(8)+u64(8)+Pubkey(32)+i64(8)+u64(8)+u64(8)+u64(8)+i64(8)+u64(8)
        //       + u64(8)+i64(8)+i64(8)+OpenBet(194)+u8(1) = 347
        assert_eq!(Player::INIT_SPACE, 347);
        assert_eq!(SeedCommitment::INIT_SPACE, 82);
    }
}
