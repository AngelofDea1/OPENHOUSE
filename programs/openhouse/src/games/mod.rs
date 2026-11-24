use anchor_lang::prelude::*;

use crate::errors::OpenHouseError;

pub mod fair;
pub mod tables;

/// Games the program can fully verify on chain. Every other game is settled by the
/// settler inside a hard cap (see top_multiplier_x100) until it is ported.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Dice,
    Flip,
    Limbo,
    Wheel,
    Plinko,
    Keno,
    Roulette,
    HotReels,
    Slots777,
    VaultBreak,
    Scratch,
    SicBo,
    Dragon,
    Baccarat,
}

fn name(game: &[u8; 16]) -> &[u8] {
    let n = game.iter().position(|b| *b == 0).unwrap_or(16);
    &game[..n]
}

pub fn verified_kind(game: &[u8; 16]) -> Option<Kind> {
    match name(game) {
        b"dice" => Some(Kind::Dice),
        b"flip" => Some(Kind::Flip),
        b"limbo" | b"crash" => Some(Kind::Limbo),
        b"wheel" => Some(Kind::Wheel),
        b"plinko" => Some(Kind::Plinko),
        b"keno" => Some(Kind::Keno),
        b"roulette" => Some(Kind::Roulette),
        b"hotreels" => Some(Kind::HotReels),
        b"slots777" => Some(Kind::Slots777),
        b"vaultbreak" => Some(Kind::VaultBreak),
        b"scratch" => Some(Kind::Scratch),
        b"sicbo" => Some(Kind::SicBo),
        b"dragon" => Some(Kind::Dragon),
        b"baccarat" => Some(Kind::Baccarat),
        _ => None,
    }
}

/// Highest total payout per stake, times 100. None means the game is unknown.
pub fn top_multiplier_x100(game: &[u8; 16]) -> Option<u64> {
    Some(match name(game) {
        b"dice" => 4950,
        b"flip" => 198,
        b"limbo" | b"crash" => 100_000,
        b"mines" => 100_000,
        b"plinko" => 100_000,
        b"hilo" => 100_000,
        b"towers" => 100_000,
        b"keno" => 50_000,
        b"wheel" => 625,
        b"roulette" => 3_600,
        b"blackjack" => 500,
        b"baccarat" => 900,
        b"poker" => 80_000,
        b"dragon" => 1_280,
        b"sicbo" => 1_200,
        b"slots777" => 50_000,
        b"hotreels" => 27_500,
        b"vaultbreak" => 65_000,
        b"scratch" => 20_000,
        _ => return None,
    })
}

// ---- parameters (fixed byte layout, shared with the settler and the browser) ----
// dice : [0] 0 = roll over, 1 = roll under   [1..3] target in basis points, u16 LE, 200..=9800
// flip : [0] 0 = heads, 1 = tails
// limbo: [0..4] target multiplier x100, u32 LE, 101..=100000

pub const DICE_MIN_BP: u16 = 200;
pub const DICE_MAX_BP: u16 = 9800;
pub const LIMBO_MIN_X100: u32 = 101;
pub const LIMBO_MAX_X100: u32 = 100_000;

fn dice_params(p: &[u8; 32]) -> Result<(bool, u16)> {
    require!(p[0] <= 1, OpenHouseError::InvalidGameParams);
    let t = u16::from_le_bytes([p[1], p[2]]);
    require!((DICE_MIN_BP..=DICE_MAX_BP).contains(&t), OpenHouseError::InvalidGameParams);
    Ok((p[0] == 1, t))
}

fn limbo_target(p: &[u8; 32]) -> Result<u32> {
    let t = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
    require!((LIMBO_MIN_X100..=LIMBO_MAX_X100).contains(&t), OpenHouseError::InvalidGameParams);
    Ok(t)
}

/// What a winning bet pays in total (stake included).
pub fn win_payout(kind: Kind, params: &[u8; 32], amount: u64) -> Result<u64> {
    let a = amount as u128;
    let v = match kind {
        Kind::Dice => {
            let (_, t) = dice_params(params)?;
            a * 9900 / t as u128
        }
        Kind::Flip => {
            require!(params[0] <= 1, OpenHouseError::InvalidGameParams);
            a * 198 / 100
        }
        Kind::Limbo => {
            let t = limbo_target(params)?;
            a * t as u128 / 100
        }
        _ => return Err(error!(OpenHouseError::InvalidGameParams)),
    };
    u64::try_from(v).map_err(|_| error!(OpenHouseError::MathOverflow))
}

/// Did the bet win, given the first u32 of the HMAC stream.
pub fn is_win(kind: Kind, params: &[u8; 32], u: u32) -> Result<bool> {
    Ok(match kind {
        Kind::Dice => {
            let (under, t) = dice_params(params)?;
            let roll = ((u as u64) * 10_000) >> 32; // floor(float * 10000), 0..=9999
            if under {
                roll < t as u64
            } else {
                roll >= 10_000 - t as u64
            }
        }
        Kind::Flip => {
            let tails = u >= 0x8000_0000;
            tails == (params[0] == 1)
        }
        Kind::Limbo => {
            let t = limbo_target(params)?;
            let crash_x100 = (99u128 << 32) / ((1u128 << 32) - u as u128);
            crash_x100 >= t as u128
        }
        _ => return Err(error!(OpenHouseError::InvalidGameParams)),
    })
}


