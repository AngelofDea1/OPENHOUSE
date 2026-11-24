//! Canonical provably fair scheme, identical in Rust, the settler and the browser.
//!
//!   mac(round) = HMAC_SHA256(key = server_seed (32 bytes),
//!                            msg = client_seed (32 bytes) || nonce (u64 LE) || round (u32 LE))
//!   float i of a round = big endian u32 at bytes [4i, 4i+4) of the mac, divided by 2^32
//!
//! The client seed is SHA256 of the text the player types in the verifier.
use anchor_lang::solana_program::hash::hashv;

pub fn hmac_sha256(key: &[u8; 32], msg: &[u8]) -> [u8; 32] {
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..32 {
        ipad[i] ^= key[i];
        opad[i] ^= key[i];
    }
    let inner = hashv(&[&ipad, msg]).to_bytes();
    hashv(&[&opad, &inner]).to_bytes()
}

pub fn mac(server_seed: &[u8; 32], client_seed: &[u8; 32], nonce: u64, round: u32) -> [u8; 32] {
    let mut msg = [0u8; 44];
    msg[..32].copy_from_slice(client_seed);
    msg[32..40].copy_from_slice(&nonce.to_le_bytes());
    msg[40..44].copy_from_slice(&round.to_le_bytes());
    hmac_sha256(server_seed, &msg)
}

/// The i-th raw u32 (0..8) of round 0.
pub fn first_u32(server_seed: &[u8; 32], client_seed: &[u8; 32], nonce: u64, i: usize) -> u32 {
    let m = mac(server_seed, client_seed, nonce, 0);
    u32::from_be_bytes([m[4 * i], m[4 * i + 1], m[4 * i + 2], m[4 * i + 3]])
}

/// The first `count` (up to 16) u32 draws of the stream: round 0 gives draws 0..8, round 1 gives 8..16.
pub fn draws(server_seed: &[u8; 32], client_seed: &[u8; 32], nonce: u64, count: usize) -> [u32; 16] {
    let mut out = [0u32; 16];
    let rounds = (count + 7) / 8;
    for r in 0..rounds {
        let m = mac(server_seed, client_seed, nonce, r as u32);
        for i in 0..8 {
            out[r * 8 + i] = u32::from_be_bytes([m[4 * i], m[4 * i + 1], m[4 * i + 2], m[4 * i + 3]]);
        }
    }
    out
}
