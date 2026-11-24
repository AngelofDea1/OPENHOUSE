use anchor_lang::prelude::*;

#[event]
pub struct BetPlaced {
    pub player: Pubkey,
    pub nonce: u64,
    pub game_id: [u8; 16],
    pub amount: u64,
    pub max_payout: u64,
    pub params: [u8; 32],
    pub client_seed: [u8; 32],
    pub server_seed_hash: [u8; 32],
}

/// Everything needed to replay the result is in this event.
#[event]
pub struct BetSettled {
    pub player: Pubkey,
    pub nonce: u64,
    pub game_id: [u8; 16],
    pub amount: u64,
    pub payout: u64,
    pub params: [u8; 32],
    pub client_seed: [u8; 32],
    pub server_seed: [u8; 32],
    pub server_seed_hash: [u8; 32],
    pub verified_on_chain: bool,
}

#[event]
pub struct BetRefunded {
    pub player: Pubkey,
    pub nonce: u64,
    pub amount: u64,
}