// ---- multi outcome games: wheel, plinko, keno, roulette ----
// wheel   : [0] risk 0 low, 1 medium, 2 high
// plinko  : [0] rows (8, 10, 12, 14 or 16)   [1] risk 0 low, 1 medium, 2 high
// keno    : [0..8] u64 LE bit mask of picks, number n is bit n-1, one to ten picks from 1..=40
// roulette: up to ten entries of 3 bytes, [slot u8, units u16 LE], units 0 ends the list.
//           slots 0..=36 straight number, 37 red, 38 black, 39 odd, 40 even, 41 low, 42 high,
//           43 to 45 dozens, 46 to 48 columns. One unit is 0.001 SOL and the stake must equal the units exactly.

pub const ROULETTE_UNIT: u64 = 1_000_000;
pub const ROULETTE_MAX_ENTRIES: usize = 10;
const RED: [u8; 18] = [1, 3, 5, 7, 9, 12, 14, 16, 18, 19, 21, 23, 25, 27, 30, 32, 34, 36];

/// How many total-payout multiples of the unit a roulette slot pays when number n lands.
pub fn roulette_mult(slot: u8, n: u8) -> u64 {
    if slot <= 36 {
        return if slot == n { 36 } else { 0 };
    }
    if n == 0 {
        return 0;
    }
    let red = RED.contains(&n);
    let hit = match slot {
        37 => red,
        38 => !red,
        39 => n % 2 == 1,
        40 => n % 2 == 0,
        41 => n <= 18,
        42 => n >= 19,
        43 => n <= 12,
        44 => n > 12 && n <= 24,
        45 => n > 24,
        46 => n % 3 == 1,
        47 => n % 3 == 2,
        48 => n % 3 == 0,
        _ => false,
    };
    if !hit {
        0
    } else if slot <= 42 {
        2
    } else {
        3
    }
}

fn roulette_entries(p: &[u8; 32], amount: u64) -> Result<([(u8, u64); ROULETTE_MAX_ENTRIES], usize)> {
    let mut out = [(0u8, 0u64); ROULETTE_MAX_ENTRIES];
    let mut n = 0usize;
    let mut total: u64 = 0;
    let mut seen: u64 = 0;
    for i in 0..ROULETTE_MAX_ENTRIES {
        let slot = p[3 * i];
        let units = u16::from_le_bytes([p[3 * i + 1], p[3 * i + 2]]) as u64;
        if units == 0 {
            break;
        }
        require!(slot <= 48 && seen & (1u64 << slot) == 0, OpenHouseError::InvalidGameParams);
        seen |= 1u64 << slot;
        out[n] = (slot, units);
        n += 1;
        total += units;
    }
    require!(n > 0 && total.checked_mul(ROULETTE_UNIT) == Some(amount), OpenHouseError::InvalidGameParams);
    Ok((out, n))
}

fn roulette_payout_for(entries: &[(u8, u64)], n: u8) -> u64 {
    entries.iter().map(|(slot, units)| units * ROULETTE_UNIT * roulette_mult(*slot, n)).sum()
}

fn keno_picks(p: &[u8; 32]) -> Result<u64> {
    let mask = u64::from_le_bytes([p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7]]);
    let c = mask.count_ones();
    require!(mask >> 40 == 0 && (1..=10).contains(&c), OpenHouseError::InvalidGameParams);
    Ok(mask)
}

fn plinko_table(p: &[u8; 32]) -> Result<&'static [u32]> {
    let idx = tables::PLINKO_ROWS.iter().position(|r| *r == p[0]).ok_or(OpenHouseError::InvalidGameParams)?;
    require!(p[1] <= 2, OpenHouseError::InvalidGameParams);
    Ok(tables::PLINKO[idx][p[1] as usize])
}

fn from_x100(amount: u64, mult_x100: u32) -> Result<u64> {
    u64::try_from(amount as u128 * mult_x100 as u128 / 100).map_err(|_| error!(OpenHouseError::MathOverflow))
}

/// The most a bet of this kind can pay in total. place_bet reserves exactly this.
pub fn max_payout(kind: Kind, params: &[u8; 32], amount: u64) -> Result<u64> {
    match kind {
        Kind::Dice | Kind::Flip | Kind::Limbo => win_payout(kind, params, amount),
        Kind::Wheel => {
            require!(params[0] <= 2, OpenHouseError::InvalidGameParams);
            from_x100(amount, *tables::WHEEL[params[0] as usize].iter().max().unwrap())
        }
        Kind::Plinko => from_x100(amount, *plinko_table(params)?.iter().max().unwrap()),
        Kind::Keno => {
            let n = keno_picks(params)?.count_ones() as usize;
            from_x100(amount, *tables::KENO[n - 1].iter().max().unwrap())
        }
        Kind::Roulette => {
            let (e, n) = roulette_entries(params, amount)?;
            Ok((0..37u8).map(|k| roulette_payout_for(&e[..n], k)).max().unwrap())
        }
        Kind::HotReels => from_x100(amount, 27_500),
        Kind::Slots777 => from_x100(amount, 33_000),
        Kind::VaultBreak => from_x100(amount, 13_000),
        Kind::Scratch => from_x100(amount, 20_000),
        Kind::SicBo => {
            require!(params[0] <= 7, OpenHouseError::InvalidGameParams);
            let m = if params[0] <= 1 { 203 } else { 1100 };
            from_x100(amount, m)
        }
        Kind::Dragon => {
            require!(params[0] <= 2, OpenHouseError::InvalidGameParams);
            let m = if params[0] == 2 { 1_280 } else { 214 };
            from_x100(amount, m)
        }
        Kind::Baccarat => {
            require!(params[0] <= 2, OpenHouseError::InvalidGameParams);
            let m = match params[0] {
                0 => 200,
                1 => 195,
                _ => 900,
            };
            from_x100(amount, m)
        }
    }
}

/// What a bet pays, given the random stream. Total payout, stake included, 0 when it loses.
pub fn settle_payout(kind: Kind, params: &[u8; 32], amount: u64, server: &[u8; 32], client: &[u8; 32], nonce: u64) -> Result<u64> {
    match kind {
        Kind::Dice | Kind::Flip | Kind::Limbo => {
            let u = fair::first_u32(server, client, nonce, 0);
            Ok(if is_win(kind, params, u)? { win_payout(kind, params, amount)? } else { 0 })
        }
        Kind::Wheel => {
            require!(params[0] <= 2, OpenHouseError::InvalidGameParams);
            let u = fair::draws(server, client, nonce, 1)[0];
            let seg = ((u as u64 * 24) >> 32) as usize;
            from_x100(amount, tables::WHEEL[params[0] as usize][seg])
        }
        Kind::Plinko => {
            let table = plinko_table(params)?;
            let d = fair::draws(server, client, nonce, params[0] as usize);
            let k = d[..params[0] as usize].iter().filter(|u| **u >= 0x8000_0000).count();
            from_x100(amount, table[k])
        }
        Kind::Keno => {
            let picks = keno_picks(params)?;
            let n = picks.count_ones() as usize;
            let hits = keno_hits(picks, &fair::draws(server, client, nonce, 10));
            from_x100(amount, tables::KENO[n - 1][hits])
        }
        Kind::Roulette => {
            let (e, n) = roulette_entries(params, amount)?;
            let u = fair::draws(server, client, nonce, 1)[0];
            let number = ((u as u64 * 37) >> 32) as u8;
            Ok(roulette_payout_for(&e[..n], number))
        }
        Kind::HotReels => {
            let d = fair::draws(server, client, nonce, 3);
            let stops = [((d[0] as u64 * 24) >> 32) as usize, ((d[1] as u64 * 24) >> 32) as usize, ((d[2] as u64 * 24) >> 32) as usize];
            from_x100(amount, hotreels_x100(stops))
        }
        Kind::Slots777 => {
            let d = fair::draws(server, client, nonce, 3);
            let stops = [((d[0] as u64 * 24) >> 32) as usize, ((d[1] as u64 * 24) >> 32) as usize, ((d[2] as u64 * 24) >> 32) as usize];
            from_x100(amount, slots777_x100(stops))
        }
        Kind::VaultBreak => {
            let d = fair::draws(server, client, nonce, 3);
            let stops = [((d[0] as u64 * 24) >> 32) as usize, ((d[1] as u64 * 24) >> 32) as usize, ((d[2] as u64 * 24) >> 32) as usize];
            from_x100(amount, vaultbreak_x100(stops))
        }
        Kind::Scratch => {
            let u = fair::draws(server, client, nonce, 1)[0];
            from_x100(amount, scratch_x100(u))
        }
        Kind::SicBo => {
            require!(params[0] <= 7, OpenHouseError::InvalidGameParams);
            let d = fair::draws(server, client, nonce, 3);
            let dice = [
                ((d[0] as u64 * 6) >> 32) as u8 + 1,
                ((d[1] as u64 * 6) >> 32) as u8 + 1,
                ((d[2] as u64 * 6) >> 32) as u8 + 1,
            ];
            from_x100(amount, sicbo_x100(params[0], dice))
        }
        Kind::Dragon => {
            require!(params[0] <= 2, OpenHouseError::InvalidGameParams);
            let d = fair::draws(server, client, nonce, 4);
            let dr = ((d[0] as u64 * 13) >> 32) as u8;
            let tr = ((d[2] as u64 * 13) >> 32) as u8;
            from_x100(amount, dragon_x100(params[0], dr, tr))
        }
        Kind::Baccarat => {
            require!(params[0] <= 2, OpenHouseError::InvalidGameParams);
            let d = fair::draws(server, client, nonce, 12);
            let ranks = [
                ((d[0] as u64 * 13) >> 32) as u8,
                ((d[2] as u64 * 13) >> 32) as u8,
                ((d[4] as u64 * 13) >> 32) as u8,
                ((d[6] as u64 * 13) >> 32) as u8,
                ((d[8] as u64 * 13) >> 32) as u8,
                ((d[10] as u64 * 13) >> 32) as u8,
            ];
            from_x100(amount, baccarat_x100(params[0], ranks))
        }
    }
}

/// Partial Fisher Yates over the numbers 1..=40, ten draws. Returns how many of the picks were drawn.
pub fn keno_hits(picks: u64, d: &[u32; 16]) -> usize {
    let mut a: [u8; 40] = core::array::from_fn(|i| i as u8 + 1);
    let mut drawn: u64 = 0;
    for i in 0..10usize {
        let j = i + ((d[i] as u64 * (40 - i as u64)) >> 32) as usize;
        a.swap(i, j);
        drawn |= 1u64 << (a[i] - 1);
    }
    (picks & drawn).count_ones() as usize
}

const HOTREELS_STRIPS: [[u8; 24]; 3] = [
    [0,1,2,3,4,0,1,2,5,0,1,2,3,4,0,1,2,0,5,3,4,0,1,2],
    [0,1,2,3,4,1,0,2,5,1,0,2,3,4,1,0,2,1,5,3,0,4,1,2],
    [0,1,2,3,4,0,1,5,2,0,3,1,4,0,2,3,1,0,5,4,2,0,1,3],
];
const HOTREELS_P3: [u32; 6] = [1100, 900, 1800, 3500, 7000, 27500];

const SLOTS777_STRIPS: [[u8; 24]; 3] = [
    [0,1,2,0,3,0,1,4,2,0,3,1,2,0,0,3,1,2,0,4,1,0,3,2],
    [0,1,2,3,0,1,4,2,0,1,3,0,2,1,0,3,2,1,0,4,0,1,3,2],
    [0,1,2,0,3,0,1,4,2,0,3,1,2,0,0,3,1,2,0,4,1,0,3,2],
];
const SLOTS777_P3: [u32; 5] = [500, 1000, 1800, 4500, 33000];

const VAULTBREAK_STRIPS: [[u8; 24]; 3] = [
    [0,1,2,0,3,1,4,2,0,1,0,2,3,5,1,0,2,1,0,4,3,0,1,2],
    [0,1,2,1,4,0,3,2,1,0,1,2,4,0,5,1,2,0,3,1,4,0,2,1],
    [0,1,2,0,3,2,1,4,0,2,0,1,3,5,2,0,1,2,0,4,3,1,0,2],
];
const VAULTBREAK_P3_LINE: [u32; 6] = [200, 300, 540, 1080, 2560, 13000];

pub fn hotreels_x100(stops: [usize; 3]) -> u32 {
    let s0 = HOTREELS_STRIPS[0][stops[0]];
    let s1 = HOTREELS_STRIPS[1][stops[1]];
    let s2 = HOTREELS_STRIPS[2][stops[2]];
    if s0 == s1 && s1 == s2 {
        HOTREELS_P3[s0 as usize]
    } else if s0 == 0 && s1 == 0 {
        300
    } else if s0 == 0 {
        50
    } else {
        0
    }
}

pub fn slots777_x100(stops: [usize; 3]) -> u32 {
    let s0 = SLOTS777_STRIPS[0][stops[0]];
    let s1 = SLOTS777_STRIPS[1][stops[1]];
    let s2 = SLOTS777_STRIPS[2][stops[2]];
    if s0 == s1 && s1 == s2 {
        SLOTS777_P3[s0 as usize]
    } else if s0 == 0 && s1 == 0 {
        100
    } else if s0 == 0 {
        30
    } else {
        0
    }
}

pub fn vaultbreak_x100(stops: [usize; 3]) -> u32 {
    const LINES: [[usize; 3]; 5] = [[0, 0, 0], [1, 1, 1], [2, 2, 2], [0, 1, 2], [2, 1, 0]];
    let mut total = 0u32;
    for line in LINES {
        let s0 = VAULTBREAK_STRIPS[0][(stops[0] + line[0] + 23) % 24];
        let s1 = VAULTBREAK_STRIPS[1][(stops[1] + line[1] + 23) % 24];
        let s2 = VAULTBREAK_STRIPS[2][(stops[2] + line[2] + 23) % 24];
        if s0 == s1 && s1 == s2 {
            total += VAULTBREAK_P3_LINE[s0 as usize];
        }
    }
    total
}

const SCRATCH_PROBS: [u64; 6] = [1_200_000, 1_100_000, 400_000, 150_000, 40_000, 5_000];
const SCRATCH_X100: [u32; 6] = [100, 200, 500, 1000, 5000, 20000];

pub fn scratch_x100(u0: u32) -> u32 {
    let mut r = (u0 as u64 * 10_000_000) >> 32;
    for i in 0..6 {
        if r < SCRATCH_PROBS[i] {
            return SCRATCH_X100[i];
        }
        r -= SCRATCH_PROBS[i];
    }
    0
}

pub fn sicbo_x100(bet: u8, dice: [u8; 3]) -> u32 {
    let sum = dice[0] + dice[1] + dice[2];
    let triple = dice[0] == dice[1] && dice[1] == dice[2];
    match bet {
        0 => if !triple && sum <= 10 { 203 } else { 0 },
        1 => if !triple && sum >= 11 { 203 } else { 0 },
        2..=7 => {
            let n = bet - 1;
            let k = dice.iter().filter(|&&d| d == n).count();
            match k {
                1 => 200,
                2 => 350,
                3 => 1100,
                _ => 0,
            }
        }
        _ => 0,
    }
}

pub fn dragon_x100(bet: u8, dr: u8, tr: u8) -> u32 {
    if dr > tr {
        if bet == 0 { 214 } else { 0 }
    } else if tr > dr {
        if bet == 1 { 214 } else { 0 }
    } else if bet == 2 {
        1280
    } else {
        0
    }
}

#[inline]
fn bacc_val(rank: u8) -> u8 {
    if rank >= 9 { 0 } else { rank + 1 }
}

pub fn baccarat_x100(bet: u8, ranks: [u8; 6]) -> u32 {
    let mut pt = (bacc_val(ranks[0]) + bacc_val(ranks[2])) % 10;
    let mut bt = (bacc_val(ranks[1]) + bacc_val(ranks[3])) % 10;
    if !(pt >= 8 || bt >= 8) {
        let mut nx = 4usize;
        let p3 = if pt <= 5 {
            let v = bacc_val(ranks[nx]);
            nx += 1;
            pt = (pt + v) % 10;
            Some(v)
        } else {
            None
        };
        let b_draw = match p3 {
            None => bt <= 5,
            Some(v) => {
                bt <= 2
                    || (bt == 3 && v != 8)
                    || (bt == 4 && (2..=7).contains(&v))
                    || (bt == 5 && (4..=7).contains(&v))
                    || (bt == 6 && (6..=7).contains(&v))
            }
        };
        if b_draw {
            bt = (bt + bacc_val(ranks[nx])) % 10;
        }
    }
    if pt > bt {
        if bet == 0 { 200 } else { 0 }
    } else if bt > pt {
        if bet == 1 { 195 } else { 0 }
    } else {
        if bet == 2 { 900 } else { 100 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex32(s: &str) -> [u8; 32] {
        let mut o = [0u8; 32];
        for i in 0..32 {
            o[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap();
        }
        o
    }

    // Vectors produced by Node's crypto.createHmac, see tests/ts/vectors.json
    const SERVER: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
    const CLIENT: &str = "cf4b6f427f89c9dc6c6b7fba1ca0810243f707f46204449c42be63561a106600";

    #[test]
    fn hmac_matches_node() {
        let (s, c) = (hex32(SERVER), hex32(CLIENT));
        assert_eq!(
            fair::mac(&s, &c, 0, 0),
            hex32("e04fc580d1979313d2e00c5e85551fc7290ad826ae06c44b738d2aa35ba5524f")
        );
        assert_eq!(
            fair::mac(&s, &c, 1, 0),
            hex32("5f68577b85bc9f9d3e9791153f03abef4e90bc87fe6f503d1eb290209735038a")
        );
        assert_eq!(
            fair::mac(&s, &c, 0, 1),
            hex32("6e17ceb7ab61ed75da3b64bc71dc9e2ef8befaff69905ef206b7f3058f0e1dd6")
        );
        assert_eq!(fair::first_u32(&s, &c, 0, 0), 3763324288);
        assert_eq!(fair::first_u32(&s, &c, 1, 1), 2243731357);
    }

    fn dice(under: bool, t: u16) -> [u8; 32] {
        let mut p = [0u8; 32];
        p[0] = under as u8;
        p[1..3].copy_from_slice(&t.to_le_bytes());
        p
    }

    #[test]
    fn dice_rtp_is_exactly_99() {
        // Count winning u32 buckets analytically via roll values 0..9999 (each roll owns 2^32/10000 u32s).
        for t in [200u16, 1000, 4950, 5000, 9000, 9800] {
            for under in [true, false] {
                let p = dice(under, t);
                let wins = (0..10_000u64)
                    .filter(|r| {
                        let u = (((*r as u128) << 32) / 10_000) as u32 + 1; // lands in bucket r
                        is_win(Kind::Dice, &p, u).unwrap()
                    })
                    .count() as u128;
                assert_eq!(wins, t as u128, "win count for target {t} under={under}");
                // payout * chance = 0.99
                let pay = win_payout(Kind::Dice, &p, 1_000_000_000).unwrap() as u128;
                let rtp = pay * wins; // pay/1e9 * wins/1e4
                assert!(rtp.abs_diff(990_000_000u128 * 10_000) <= 10_000 * 10_000, "rtp {rtp}");
            }
        }
    }

    #[test]
    fn dice_rejects_bad_params() {
        assert!(win_payout(Kind::Dice, &dice(true, 100), 1).is_err());
        assert!(win_payout(Kind::Dice, &dice(true, 9900), 1).is_err());
        let mut p = dice(true, 5000);
        p[0] = 2;
        assert!(win_payout(Kind::Dice, &p, 1).is_err());
    }

    #[test]
    fn flip_is_fair_and_pays_198() {
        let heads = [0u8; 32];
        let mut tails = [0u8; 32];
        tails[0] = 1;
        assert!(is_win(Kind::Flip, &heads, 0).unwrap());
        assert!(is_win(Kind::Flip, &heads, 0x7fff_ffff).unwrap());
        assert!(!is_win(Kind::Flip, &heads, 0x8000_0000).unwrap());
        assert!(is_win(Kind::Flip, &tails, 0x8000_0000).unwrap());
        assert_eq!(win_payout(Kind::Flip, &heads, 1_000_000).unwrap(), 1_980_000);
    }

    #[test]
    fn limbo_probability_matches_99_over_target() {
        for t in [101u32, 200, 1000, 100_000] {
            let mut p = [0u8; 32];
            p[..4].copy_from_slice(&t.to_le_bytes());
            // threshold u at which crash_x100 >= t flips: u >= 2^32 - 99*2^32/t
            let thresh = (1u128 << 32) - (99u128 << 32) / t as u128;
            let below = (thresh as u32).saturating_sub(2);
            assert!(!is_win(Kind::Limbo, &p, below).unwrap() || thresh < 3);
            assert!(is_win(Kind::Limbo, &p, u32::MAX).unwrap());
            let wins = (1u128 << 32) - thresh;
            let chance = wins as f64 / (1u64 << 32) as f64;
            let ev = chance * t as f64 / 100.0;
            assert!((ev - 0.99).abs() < 1e-3, "t={t} ev={ev}");
        }
    }

    #[test]
    fn top_multipliers_cover_every_game() {
        for g in [
            "dice", "flip", "limbo", "crash", "mines", "plinko", "hilo", "towers", "keno", "wheel",
            "roulette", "blackjack", "baccarat", "poker", "dragon", "sicbo", "slots777",
            "hotreels", "vaultbreak", "scratch",
        ] {
            let mut id = [0u8; 16];
            id[..g.len()].copy_from_slice(g.as_bytes());
            assert!(top_multiplier_x100(&id).is_some(), "{g}");
        }
        let mut id = [0u8; 16];
        id[..4].copy_from_slice(b"nope");
        assert!(top_multiplier_x100(&id).is_none());
    }

    #[test]
    fn new_games_rtp_and_caps() {
        for (r, t) in tables::WHEEL.iter().enumerate() {
            let sum: u64 = t.iter().map(|m| *m as u64).sum();
            assert!(sum * 100 < 24 * 10_000 && sum * 100 > 24 * 9_800, "wheel {r} {sum}");
            assert!(*t.iter().max().unwrap() <= 625);
        }
        for (ri, rows) in tables::PLINKO_ROWS.iter().enumerate() {
            for risk in 0..3 {
                let t = tables::PLINKO[ri][risk];
                assert_eq!(t.len(), *rows as usize + 1);
                let mut b = vec![1u128];
                for _ in 0..*rows {
                    let mut x = vec![1u128];
                    for j in 1..b.len() { x.push(b[j - 1] + b[j]); }
                    x.push(1);
                    b = x;
                }
                let sum: u128 = t.iter().zip(&b).map(|(m, c)| *m as u128 * c).sum();
                let rtp = sum * 10_000 / (100u128 << rows);
                assert!(rtp < 10_000 && rtp > 9_800, "plinko {rows} {risk} {rtp}");
                assert!(*t.iter().max().unwrap() <= 100_000);
            }
        }
        let c = |n: u128, k: u128| -> u128 { if k > n { 0 } else { (1..=k).fold(1u128, |a, i| a * (n - k + i) / i) } };
        for n in 1..=10usize {
            let t = tables::KENO[n - 1];
            assert_eq!(t.len(), n + 1);
            let sum: u128 = (0..=n).map(|h| t[h] as u128 * c(n as u128, h as u128) * c(40 - n as u128, 10 - h as u128)).sum();
            let rtp = sum * 10_000 / (100 * c(40, 10));
            assert!(rtp < 10_000 && rtp > 9_800, "keno {n} {rtp}");
            assert!(*t.iter().max().unwrap() <= 50_000);
        }
    }

    #[test]
    fn roulette_slots_return_36_over_37_and_encoding_is_strict() {
        for slot in 0..=48u8 {
            let total: u64 = (0..37u8).map(|n| roulette_mult(slot, n)).sum();
            assert_eq!(total, 36, "slot {slot}");
        }
        let mut p = [0u8; 32];
        p[0] = 37; p[1] = 5; // red x5 units
        p[3] = 17; p[4] = 1; // straight 17 x1 unit
        assert_eq!(max_payout(Kind::Roulette, &p, 6 * ROULETTE_UNIT).unwrap(), 36 * ROULETTE_UNIT);
        assert!(max_payout(Kind::Roulette, &p, 5 * ROULETTE_UNIT).is_err(), "stake must equal units");
        let mut dup = p; dup[3] = 37; // same slot twice
        assert!(max_payout(Kind::Roulette, &dup, 6 * ROULETTE_UNIT).is_err());
        let mut bad = p; bad[3] = 49;
        assert!(max_payout(Kind::Roulette, &bad, 6 * ROULETTE_UNIT).is_err());
    }

    #[test]
    fn draws_are_uniform_for_every_mapping() {
        let (s, c) = (hex32(SERVER), hex32(CLIENT));
        let n = 20_000u64;
        let (mut wheel, mut rou, mut flip) = ([0u32; 24], [0u32; 37], 0u32);
        for nonce in 0..n {
            let d = fair::draws(&s, &c, nonce, 1);
            wheel[((d[0] as u64 * 24) >> 32) as usize] += 1;
            rou[((d[0] as u64 * 37) >> 32) as usize] += 1;
            if d[0] >= 0x8000_0000 { flip += 1; }
        }
        for b in wheel { assert!((b as i64 - 833).abs() < 170, "wheel bin {b}"); }
        for b in rou { assert!((b as i64 - 540).abs() < 140, "roulette bin {b}"); }
        assert!((flip as i64 - 10_000).abs() < 400);
        // keno: average hits for 10 picks must sit near 10 * 10 / 40 = 2.5
        let picks: u64 = (1u64 << 10) - 1;
        let mut hits = 0usize;
        for nonce in 0..5_000u64 { hits += keno_hits(picks, &fair::draws(&s, &c, nonce, 10)); }
        let mean = hits as f64 / 5_000.0;
        assert!((mean - 2.5).abs() < 0.1, "keno mean {mean}");
    }

    #[test]
    #[ignore]
    fn export_rust_vectors() {
        use std::fmt::Write as _;
        fn to_hex(b: &[u8]) -> String {
            let mut s = String::with_capacity(b.len() * 2);
            for x in b { write!(s, "{x:02x}").unwrap(); }
            s
        }
        let mut rng: u64 = 0x9e37_79b9_7f4a_7c15;
        fn next_u64(r: &mut u64) -> u64 {
            *r ^= *r << 13;
            *r ^= *r >> 7;
            *r ^= *r << 17;
            *r
        }
        fn next_32(r: &mut u64) -> [u8; 32] {
            let mut b = [0u8; 32];
            for c in b.chunks_mut(8) { c.copy_from_slice(&next_u64(r).to_le_bytes()); }
            b
        }
        let mut out = String::from("[\n");
        let mut first = true;
        let mut push = |game: &str, params: &[u8; 32], amount: u64, s: &[u8; 32], c: &[u8; 32], nonce: u64, kind: Kind| {
            let mp = max_payout(kind, params, amount).unwrap();
            let sp = settle_payout(kind, params, amount, s, c, nonce).unwrap();
            if !first { out.push_str(",\n"); }
            first = false;
            write!(
                out,
                "  {{\"game\":\"{game}\",\"params\":\"{}\",\"amount\":\"{amount}\",\"server\":\"{}\",\"client\":\"{}\",\"nonce\":\"{nonce}\",\"maxPayout\":\"{mp}\",\"settlePayout\":\"{sp}\"}}",
                to_hex(params), to_hex(s), to_hex(c)
            ).unwrap();
        };

        for _ in 0..200 {
            let mut p = [0u8; 32];
            p[0] = (next_u64(&mut rng) % 3) as u8;
            let amt = 1_000_000 + (next_u64(&mut rng) % 50) * 1_000_000;
            let (s, c, nonce) = (next_32(&mut rng), next_32(&mut rng), next_u64(&mut rng) % 100_000);
            push("wheel", &p, amt, &s, &c, nonce, Kind::Wheel);
        }
        for _ in 0..200 {
            let mut p = [0u8; 32];
            p[0] = tables::PLINKO_ROWS[(next_u64(&mut rng) % 5) as usize];
            p[1] = (next_u64(&mut rng) % 3) as u8;
            let amt = 1_000_000 + (next_u64(&mut rng) % 50) * 1_000_000;
            let (s, c, nonce) = (next_32(&mut rng), next_32(&mut rng), next_u64(&mut rng) % 100_000);
            push("plinko", &p, amt, &s, &c, nonce, Kind::Plinko);
        }
        for _ in 0..200 {
            let n_picks = 1 + (next_u64(&mut rng) % 10) as usize;
            let mut mask = 0u64;
            while (mask.count_ones() as usize) < n_picks {
                mask |= 1u64 << (next_u64(&mut rng) % 40);
            }
            let mut p = [0u8; 32];
            p[0..8].copy_from_slice(&mask.to_le_bytes());
            let amt = 1_000_000 + (next_u64(&mut rng) % 50) * 1_000_000;
            let (s, c, nonce) = (next_32(&mut rng), next_32(&mut rng), next_u64(&mut rng) % 100_000);
            push("keno", &p, amt, &s, &c, nonce, Kind::Keno);
        }
        for _ in 0..200 {
            let n_ent = 1 + (next_u64(&mut rng) % 10) as usize;
            let mut seen = 0u64;
            let mut p = [0u8; 32];
            let mut total_units = 0u64;
            for i in 0..n_ent {
                let mut slot = (next_u64(&mut rng) % 49) as u8;
                while (seen & (1u64 << slot)) != 0 { slot = (slot + 1) % 49; }
                seen |= 1u64 << slot;
                let units = (1 + (next_u64(&mut rng) % 10)) as u16;
                p[3 * i] = slot;
                p[3 * i + 1..3 * i + 3].copy_from_slice(&units.to_le_bytes());
                total_units += units as u64;
            }
            let (s, c, nonce) = (next_32(&mut rng), next_32(&mut rng), next_u64(&mut rng) % 100_000);
            push("roulette", &p, total_units * ROULETTE_UNIT, &s, &c, nonce, Kind::Roulette);
        }
        out.push_str("\n]\n");
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/core/test/rust-vectors.json");
        std::fs::write(path, out).unwrap();
    }

    #[test]
    fn slots_brute_force_max_and_rtp() {
        let mut max_hr = 0u64;
        let mut sum_hr = 0u64;
        let mut max_s7 = 0u64;
        let mut sum_s7 = 0u64;
        let mut max_vb = 0u64;
        let mut sum_vb = 0u64;
        for a in 0..24 {
            for b in 0..24 {
                for c in 0..24 {
                    let s = [a, b, c];
                    let hr = hotreels_x100(s) as u64;
                    if hr > max_hr { max_hr = hr; }
                    sum_hr += hr;
                    let s7 = slots777_x100(s) as u64;
                    if s7 > max_s7 { max_s7 = s7; }
                    sum_s7 += s7;
                    let vb = vaultbreak_x100(s) as u64;
                    if vb > max_vb { max_vb = vb; }
                    sum_vb += vb;
                }
            }
        }
        assert_eq!(max_hr, 27_500);
        assert_eq!(max_s7, 33_000);
        assert_eq!(max_vb, 13_000);
        let n3 = (24 * 24 * 24) as f64 * 100.0;
        let rtp_hr = sum_hr as f64 / n3;
        let rtp_s7 = sum_s7 as f64 / n3;
        let rtp_vb = sum_vb as f64 / n3;
        assert!((rtp_hr - 0.9742).abs() < 0.0001, "hotreels rtp {}", rtp_hr);
        assert!((rtp_s7 - 0.9683).abs() < 0.0001, "slots777 rtp {}", rtp_s7);
        assert!((rtp_vb - 0.9620).abs() < 0.0001, "vaultbreak rtp {}", rtp_vb);
    }

    #[test]
    fn scratch_exact_rtp_099() {
        let mut ev = 0u128;
        for i in 0..6 {
            ev += SCRATCH_PROBS[i] as u128 * SCRATCH_X100[i] as u128;
        }
        // 10_000_000 * 100 * 0.99 = 990_000_000
        assert_eq!(ev, 990_000_000);
    }

    #[test]
    fn dragon_and_baccarat_rtp_below_one() {
        for bet in 0..=2u8 {
            let mut sum = 0u64;
            for dr in 0..13u8 {
                for tr in 0..13u8 {
                    sum += dragon_x100(bet, dr, tr) as u64;
                }
            }
            assert!(sum < 13 * 13 * 100, "dragon bet {} rtp >= 1", bet);
        }
        for bet in 0..=2u8 {
            // Exact enumeration over 13^4 initial 4-card combinations weighted by 13^2
            // or full 13^6 enumeration: 13^6 = 4,826,809 (fast in release/test)
            let mut sum = 0u64;
            let mut cnt = 0u64;
            for r0 in 0..13u8 {
                for r1 in 0..13u8 {
                    for r2 in 0..13u8 {
                        for r3 in 0..13u8 {
                            for r4 in 0..13u8 {
                                for r5 in 0..13u8 {
                                    sum += baccarat_x100(bet, [r0, r1, r2, r3, r4, r5]) as u64;
                                    cnt += 1;
                                }
                            }
                        }
                    }
                }
            }
            assert!(sum < cnt * 100, "baccarat bet {} rtp >= 1 ({}/{})", bet, sum, cnt * 100);
        }
    }

    #[test]
    fn batch2_encoder_rejections() {
        let mut p = [0u8; 32];
        p[0] = 8;
        assert!(max_payout(Kind::SicBo, &p, 1_000_000).is_err());
        p[0] = 3;
        assert!(max_payout(Kind::Dragon, &p, 1_000_000).is_err());
        assert!(max_payout(Kind::Baccarat, &p, 1_000_000).is_err());
    }

    #[test]
    #[ignore]
    fn export_rust_vectors_batch2() {
        let mut rng: u64 = 0xCAFE_BABE_1234_5678;
        let next_u64 = |r: &mut u64| -> u64 {
            *r ^= *r << 13;
            *r ^= *r >> 7;
            *r ^= *r << 17;
            *r
        };
        let next_32 = |r: &mut u64| -> [u8; 32] {
            let mut b = [0u8; 32];
            for c in b.chunks_mut(8) {
                c.copy_from_slice(&next_u64(r).to_le_bytes());
            }
            b
        };
        let to_hex = |b: &[u8]| -> String { b.iter().map(|x| format!("{:02x}", x)).collect() };
        let mut out = String::from("[\n");
        let mut first = true;
        let mut push = |game: &str, params: &[u8; 32], amount: u64, s: &[u8; 32], c: &[u8; 32], nonce: u64, kind: Kind| {
            let pay = settle_payout(kind, params, amount, s, c, nonce).unwrap();
            let max_p = max_payout(kind, params, amount).unwrap();
            if !first { out.push_str(",\n"); }
            first = false;
            out.push_str(&format!(
                "  {{\"game\":\"{}\",\"params\":\"{}\",\"amount\":{},\"server\":\"{}\",\"client\":\"{}\",\"nonce\":{},\"payout\":{},\"maxPayout\":{}}}",
                game, to_hex(params), amount, to_hex(s), to_hex(c), nonce, pay, max_p
            ));
        };
        for (game, kind) in [
            ("hotreels", Kind::HotReels),
            ("slots777", Kind::Slots777),
            ("vaultbreak", Kind::VaultBreak),
            ("scratch", Kind::Scratch),
        ] {
            for _ in 0..200 {
                let p = [0u8; 32];
                let amt = 1_000_000 + (next_u64(&mut rng) % 50) * 1_000_000;
                let (s, c, nonce) = (next_32(&mut rng), next_32(&mut rng), next_u64(&mut rng) % 100_000);
                push(game, &p, amt, &s, &c, nonce, kind);
            }
        }
        for _ in 0..200 {
            let mut p = [0u8; 32];
            p[0] = (next_u64(&mut rng) % 8) as u8;
            let amt = 1_000_000 + (next_u64(&mut rng) % 50) * 1_000_000;
            let (s, c, nonce) = (next_32(&mut rng), next_32(&mut rng), next_u64(&mut rng) % 100_000);
            push("sicbo", &p, amt, &s, &c, nonce, Kind::SicBo);
        }
        for _ in 0..200 {
            let mut p = [0u8; 32];
            p[0] = (next_u64(&mut rng) % 3) as u8;
            let amt = 1_000_000 + (next_u64(&mut rng) % 50) * 1_000_000;
            let (s, c, nonce) = (next_32(&mut rng), next_32(&mut rng), next_u64(&mut rng) % 100_000);
            push("dragon", &p, amt, &s, &c, nonce, Kind::Dragon);
        }
        for _ in 0..200 {
            let mut p = [0u8; 32];
            p[0] = (next_u64(&mut rng) % 3) as u8;
            let amt = 1_000_000 + (next_u64(&mut rng) % 50) * 1_000_000;
            let (s, c, nonce) = (next_32(&mut rng), next_32(&mut rng), next_u64(&mut rng) % 100_000);
            push("baccarat", &p, amt, &s, &c, nonce, Kind::Baccarat);
        }
        out.push_str("\n]\n");
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/core/test/rust-vectors-b2.json");
        std::fs::write(path, out).unwrap();
    }
}
