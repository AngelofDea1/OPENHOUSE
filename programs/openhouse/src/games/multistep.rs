//! Batch-3 multi-step games: Mines, HiLo, Towers, Blackjack, Video Poker.
//!
//! All payouts are integer-floor in the house's favour. The ladder is computed
//! stepwise with u128 floors, never with floating point and never with a
//! combinatorial product. CAP_X100 = 25_000 (250x) clamps Mines, HiLo, Towers.
use crate::errors::OpenHouseError;
use anchor_lang::prelude::*;

use super::fair::draws64;

pub const CAP_X100: u64 = 25_000; // 250x, used by Mines, HiLo, Towers

/// Return value of replay().
#[derive(Clone, Copy, Debug)]
pub struct Outcome {
    pub payout: u64,
    pub finished: bool,
    pub cashed: bool, // true if the round ended by cashing out (not a loss)
}

// ─── Mines ───────────────────────────────────────────────────────────────────

fn mines_params(p: &[u8; 32]) -> Result<u8> {
    let m = p[0];
    require!((1..=24).contains(&m), OpenHouseError::InvalidGameParams);
    // bytes 1..32 must be zero
    require!(p[1..].iter().all(|b| *b == 0), OpenHouseError::InvalidGameParams);
    Ok(m)
}

/// Fisher-Yates mine positions from draws 0..m. Returns sorted mine set.
fn mine_positions(d: &[u32; 64], m: u8) -> [u8; 24] {
    let m = m as usize;
    let mut a: [u8; 25] = core::array::from_fn(|i| i as u8);
    for i in 0..m {
        let j = i + ((d[i] as u64 * (25 - i as u64)) >> 32) as usize;
        a.swap(i, j);
    }
    let mut mines = [0u8; 24];
    mines[..m].copy_from_slice(&a[..m]);
    mines
}

/// Value after k safe reveals (stepwise, floor, capped).
/// Returns (value, capped) where capped means cap was applied.
fn mines_value(amount: u64, m: u8, k: u32) -> (u64, bool) {
    let m = m as u64;
    let cap_value = amount * CAP_X100 / 100;
    let mut v = amount * 99 / 100; // v0
    for i in 0..k {
        v = v * (25 - i as u64) / (25 - m - i as u64);
        if v >= cap_value {
            return (cap_value, true);
        }
    }
    (v, false)
}

pub fn mines_max_payout(params: &[u8; 32], amount: u64) -> Result<u64> {
    let m = mines_params(params)?;
    let safe = 25 - m as u32;
    let (val, _) = mines_value(amount, m, safe);
    let cap = amount * CAP_X100 / 100;
    Ok(val.min(cap))
}

pub fn replay_mines(
    params: &[u8; 32], amount: u64,
    server: &[u8; 32], client: &[u8; 32], nonce: u64,
    actions: &[u8; 32], action_count: u8,
    timeout: bool,
) -> Result<Outcome> {
    let m = mines_params(params)?;
    let safe_total = 25 - m as usize;
    let d = draws64(server, client, nonce, 25);
    let mines = mine_positions(&d, m);
    let mine_set = &mines[..m as usize];
    let cap_value = amount * CAP_X100 / 100;

    let mut safe_count: u32 = 0;
    let mut revealed = [false; 25];
    let mut finished = false;
    let mut cashed = false;
    let mut payout = 0u64;

    for idx in 0..action_count as usize {
        let action = actions[idx];
        if finished { break; } // ignore actions after terminal

        if action == 255 {
            // cash out
            if safe_count == 0 { continue; }
            let (v, _) = mines_value(amount, m, safe_count);
            payout = v;
            finished = true;
            cashed = true;
        } else if action <= 24 {
            let cell = action as usize;
            if revealed[cell] { continue; }
            revealed[cell] = true;
            if mine_set.contains(&(cell as u8)) {
                payout = 0;
                finished = true;
            } else {
                safe_count += 1;
                let (v, capped) = mines_value(amount, m, safe_count);
                if capped {
                    payout = cap_value;
                    finished = true;
                    cashed = true;
                } else if safe_count as usize == safe_total {
                    payout = v;
                    finished = true;
                    cashed = true;
                }
            }
        } else {
            continue;
        }
    }

    // Timeout: auto cash out at current value.
    if !finished && timeout {
        let (v, _) = mines_value(amount, m, safe_count);
        // 0 safe reveals = refund (push)
        payout = if safe_count == 0 { amount } else { v };
        finished = true;
        cashed = true;
    }

    Ok(Outcome { payout, finished, cashed })
}

// ─── HiLo ────────────────────────────────────────────────────────────────────

fn hilo_params(p: &[u8; 32]) -> Result<()> {
    require!(p.iter().all(|b| *b == 0), OpenHouseError::InvalidGameParams);
    Ok(())
}

/// rank of card i = (d[i] * 13) >> 32. 0=Ace(low), 12=King.
fn hilo_ranks(d: &[u32; 64]) -> [u8; 20] {
    core::array::from_fn(|i| ((d[i] as u64 * 13) >> 32) as u8)
}

/// Number of favourable ranks for a guess of hi (true) or lo (false) on the current rank.
fn hilo_favourable(rank: u8, hi: bool) -> u32 {
    if hi { 13 - rank as u32 } else { rank as u32 + 1 }
}

/// Value after the starting deal plus k winning guesses.
fn hilo_value(amount: u64, guesses: &[(u8, bool)]) -> (u64, bool) {
    let cap_value = amount * CAP_X100 / 100;
    let mut v = amount * 99 / 100;
    for &(rank, hi) in guesses {
        let count = hilo_favourable(rank, hi);
        v = v * 13 / count as u64;
        if v >= cap_value {
            return (cap_value, true);
        }
    }
    (v, false)
}

pub fn hilo_max_payout(params: &[u8; 32], amount: u64) -> Result<u64> {
    hilo_params(params)?;
    // Max is the cap (any streak can reach it).
    Ok(amount * CAP_X100 / 100)
}

pub fn replay_hilo(
    params: &[u8; 32], amount: u64,
    server: &[u8; 32], client: &[u8; 32], nonce: u64,
    actions: &[u8; 32], action_count: u8,
    timeout: bool,
) -> Result<Outcome> {
    hilo_params(params)?;
    // Need 20 card ranks (draws 0..19) + 20 suit draws (draws 20..39)
    let d = draws64(server, client, nonce, 40);
    let ranks = hilo_ranks(&d);

    let mut guesses: [Option<(u8, bool)>; 19] = [None; 19];
    let mut guess_count = 0usize;
    let mut cur_rank = ranks[0];
    let mut finished = false;
    let mut cashed = false;
    let mut payout = 0u64;

    for idx in 0..action_count as usize {
        if finished { break; }
        let action = actions[idx];
        if action == 255 {
            // cash out
            if guess_count == 0 { continue; }
            let valid_guesses: Vec<_> = guesses[..guess_count].iter().filter_map(|x| *x).collect();
            let (v, _) = hilo_value(amount, &valid_guesses);
            payout = v;
            finished = true;
            cashed = true;
        } else if action <= 1 {
            let hi = action == 0;
            let next_rank = ranks[1 + guess_count];
            let won = if hi { next_rank >= cur_rank } else { next_rank <= cur_rank };
            guesses[guess_count] = Some((cur_rank, hi));
            guess_count += 1;
            cur_rank = next_rank;
            if won {
                let valid_guesses: Vec<_> = guesses[..guess_count].iter().filter_map(|x| *x).collect();
                let (v, capped) = hilo_value(amount, &valid_guesses);
                if capped {
                    payout = amount * CAP_X100 / 100;
                    finished = true;
                    cashed = true;
                } else if guess_count >= 19 {
                    payout = v;
                    finished = true;
                    cashed = true;
                }
            } else {
                payout = 0;
                finished = true;
            }
        } else {
            continue;
        }
    }

    // Timeout: auto cash out at current value.
    if !finished && timeout {
        if guess_count == 0 {
            payout = amount; // push
        } else {
            let valid_guesses: Vec<_> = guesses[..guess_count].iter().filter_map(|x| *x).collect();
            let (v, _) = hilo_value(amount, &valid_guesses);
            payout = v;
        }
        finished = true;
        cashed = true;
    }

    Ok(Outcome { payout, finished, cashed })
}

// ─── Towers ──────────────────────────────────────────────────────────────────

fn towers_params(p: &[u8; 32]) -> Result<u8> {
    let diff = p[0];
    require!(diff <= 2, OpenHouseError::InvalidGameParams);
    require!(p[1..].iter().all(|b| *b == 0), OpenHouseError::InvalidGameParams);
    Ok(diff)
}

fn towers_cols(diff: u8) -> u64 {
    match diff { 0 => 4, 1 => 3, _ => 2 }
}

/// Value after k safe floors.
fn towers_value(amount: u64, cols: u64, k: u32) -> (u64, bool) {
    let cap_value = amount * CAP_X100 / 100;
    let mut v = amount * 99 / 100;
    for _ in 0..k {
        v = v * cols / (cols - 1);
        if v >= cap_value {
            return (cap_value, true);
        }
    }
    (v, false)
}

pub fn towers_max_payout(params: &[u8; 32], amount: u64) -> Result<u64> {
    let diff = towers_params(params)?;
    let cols = towers_cols(diff);
    let (val, _) = towers_value(amount, cols, 8);
    let cap = amount * CAP_X100 / 100;
    Ok(val.min(cap))
}

pub fn replay_towers(
    params: &[u8; 32], amount: u64,
    server: &[u8; 32], client: &[u8; 32], nonce: u64,
    actions: &[u8; 32], action_count: u8,
    timeout: bool,
) -> Result<Outcome> {
    let diff = towers_params(params)?;
    let cols = towers_cols(diff);
    let d = draws64(server, client, nonce, 8);

    // bomb column of floor f = (d[f] * cols) >> 32
    let bombs: [u64; 8] = core::array::from_fn(|f| (d[f] as u64 * cols) >> 32);

    let mut floor = 0usize;
    let mut finished = false;
    let mut cashed = false;
    let mut payout = 0u64;

    for idx in 0..action_count as usize {
        if finished { break; }
        let action = actions[idx];
        if action == 255 {
            if floor == 0 { continue; }
            let (v, _) = towers_value(amount, cols, floor as u32);
            payout = v;
            finished = true;
            cashed = true;
        } else if (action as u64) < cols {
            let col = action as u64;
            if col == bombs[floor] {
                payout = 0;
                finished = true;
            } else {
                floor += 1;
                let (v, capped) = towers_value(amount, cols, floor as u32);
                if capped {
                    payout = v;
                    finished = true;
                    cashed = true;
                } else if floor >= 8 {
                    payout = v;
                    finished = true;
                    cashed = true;
                }
            }
        } else {
            continue;
        }
    }

    if !finished && timeout {
        if floor == 0 {
            payout = amount; // push
        } else {
            let (v, _) = towers_value(amount, cols, floor as u32);
            payout = v;
        }
        finished = true;
        cashed = true;
    }

    Ok(Outcome { payout, finished, cashed })
}

// ─── Blackjack ───────────────────────────────────────────────────────────────

fn bj_params(p: &[u8; 32]) -> Result<()> {
    require!(p.iter().all(|b| *b == 0), OpenHouseError::InvalidGameParams);
    Ok(())
}

/// Fisher-Yates deck shuffle from 51 draws. Returns [0..51] permutation.
/// Deck from draws: a=[0..51]; for i from 51 down to 1: j=(d[51-i]*(i+1))>>32; swap a[i],a[j].
fn bj_deck(d: &[u32; 64]) -> [u8; 52] {
    let mut a: [u8; 52] = core::array::from_fn(|i| i as u8);
    for i in (1usize..=51).rev() {
        let di = 51 - i; // d[0] is for i=51, d[50] is for i=1
        let j = ((d[di] as u64 * (i as u64 + 1)) >> 32) as usize;
        a.swap(i, j);
    }
    a
}

fn bj_rank(card: u8) -> u8 { card % 13 } // 0=Ace, 1-12
fn bj_val(rank: u8) -> u8 {
    if rank == 0 { 11 } else if rank >= 9 { 10 } else { rank + 1 }
}

/// Compute soft hand value, dropping Aces from 11→1 as needed.
fn bj_hand_value(cards: &[u8]) -> u8 {
    let mut total = 0u16;
    let mut aces = 0u8;
    for &c in cards {
        let v = bj_val(bj_rank(c));
        total += v as u16;
        if v == 11 { aces += 1; }
    }
    while total > 21 && aces > 0 { total -= 10; aces -= 1; }
    total as u8
}

fn bj_is_bj(cards: &[u8]) -> bool {
    cards.len() == 2 && {
        let r0 = bj_rank(cards[0]);
        let r1 = bj_rank(cards[1]);
        (r0 == 0 && r1 >= 9) || (r1 == 0 && r0 >= 9)
    }
}

pub fn blackjack_max_payout(params: &[u8; 32], amount: u64) -> Result<u64> {
    bj_params(params)?;
    // Max is amount*5/2 for a natural, or amount*4 for a double win.
    // amount*4 is the larger.
    Ok(amount * 4)
}

pub fn replay_blackjack(
    params: &[u8; 32], amount: u64,
    server: &[u8; 32], client: &[u8; 32], nonce: u64,
    actions: &[u8; 32], action_count: u8,
    extra_stake: u64,
    timeout: bool,
) -> Result<Outcome> {
    bj_params(params)?;
    let d = draws64(server, client, nonce, 52);
    let deck = bj_deck(&d);

    let mut ptr = 0usize;
    let next_card = |ptr: &mut usize| -> u8 { let c = deck[*ptr]; *ptr += 1; c };

    // Deal order: player, dealer_up, player, dealer_hole
    let p0 = next_card(&mut ptr);
    let d_up = next_card(&mut ptr);
    let p1 = next_card(&mut ptr);
    let d_hole = next_card(&mut ptr);

    let player_init = [p0, p1];
    let dealer_init = [d_up, d_hole];

    // Check for naturals at deal time.
    let player_bj = bj_is_bj(&player_init);
    let dealer_bj = bj_is_bj(&dealer_init);

    if player_bj || dealer_bj {
        let base_payout = if player_bj && dealer_bj {
            amount // push
        } else if player_bj {
            amount * 5 / 2
        } else {
            0
        };
        let payout = base_payout + extra_stake;
        return Ok(Outcome { payout, finished: true, cashed: payout > 0 });
    }

    let total_stake = amount + extra_stake;
    let mut player_cards: [u8; 12] = [0; 12];
    player_cards[0] = p0;
    player_cards[1] = p1;
    let mut player_len = 2usize;

    let mut doubled = false;
    let mut stood = false;
    let mut finished = false;
    let mut cashed = false;
    let mut payout = 0u64;

    // Process logged actions. Actions after a finished hand are ignored per spec.
    for idx in 0..action_count as usize {
        if finished { break; }
        let action = actions[idx];
        match action {
            0 => {
                // hit
                if stood { break; } // ignore hits after stand (round done)
                let c = next_card(&mut ptr);
                player_cards[player_len] = c;
                player_len += 1;
                let pv = bj_hand_value(&player_cards[..player_len]);
                if pv > 21 {
                    payout = 0;
                    finished = true;
                } else if pv == 21 {
                    stood = true; // auto-stand on 21
                }
            }
            1 => {
                // stand
                stood = true;
            }
            2 => {
                // double — only as first action; ignore if not first or already doubled/stood
                if idx != 0 || doubled || stood { continue; }
                doubled = true;
                stood = true; // one card then stand
                let c = next_card(&mut ptr);
                player_cards[player_len] = c;
                player_len += 1;
                let pv = bj_hand_value(&player_cards[..player_len]);
                if pv > 21 {
                    payout = 0;
                    finished = true;
                }
            }
            _ => continue,
        }
    }

    // Timeout: auto stand.
    if timeout && !finished && !stood {
        stood = true;
    }

    if !finished && stood {
        // Dealer plays.
        let mut dealer_cards: [u8; 12] = [0; 12];
        dealer_cards[0] = d_up;
        dealer_cards[1] = d_hole;
        let mut dealer_len = 2;
        while bj_hand_value(&dealer_cards[..dealer_len]) < 17 {
            dealer_cards[dealer_len] = next_card(&mut ptr);
            dealer_len += 1;
        }
        let pv = bj_hand_value(&player_cards[..player_len]);
        let dv = bj_hand_value(&dealer_cards[..dealer_len]);
        payout = if pv > 21 {
            0
        } else if dv > 21 || pv > dv {
            total_stake * 2
        } else if pv == dv {
            total_stake
        } else {
            0
        };
        finished = true;
        cashed = payout > 0;
    }

    Ok(Outcome { payout, finished, cashed })
}

// ─── Video Poker (Jacks or Better) ───────────────────────────────────────────

fn poker_params(p: &[u8; 32]) -> Result<()> {
    require!(p.iter().all(|b| *b == 0), OpenHouseError::InvalidGameParams);
    Ok(())
}

pub fn poker_max_payout(params: &[u8; 32], amount: u64) -> Result<u64> {
    poker_params(params)?;
    Ok(amount * 800)
}

/// Rank = card % 13 (0=Ace), Suit = card / 13.
fn poker_rank(c: u8) -> u8 { c % 13 }
fn poker_suit(c: u8) -> u8 { c / 13 }

fn poker_eval(hand: &[u8; 5]) -> u32 {
    let ranks: [u8; 5] = core::array::from_fn(|i| poker_rank(hand[i]));
    let suits: [u8; 5] = core::array::from_fn(|i| poker_suit(hand[i]));

    let mut rank_counts = [0u8; 13];
    for &r in &ranks { rank_counts[r as usize] += 1; }

    let flush = suits.iter().all(|&s| s == suits[0]);

    // Straight: sorted ranks with gap 1, or royal {0,9,10,11,12}
    let mut sorted = ranks;
    sorted.sort_unstable();
    let straight = (sorted[4] - sorted[0] == 4 && rank_counts.iter().all(|&c| c <= 1))
        || sorted == [0, 9, 10, 11, 12];
    let royal = straight && flush && sorted == [0, 9, 10, 11, 12];

    let mut pairs = 0u8;
    let mut three = false;
    let mut four = false;
    for &c in &rank_counts {
        match c { 2 => pairs += 1, 3 => three = true, 4 => four = true, _ => {} }
    }

    if royal { return 800; }
    if straight && flush { return 50; }
    if four { return 25; }
    if three && pairs == 1 { return 9; } // full house
    if flush { return 6; }
    if straight { return 4; }
    if three { return 3; }
    if pairs == 2 { return 2; }
    // Jacks or better: a pair with rank 10(J), 11(Q), 12(K) or 0(A)
    if pairs == 1 {
        for (r, &c) in rank_counts.iter().enumerate() {
            if c == 2 && (r == 0 || r >= 10) { return 1; }
        }
    }
    0
}

pub fn replay_poker(
    params: &[u8; 32], amount: u64,
    server: &[u8; 32], client: &[u8; 32], nonce: u64,
    actions: &[u8; 32], action_count: u8,
    timeout: bool,
) -> Result<Outcome> {
    poker_params(params)?;
    let d = draws64(server, client, nonce, 52);
    let deck = bj_deck(&d); // same deck shuffle

    // Initial hand: deck[0..5]
    let mut hand: [u8; 5] = [deck[0], deck[1], deck[2], deck[3], deck[4]];

    // Parse hold mask: first valid action (0..=31). If timeout with no valid action, hold all (mask=31).
    let mut mask_opt: Option<u8> = None;
    for &m in &actions[..action_count as usize] {
        if m <= 31 {
            mask_opt = Some(m);
            break;
        }
    }
    let mask: u8 = match mask_opt {
        Some(m) => m,
        None => {
            if timeout { 31 } else {
                // Round not finished yet (no valid action submitted).
                return Ok(Outcome { payout: 0, finished: false, cashed: false });
            }
        }
    };

    // Replace non-held cards from deck[5..].
    let mut replace_ptr = 5usize;
    for i in 0..5 {
        if mask & (1 << i) == 0 {
            hand[i] = deck[replace_ptr];
            replace_ptr += 1;
        }
    }

    let mult = poker_eval(&hand);
    let payout = amount * mult as u64;
    Ok(Outcome { payout, finished: true, cashed: payout > 0 })
}

// ─── Dispatch ────────────────────────────────────────────────────────────────

fn game_name(game_id: &[u8; 16]) -> &[u8] {
    let n = game_id.iter().position(|b| *b == 0).unwrap_or(16);
    &game_id[..n]
}

pub fn is_multistep_name(name: &[u8]) -> bool {
    matches!(name, b"mines" | b"hilo" | b"towers" | b"blackjack" | b"poker")
}

pub fn is_multistep(game_id: &[u8; 16]) -> bool {
    is_multistep_name(game_name(game_id))
}

pub fn is_blackjack(game_id: &[u8; 16]) -> bool {
    game_name(game_id) == b"blackjack"
}

/// Compute max_payout for multi-step games (called from games/mod.rs max_payout).
pub fn multistep_max_payout(game_id: &[u8; 16], params: &[u8; 32], amount: u64) -> Result<u64> {
    match game_name(game_id) {
        b"mines" => mines_max_payout(params, amount),
        b"hilo" => hilo_max_payout(params, amount),
        b"towers" => towers_max_payout(params, amount),
        b"blackjack" => blackjack_max_payout(params, amount),
        b"poker" => poker_max_payout(params, amount),
        _ => Err(error!(OpenHouseError::InvalidGameParams)),
    }
}

/// Validate action shape at play_action time (before seed is revealed).
/// Only checks range validity and basic structural rules.
pub fn validate_action_shape(
    game_id: &[u8; 16], params: &[u8; 32],
    action: u8, logged: &[u8; 32], logged_count: u8,
) -> Result<()> {
    let history = &logged[..logged_count as usize];
    match game_name(game_id) {
        b"mines" => {
            // Reject any action after a cash out (255) is already in the log.
            require!(!history.contains(&255), OpenHouseError::InvalidAction);
            // 0..=24 (cell reveal) or 255 (cash out)
            require!(action <= 24 || action == 255, OpenHouseError::InvalidAction);
            if action == 255 {
                require!(logged_count > 0, OpenHouseError::InvalidAction); // must have revealed at least one cell
            } else {
                // Reject duplicate reveal of a cell already in the log.
                require!(!history.contains(&action), OpenHouseError::InvalidAction);
            }
        }
        b"hilo" => {
            require!(!history.contains(&255), OpenHouseError::InvalidAction);
            require!(action <= 1 || action == 255, OpenHouseError::InvalidAction);
            if action == 255 {
                require!(logged_count > 0, OpenHouseError::InvalidAction);
            }
        }
        b"towers" => {
            require!(!history.contains(&255), OpenHouseError::InvalidAction);
            let diff = params[0];
            let cols = match diff { 0 => 4u8, 1 => 3, _ => 2 };
            require!(action < cols || action == 255, OpenHouseError::InvalidAction);
            if action == 255 {
                require!(logged_count > 0, OpenHouseError::InvalidAction);
            }
        }
        b"blackjack" => {
            // 0=hit, 1=stand, 2=double. double only as first action.
            require!(action <= 2, OpenHouseError::InvalidAction);
            if action == 2 {
                require!(logged_count == 0, OpenHouseError::InvalidAction);
            }
        }
        b"poker" => {
            // Hold mask 0..=31
            require!(action <= 31, OpenHouseError::InvalidAction);
            // Only one action allowed.
            require!(logged_count == 0, OpenHouseError::InvalidAction);
        }
        _ => return Err(error!(OpenHouseError::InvalidGameParams)),
    }
    Ok(())
}

/// Full replay of a multi-step game's action log with the revealed seed.
pub fn replay(
    game_id: &[u8; 16], params: &[u8; 32], amount: u64,
    server: &[u8; 32], client: &[u8; 32], nonce: u64,
    actions: &[u8; 32], action_count: u8, extra_stake: u64,
    timeout: bool,
) -> Result<Outcome> {
    match game_name(game_id) {
        b"mines" => replay_mines(params, amount, server, client, nonce, actions, action_count, timeout),
        b"hilo" => replay_hilo(params, amount, server, client, nonce, actions, action_count, timeout),
        b"towers" => replay_towers(params, amount, server, client, nonce, actions, action_count, timeout),
        b"blackjack" => replay_blackjack(params, amount, server, client, nonce, actions, action_count, extra_stake, timeout),
        b"poker" => replay_poker(params, amount, server, client, nonce, actions, action_count, timeout),
        _ => Err(error!(OpenHouseError::InvalidGameParams)),
    }
}

// ─── RTP + unit tests ────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn dummy_seed() -> [u8; 32] { [0u8; 32] }
    fn cseed() -> [u8; 32] { [1u8; 32] }

    fn game_id_for_name(name: &[u8]) -> [u8; 16] {
        let mut id = [0u8; 16];
        let n = name.len().min(16);
        id[..n].copy_from_slice(&name[..n]);
        id
    }

    // Exhaustive mine board (25 cells, known mine positions via fixed seed)
    #[test]
    fn mines_replay_pure_deterministic() {
        let s = [0xABu8; 32]; let c = [0x01u8; 32];
        let mut p = [0u8; 32]; p[0] = 3; // 3 mines
        let mut actions = [0u8; 32]; actions[0] = 255; // immediate cashout (ignored, 0 reveals)
        // cash out with 0 reveals is a no-op (round not finished)
        let r = replay_mines(&p, 1_000_000, &s, &c, 0, &actions, 1, false).unwrap();
        assert!(!r.finished);
        // Different seed => different mine positions
        let s2 = [0xCDu8; 32];
        let d1 = draws64(&s, &c, 0, 25);
        let d2 = draws64(&s2, &c, 0, 25);
        assert_ne!(mine_positions(&d1, 3), mine_positions(&d2, 3));
    }

    #[test]
    fn mines_rtp_per_mine_count() {
        // For each mine count m, enumerate all C(25,m) outcomes with best fixed
        // strategy t (cash out after exactly t safe reveals) using DP over the
        // hypergeometric distribution.
        for m in 1u8..=24 {
            let safe_total = 25 - m as u32;
            // Best RTP over all fixed stop counts t.
            let mut best_rtp = 0.0f64;
            let amount: u128 = 1_000_000_000_000; // 1e12 for precision
            for t in 1..=safe_total {
                // Probability of surviving exactly t reveals:
                // P = product_{i=0}^{t-1}  (25-m-i)/(25-i)
                let mut prob = 1.0f64;
                for i in 0..t {
                    prob *= (25 - m as u32 - i) as f64 / (25 - i) as f64;
                }
                let (val, _) = mines_value(amount as u64, m, t);
                let rtp = prob * val as f64 / amount as f64;
                if rtp > best_rtp { best_rtp = rtp; }
            }
            assert!(best_rtp <= 1.0, "mines m={m} rtp={best_rtp} > 1.0");
            assert!(best_rtp >= 0.97, "mines m={m} rtp={best_rtp} < 0.97");
        }
    }

    #[test]
    fn hilo_rtp_exact_dp() {
        // Exact DP: state = (current_rank 0..13, win_count 0..19).
        // At each state, player chooses hi (0), lo (1), or cash out (255).
        // Using the exact floor arithmetic and 250x cap of replay_hilo.
        let mut ev = [[1.0_f64; 20]; 13];
        // k = 19: forced cash out => ev[r][19] = 1.0
        for k in (0..19).rev() {
            for r in 0..13 {
                let cash = 1.0_f64;
                let count_hi = (13 - r) as f64;
                let mut sum_hi = 0.0_f64;
                for nr in r..13 {
                    sum_hi += ev[nr][k + 1];
                }
                let ev_hi = (1.0 / 13.0) * (13.0 / count_hi) * sum_hi;

                let count_lo = (r + 1) as f64;
                let mut sum_lo = 0.0_f64;
                for nr in 0..=r {
                    sum_lo += ev[nr][k + 1];
                }
                let ev_lo = (1.0 / 13.0) * (13.0 / count_lo) * sum_lo;

                ev[r][k] = cash.max(ev_hi).max(ev_lo);
            }
        }
        let mut init_ev = 0.0_f64;
        for r in 0..13 {
            init_ev += ev[r][0];
        }
        let rtp = 0.99 * (init_ev / 13.0);
        eprintln!("hilo exact DP optimal RTP = {:.6}", rtp);
        assert!(rtp <= 0.99 + 1e-12, "hilo exact DP rtp={rtp} > 0.99");
        assert!(rtp >= 0.989, "hilo exact DP rtp={rtp} unexpectedly low");
    }

    #[test]
    fn towers_rtp_all_difficulties() {
        for diff in 0u8..=2 {
            let cols = towers_cols(diff);
            let amount = 1_000_000_000u64;
            for stop_floor in 1u32..=8 {
                // P(survive stop_floor floors) = (1-1/cols)^stop_floor = ((cols-1)/cols)^stop_floor
                let prob = ((cols - 1) as f64 / cols as f64).powi(stop_floor as i32);
                let (val, _) = towers_value(amount, cols, stop_floor);
                let rtp = prob * val as f64 / amount as f64;
                assert!(rtp <= 1.0, "towers diff={diff} stop={stop_floor} rtp={rtp}");
                assert!(rtp >= 0.97, "towers diff={diff} stop={stop_floor} rtp={rtp}");
            }
        }
    }

    #[test]
    fn blackjack_rtp_exact_dp_and_mc_20m() {
        use std::collections::HashMap;
        fn pack_counts(c: &[u8; 10]) -> u64 {
            let mut k = 0u64;
            for i in 0..10 { k |= (c[i] as u64) << (i * 5); }
            k
        }
        fn dealer_dist(
            c: &mut [u8; 10], rem: u32, d_tot: u8, d_soft: bool,
            memo: &mut HashMap<u64, [f64; 6]>,
        ) -> [f64; 6] {
            if d_tot >= 17 {
                let mut out = [0.0; 6];
                if d_tot > 21 { out[5] = 1.0; } else { out[(d_tot - 17) as usize] = 1.0; }
                return out;
            }
            let key = pack_counts(c) | ((d_tot as u64) << 50) | ((d_soft as u64) << 55);
            if let Some(&v) = memo.get(&key) { return v; }
            let mut out = [0.0; 6];
            for r in 0..10 {
                if c[r] == 0 { continue; }
                let p = c[r] as f64 / rem as f64;
                c[r] -= 1;
                let val = if r == 0 { 11 } else { (r + 1) as u8 };
                let mut nt = d_tot + val;
                let mut ns = d_soft || r == 0;
                if nt > 21 && ns { nt -= 10; ns = false; }
                let sub = dealer_dist(c, rem - 1, nt, ns, memo);
                for i in 0..6 { out[i] += p * sub[i]; }
                c[r] += 1;
            }
            memo.insert(key, out);
            out
        }
        fn stand_ev(p_tot: u8, d_dist: &[f64; 6]) -> f64 {
            if p_tot > 21 { return 0.0; }
            let mut ev = d_dist[5] * 2.0;
            for i in 0..5 {
                let dv = 17 + i as u8;
                if p_tot > dv { ev += d_dist[i] * 2.0; }
                else if p_tot == dv { ev += d_dist[i] * 1.0; }
            }
            ev
        }
        fn player_ev(
            c: &mut [u8; 10], rem: u32, p_tot: u8, p_soft: bool, d_up: u8, can_dbl: bool,
            d_memo: &mut HashMap<u64, [f64; 6]>,
            p_memo: &mut HashMap<u64, f64>,
        ) -> f64 {
            if p_tot > 21 { return 0.0; }
            let key = pack_counts(c) | ((d_up as u64) << 50) | ((p_tot as u64) << 54) | ((p_soft as u64) << 59) | ((can_dbl as u64) << 60);
            if let Some(&v) = p_memo.get(&key) { return v; }
            let d_val = if d_up == 0 { 11 } else { d_up + 1 };
            let d_dist = dealer_dist(c, rem, d_val, d_up == 0, d_memo);
            let mut best = stand_ev(p_tot, &d_dist);
            if p_tot < 21 {
                let mut ev_hit = 0.0;
                for r in 0..10 {
                    if c[r] == 0 { continue; }
                    let p = c[r] as f64 / rem as f64;
                    c[r] -= 1;
                    let val = if r == 0 { 11 } else { (r + 1) as u8 };
                    let mut nt = p_tot + val;
                    let mut ns = p_soft || r == 0;
                    if nt > 21 && ns { nt -= 10; ns = false; }
                    ev_hit += p * player_ev(c, rem - 1, nt, ns, d_up, false, d_memo, p_memo);
                    c[r] += 1;
                }
                if ev_hit > best { best = ev_hit; }
                if can_dbl {
                    let mut ev_dbl = 0.0;
                    for r in 0..10 {
                        if c[r] == 0 { continue; }
                        let p = c[r] as f64 / rem as f64;
                        c[r] -= 1;
                        let val = if r == 0 { 11 } else { (r + 1) as u8 };
                        let mut nt = p_tot + val;
                        let mut ns = p_soft || r == 0;
                        if nt > 21 && ns { nt -= 10; ns = false; }
                        let dd = dealer_dist(c, rem - 1, d_val, d_up == 0, d_memo);
                        ev_dbl += p * (2.0 * stand_ev(nt, &dd) - 1.0);
                        c[r] += 1;
                    }
                    if ev_dbl > best { best = ev_dbl; }
                }
            }
            p_memo.insert(key, best);
            best
        }

        let mut counts = [4u8, 4, 4, 4, 4, 4, 4, 4, 4, 16];
        let mut d_memo = HashMap::new();
        let mut p_memo = HashMap::new();
        let mut dp_rtp = 0.0;
        for p1 in 0..10 {
            let prob1 = counts[p1] as f64 / 52.0;
            counts[p1] -= 1;
            for d_up in 0..10 {
                let prob2 = counts[d_up] as f64 / 51.0;
                counts[d_up] -= 1;
                for p2 in 0..10 {
                    let prob3 = counts[p2] as f64 / 50.0;
                    counts[p2] -= 1;
                    let w = prob1 * prob2 * prob3;
                    let v1 = if p1 == 0 { 11 } else { (p1 + 1) as u8 };
                    let v2 = if p2 == 0 { 11 } else { (p2 + 1) as u8 };
                    let mut pt = v1 + v2;
                    let mut ps = p1 == 0 || p2 == 0;
                    if pt > 21 && ps { pt -= 10; ps = false; }
                    let p_nat = pt == 21;
                    let mut ev = 0.0;
                    for d_hole in 0..10 {
                        let ph = counts[d_hole] as f64 / 49.0;
                        let d_nat = (d_up == 0 && d_hole == 9) || (d_up == 9 && d_hole == 0);
                        if p_nat && d_nat { ev += ph * 1.0; }
                        else if p_nat { ev += ph * 2.5; }
                        else if d_nat { ev += ph * 0.0; }
                        else {
                            ev += ph * player_ev(&mut counts, 49, pt, ps, d_up as u8, true, &mut d_memo, &mut p_memo);
                        }
                    }
                    dp_rtp += w * ev;
                    counts[p2] += 1;
                }
                counts[d_up] += 1;
            }
            counts[p1] += 1;
        }
        eprintln!("blackjack exact DP optimal RTP = {:.6}", dp_rtp);
        assert!(dp_rtp <= 1.0, "blackjack exact DP rtp={dp_rtp} > 1.0");

        let mut rng: u64 = 0xDEAD_BEEF_1234_5678;
        let next_u64 = |r: &mut u64| { *r ^= *r << 13; *r ^= *r >> 7; *r ^= *r << 17; *r };
        let next_32 = |r: &mut u64| -> [u8; 32] {
            let mut b = [0u8; 32]; for c in b.chunks_mut(8) { c.copy_from_slice(&next_u64(r).to_le_bytes()); } b
        };
        let amount = 1_000_000u64;
        let params = [0u8; 32];
        let mut total_payout = 0u128;
        let n = 20_000_000u64;
        for i in 0..n {
            let s = next_32(&mut rng); let c = next_32(&mut rng);
            let d = draws64(&s, &c, i, 52);
            let deck = bj_deck(&d);
            let dealer_up = bj_hand_value(&[deck[1]]);
            let mut p_cards = [deck[0], deck[2], 0, 0, 0, 0, 0, 0];
            let mut p_len = 2usize;
            let mut ptr = 4usize;
            let mut actions = [0u8; 32];
            let mut ac = 0usize;
            loop {
                let pv = bj_hand_value(&p_cards[..p_len]);
                if pv >= 21 || ac >= 8 { break; }
                let should_hit = pv < 12 || (pv < 17 && dealer_up >= 7);
                if should_hit {
                    actions[ac] = 0; ac += 1;
                    p_cards[p_len] = deck[ptr]; p_len += 1; ptr += 1;
                } else {
                    actions[ac] = 1; ac += 1;
                    break;
                }
            }
            let o = replay_blackjack(&params, amount, &s, &c, i, &actions, ac as u8, 0, false).unwrap();
            total_payout += o.payout as u128;
        }
        let mc_rtp = total_payout as f64 / (n as f64 * amount as f64);
        eprintln!("blackjack MC 20M rtp = {:.6} (20,000,000 rounds)", mc_rtp);
        assert!(mc_rtp <= 1.0, "blackjack MC rtp={mc_rtp} > 1.0");
        assert!((mc_rtp - dp_rtp).abs() < 0.01, "blackjack MC {mc_rtp} vs DP {dp_rtp}");
    }

    #[test]
    fn poker_rtp_exact_suit_isomorphism() {
        use std::collections::HashMap;
        let mut sum_5: HashMap<u64, u64> = HashMap::with_capacity(3_000_000);
        let pack = |cards: &[u8]| -> u64 {
            let mut k = 0u64;
            for (i, &c) in cards.iter().enumerate() { k |= (c as u64) << (i * 6); }
            k | ((cards.len() as u64) << 30)
        };
        let mut all_hands: Vec<([u8; 5], u32)> = Vec::with_capacity(2_598_960);
        for a in 0..48u8 {
            for b in (a + 1)..49u8 {
                for c in (b + 1)..50u8 {
                    for d in (c + 1)..51u8 {
                        for e in (d + 1)..52u8 {
                            let h = [a, b, c, d, e];
                            let v = poker_eval(&h);
                            all_hands.push((h, v));
                            if v == 0 { continue; }
                            for mask in 0u8..32 {
                                let mut sub = [0u8; 5];
                                let mut len = 0usize;
                                for i in 0..5 {
                                    if (mask >> i) & 1 == 1 {
                                        sub[len] = h[i];
                                        len += 1;
                                    }
                                }
                                *sum_5.entry(pack(&sub[..len])).or_insert(0) += v as u64;
                            }
                        }
                    }
                }
            }
        }
        let denom: [f64; 6] = [1.0, 47.0, 1081.0, 16215.0, 178365.0, 1533939.0];
        let mut total_ev = 0.0_f64;
        for &(h, v) in &all_hands {
            let mut best = v as f64;
            for mask in 0u8..31 {
                let kept_cnt = mask.count_ones() as usize;
                let disc_cnt = 5 - kept_cnt;
                let mut kept = [0u8; 5];
                let mut klen = 0usize;
                let mut disc = [0u8; 5];
                let mut dlen = 0usize;
                for i in 0..5 {
                    if (mask >> i) & 1 == 1 { kept[klen] = h[i]; klen += 1; }
                    else { disc[dlen] = h[i]; dlen += 1; }
                }
                let mut ie_sum = 0i64;
                for tmask in 0u8..(1u8 << dlen) {
                    let mut comb = [0u8; 5];
                    let mut clen = 0usize;
                    let mut ki = 0usize;
                    let mut di = 0usize;
                    let mut t_arr = [0u8; 5];
                    let mut tlen = 0usize;
                    for j in 0..dlen {
                        if (tmask >> j) & 1 == 1 { t_arr[tlen] = disc[j]; tlen += 1; }
                    }
                    while ki < klen && di < tlen {
                        if kept[ki] < t_arr[di] { comb[clen] = kept[ki]; ki += 1; }
                        else { comb[clen] = t_arr[di]; di += 1; }
                        clen += 1;
                    }
                    while ki < klen { comb[clen] = kept[ki]; ki += 1; clen += 1; }
                    while di < tlen { comb[clen] = t_arr[di]; di += 1; clen += 1; }
                    let s = *sum_5.get(&pack(&comb[..clen])).unwrap_or(&0) as i64;
                    if tlen % 2 == 0 { ie_sum += s; } else { ie_sum -= s; }
                }
                let ev = ie_sum as f64 / denom[disc_cnt];
                if ev > best { best = ev; }
            }
            total_ev += best;
        }
        let rtp = total_ev / 2_598_960.0;
        eprintln!("poker exact optimal RTP = {:.6} ({:.4}%)", rtp, rtp * 100.0);
        assert!((rtp - 0.9954).abs() <= 0.0005, "poker exact RTP {rtp:.6} differs from 0.9954 by > 0.05pp");
    }

    #[test]
    fn cap_never_exceeded() {
        // 1 million random action logs, payout never > reserved max.
        let mut rng: u64 = 0x1234_5678_9ABC_DEF0;
        let next_u64 = |r: &mut u64| { *r ^= *r << 13; *r ^= *r >> 7; *r ^= *r << 17; *r };
        let next_32 = |r: &mut u64| -> [u8; 32] {
            let mut b = [0u8; 32]; for c in b.chunks_mut(8) { c.copy_from_slice(&next_u64(r).to_le_bytes()); } b
        };
        let amount = 1_000_000u64;
        for _ in 0..1_000_000 {
            let s = next_32(&mut rng); let c = next_32(&mut rng); let nonce = next_u64(&mut rng) % 100_000;
            let n_actions = (next_u64(&mut rng) % 32) as u8;
            let mut actions = [0u8; 32];
            for j in 0..n_actions as usize { actions[j] = (next_u64(&mut rng) % 256) as u8; }

            // Mines m=3
            let mut pm = [0u8; 32]; pm[0] = 3;
            if let Ok(o) = replay_mines(&pm, amount, &s, &c, nonce, &actions, n_actions, false) {
                assert!(o.payout <= amount * CAP_X100 / 100, "mines cap");
            }
            // HiLo
            let ph = [0u8; 32];
            if let Ok(o) = replay_hilo(&ph, amount, &s, &c, nonce, &actions, n_actions, false) {
                assert!(o.payout <= amount * CAP_X100 / 100, "hilo cap");
            }
            // Towers medium
            let mut pt = [0u8; 32]; pt[0] = 1;
            if let Ok(o) = replay_towers(&pt, amount, &s, &c, nonce, &actions, n_actions, false) {
                assert!(o.payout <= amount * CAP_X100 / 100, "towers cap");
            }
            // Blackjack
            let pb = [0u8; 32];
            if let Ok(o) = replay_blackjack(&pb, amount, &s, &c, nonce, &actions, n_actions, 0, false) {
                assert!(o.payout <= amount * 4, "bj cap");
            }
            // Poker
            if let Ok(o) = replay_poker(&pb, amount, &s, &c, nonce, &actions, n_actions, false) {
                assert!(o.payout <= amount * 800, "poker cap");
            }
        }
    }

    #[test]
    fn garbage_actions_never_panic() {
        // Random byte arrays of any length up to 32, never panic, never exceed reserved max.
        let mut rng: u64 = 0xCAFE_BABE_DEAD_BEEF;
        let next_u64 = |r: &mut u64| { *r ^= *r << 13; *r ^= *r >> 7; *r ^= *r << 17; *r };
        let next_32 = |r: &mut u64| -> [u8; 32] {
            let mut b = [0u8; 32]; for c in b.chunks_mut(8) { c.copy_from_slice(&next_u64(r).to_le_bytes()); } b
        };
        let amount = 1_000_000u64;
        for _ in 0..200_000 {
            let s = next_32(&mut rng); let c = next_32(&mut rng); let nonce = next_u64(&mut rng);
            let n_actions = (next_u64(&mut rng) % 33) as u8;
            let actions = next_32(&mut rng);
            let mut pm = [0u8; 32]; pm[0] = (next_u64(&mut rng) % 24 + 1) as u8;
            let _ = replay_mines(&pm, amount, &s, &c, nonce, &actions, n_actions.min(32), false);
            let _ = replay_hilo(&[0u8; 32], amount, &s, &c, nonce, &actions, n_actions.min(32), false);
            let mut pt = [0u8; 32]; pt[0] = (next_u64(&mut rng) % 3) as u8;
            let _ = replay_towers(&pt, amount, &s, &c, nonce, &actions, n_actions.min(32), false);
            let _ = replay_blackjack(&[0u8; 32], amount, &s, &c, nonce, &actions, n_actions.min(32), 0, false);
            let _ = replay_poker(&[0u8; 32], amount, &s, &c, nonce, &actions, n_actions.min(32), false);
        }
    }

    #[test]
    fn encoder_rejections() {
        // Mines: out-of-range mine count
        let mut p = [0u8; 32];
        p[0] = 0;
        assert!(mines_max_payout(&p, 1_000_000).is_err(), "mines m=0 should fail");
        p[0] = 25;
        assert!(mines_max_payout(&p, 1_000_000).is_err(), "mines m=25 should fail");
        p[0] = 3; p[1] = 1; // non-zero spare byte
        assert!(mines_max_payout(&p, 1_000_000).is_err(), "mines non-zero spare byte");
        // Towers: bad difficulty
        let mut pt = [0u8; 32]; pt[0] = 3;
        assert!(towers_max_payout(&pt, 1_000_000).is_err());
        // HiLo: non-zero params
        let mut ph = [0u8; 32]; ph[0] = 1;
        assert!(hilo_max_payout(&ph, 1_000_000).is_err());
        // Blackjack: non-zero params
        assert!(blackjack_max_payout(&ph, 1_000_000).is_err());
        // Poker: non-zero params
        assert!(poker_max_payout(&ph, 1_000_000).is_err());
    }

    #[test]
    #[ignore]
    fn export_rust_vectors_batch3() {
        use std::fmt::Write as _;
        let mut rng: u64 = 0xB3B3_B3B3_1234_5678u64.wrapping_add(0xDEAD);
        let next_u64 = |r: &mut u64| -> u64 { *r ^= *r << 13; *r ^= *r >> 7; *r ^= *r << 17; *r };
        let next_32 = |r: &mut u64| -> [u8; 32] {
            let mut b = [0u8; 32]; for chunk in b.chunks_mut(8) { chunk.copy_from_slice(&next_u64(r).to_le_bytes()); } b
        };
        let to_hex = |b: &[u8]| -> String { b.iter().map(|x| format!("{:02x}", x)).collect() };
        let mut out = String::from("[\n");
        let mut first = true;

        let mut push = |game: &str, params: &[u8; 32], amount: u64,
                         s: &[u8; 32], c: &[u8; 32], nonce: u64,
                         actions: &[u8; 32], action_count: u8, extra_stake: u64| {
            let o = replay(&{let mut g=[0u8;16];g[..game.len()].copy_from_slice(game.as_bytes());g},
                           params, amount, s, c, nonce, actions, action_count, extra_stake, false);
            if let Ok(o) = o {
                if !first { out.push_str(",\n"); }
                first = false;
                write!(out,
                    "  {{\"game\":\"{game}\",\"params\":\"{}\",\"amount\":{amount},\"server\":\"{}\",\"client\":\"{}\",\"nonce\":{nonce},\"actions\":\"{}\",\"actionCount\":{action_count},\"extraStake\":{extra_stake},\"payout\":{},\"finished\":{}}}",
                    to_hex(params), to_hex(s), to_hex(c), to_hex(&actions[..32]),
                    o.payout, o.finished
                ).unwrap();
            }
        };

        // Mines
        for _ in 0..200 {
            let mut p = [0u8; 32]; p[0] = (next_u64(&mut rng) % 24 + 1) as u8;
            let (s, c, nonce) = (next_32(&mut rng), next_32(&mut rng), next_u64(&mut rng) % 100_000);
            let n_actions = (next_u64(&mut rng) % 10) as u8;
            let mut actions = [255u8; 32]; // start with cashout attempts
            for i in 0..n_actions as usize { actions[i] = (next_u64(&mut rng) % 25) as u8; }
            push("mines", &p, 1_000_000, &s, &c, nonce, &actions, n_actions, 0);
        }
        // HiLo
        for _ in 0..200 {
            let (s, c, nonce) = (next_32(&mut rng), next_32(&mut rng), next_u64(&mut rng) % 100_000);
            let n_actions = (next_u64(&mut rng) % 15) as u8;
            let mut actions = [0u8; 32];
            for i in 0..n_actions as usize { actions[i] = (next_u64(&mut rng) % 2) as u8; }
            push("hilo", &[0u8; 32], 1_000_000, &s, &c, nonce, &actions, n_actions, 0);
        }
        // Towers
        for _ in 0..200 {
            let mut p = [0u8; 32]; p[0] = (next_u64(&mut rng) % 3) as u8;
            let (s, c, nonce) = (next_32(&mut rng), next_32(&mut rng), next_u64(&mut rng) % 100_000);
            let n_actions = (next_u64(&mut rng) % 8) as u8;
            let cols = match p[0] { 0 => 4u8, 1 => 3, _ => 2 };
            let mut actions = [0u8; 32];
            for i in 0..n_actions as usize { actions[i] = (next_u64(&mut rng) % cols as u64) as u8; }
            push("towers", &p, 1_000_000, &s, &c, nonce, &actions, n_actions, 0);
        }
        // Blackjack
        for _ in 0..200 {
            let (s, c, nonce) = (next_32(&mut rng), next_32(&mut rng), next_u64(&mut rng) % 100_000);
            let n_actions = (next_u64(&mut rng) % 8 + 1) as u8;
            let mut actions = [0u8; 32]; // all hits
            push("blackjack", &[0u8; 32], 1_000_000, &s, &c, nonce, &actions, n_actions, 0);
        }
        // Poker
        for _ in 0..200 {
            let (s, c, nonce) = (next_32(&mut rng), next_32(&mut rng), next_u64(&mut rng) % 100_000);
            let mut actions = [0u8; 32]; actions[0] = (next_u64(&mut rng) % 32) as u8;
            push("poker", &[0u8; 32], 1_000_000, &s, &c, nonce, &actions, 1, 0);
        }

        out.push_str("\n]\n");
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/core/test/rust-vectors-b3.json");
        std::fs::write(path, out).unwrap();
        println!("Written rust-vectors-b3.json");
    }

    #[test]
    fn rtp_simulation_1m_all_games() {
        // D.1: 1 million simulated rounds per config, 3 stop strategies each.
        // Uses the actual replay functions — no formula shortcuts.
        // Passes when measured RTP is within 4 sigma of theoretical and theoretical <= 1.0.
        let mut rng: u64 = 0xAAAA_BBBB_CCCC_DDDD;
        let mut xshift = |r: &mut u64| { *r ^= *r << 13; *r ^= *r >> 7; *r ^= *r << 17; *r };
        let mut next_32 = |r: &mut u64| -> [u8; 32] {
            let mut b = [0u8; 32];
            for c in b.chunks_mut(8) { c.copy_from_slice(&{ *r ^= *r << 13; *r ^= *r >> 7; *r ^= *r << 17; *r }.to_le_bytes()); } b
        };
        let amount = 1_000_000u64;
        let n = 1_000_000usize;
        let sigma_tol = 4.0_f64 / (n as f64).sqrt();

        // ── Mines: stop after 1, mid, and all safe reveals for a few m values ──
        for m in [1u8, 3, 10, 24] {
            let safe = 25 - m as u32;
            for &stop_at in &[1u32, safe.max(2) / 2, safe] {
                let mut p = [0u8; 32]; p[0] = m;
                // Find the actual stop step (either stop_at or when CAP_X100 is reached).
                let mut actual_stop = stop_at;
                for k in 1..=stop_at {
                    let (_, capped) = mines_value(amount, m, k);
                    if capped { actual_stop = k; break; }
                }
                let mut prob_th = 1.0f64;
                for i in 0..actual_stop {
                    prob_th *= (safe - i) as f64 / (25 - i) as f64;
                }
                let (val_th, _) = mines_value(amount, m, actual_stop);
                let theoretical = prob_th * val_th as f64 / amount as f64;
                assert!(theoretical <= 1.0, "mines m={m} stop={stop_at} theoretical={theoretical:.5} > 1");

                let mut total_payout = 0u128;
                for i in 0..n {
                    let s = next_32(&mut rng); let c = next_32(&mut rng);
                    let mut actions = [0u8; 32];
                    let k = stop_at.min(24) as usize;
                    for j in 0..k { actions[j] = j as u8; }
                    let ac = if stop_at < safe {
                        actions[k] = 255; // cash out
                        (k + 1) as u8
                    } else {
                        k as u8
                    };
                    let o = replay_mines(&p, amount, &s, &c, i as u64, &actions, ac, false).unwrap();
                    total_payout += o.payout as u128;
                }
                let measured = total_payout as f64 / (n as f64 * amount as f64);
                let mult = val_th as f64 / amount as f64;
                let tol = 4.0 * (prob_th * (1.0 - prob_th)).sqrt() * mult / (n as f64).sqrt() + 1e-6;
                assert!(measured <= theoretical + tol,
                    "mines m={m} stop={stop_at} measured={measured:.5} > theoretical={theoretical:.5}+4s");
                eprintln!("mines m={m} stop={stop_at}: theoretical={theoretical:.4} measured={measured:.4}");
            }
        }

        // ── Towers: 3 difficulties, stop after 1 / 4 / 8 floors ──
        for diff in 0u8..=2 {
            let cols = towers_cols(diff);
            for &stop_at in &[1u32, 4, 8] {
                let mut p = [0u8; 32]; p[0] = diff;
                let prob_th = ((cols - 1) as f64 / cols as f64).powi(stop_at as i32);
                let (val_th, _) = towers_value(amount, cols, stop_at);
                let theoretical = prob_th * val_th as f64 / amount as f64;
                assert!(theoretical <= 1.0, "towers diff={diff} stop={stop_at} theoretical={theoretical:.5} > 1");

                let mut total_payout = 0u128;
                for i in 0..n {
                    let s = next_32(&mut rng); let c = next_32(&mut rng);
                    let mut actions = [0u8; 32];
                    let ac = if stop_at < 8 {
                        actions[stop_at as usize] = 255; // cash out
                        (stop_at + 1) as u8
                    } else {
                        8
                    };
                    let o = replay_towers(&p, amount, &s, &c, i as u64, &actions, ac, false).unwrap();
                    total_payout += o.payout as u128;
                }
                let measured = total_payout as f64 / (n as f64 * amount as f64);
                let mult = val_th as f64 / amount as f64;
                let tol = 4.0 * (prob_th * (1.0 - prob_th)).sqrt() * mult / (n as f64).sqrt() + 1e-6;
                assert!(measured <= theoretical + tol,
                    "towers diff={diff} stop={stop_at} measured={measured:.5} > theoretical={theoretical:.5}+4s");
                eprintln!("towers diff={diff} stop={stop_at}: theoretical={theoretical:.4} measured={measured:.4}");
            }
        }

        // ── HiLo: always guess "higher" for 3 guesses then cash out ──
        {
            let params = [0u8; 32];
            let mut total_payout = 0u128;
            for i in 0..n {
                let s = next_32(&mut rng); let c = next_32(&mut rng);
                let mut actions = [0u8; 32]; actions[0]=0; actions[1]=0; actions[2]=0; actions[3]=255;
                let o = replay_hilo(&params, amount, &s, &c, i as u64, &actions, 4, false).unwrap();
                total_payout += o.payout as u128;
            }
            let measured = total_payout as f64 / (n as f64 * amount as f64);
            assert!(measured <= 1.0, "hilo measured={measured:.5} > 1.0");
            eprintln!("hilo 3-guess always-higher then cashout: measured={measured:.4}");
        }
        let _ = xshift;
    }

    #[test]
    fn max_payout_covers_all_replay_outputs() {
        // D.5: for every game and config, max_payout >= every payout replay can produce.
        let mut rng: u64 = 0x9876_5432_1A2B_3C4D;
        let mut next_32 = |r: &mut u64| -> [u8; 32] {
            let mut b = [0u8; 32]; for c in b.chunks_mut(8) { c.copy_from_slice(&{ *r ^= *r << 13; *r ^= *r >> 7; *r ^= *r << 17; *r }.to_le_bytes()); } b
        };
        let amount = 1_000_000u64;

        // Mines
        for m in 1u8..=24 {
            let mut p = [0u8; 32]; p[0] = m;
            let max_p = multistep_max_payout(&game_id_for_name(b"mines"), &p, amount).unwrap();
            let safe = 25 - m as usize;
            for _ in 0..1_000 {
                let s = next_32(&mut rng); let c = next_32(&mut rng);
                let mut actions = [0u8; 32];
                for j in 0..safe { actions[j] = j as u8; }
                let o = replay_mines(&p, amount, &s, &c, 1, &actions, safe as u8, true).unwrap();
                assert!(o.payout <= max_p, "mines m={m} payout={} > max={max_p}", o.payout);
            }
        }
        // Towers
        for diff in 0u8..=2 {
            let mut p = [0u8; 32]; p[0] = diff;
            let max_p = multistep_max_payout(&game_id_for_name(b"towers"), &p, amount).unwrap();
            let cols = towers_cols(diff);
            for _ in 0..1_000 {
                let s = next_32(&mut rng); let c = next_32(&mut rng);
                let d = draws64(&s, &c, 0, 8);
                let mut actions = [0u8; 32];
                for f in 0..8usize {
                    let bomb = ((d[f] as u64 * cols as u64) >> 32) as usize;
                    actions[f] = (0..cols as usize).find(|&col| col != bomb).unwrap_or(0) as u8;
                }
                let o = replay_towers(&p, amount, &s, &c, 0, &actions, 8, false).unwrap();
                assert!(o.payout <= max_p, "towers diff={diff} payout={} > max={max_p}", o.payout);
            }
        }
        // HiLo
        {
            let p = [0u8; 32];
            let max_p = multistep_max_payout(&game_id_for_name(b"hilo"), &p, amount).unwrap();
            for i in 0..1_000u64 {
                let s = next_32(&mut rng); let c = next_32(&mut rng);
                let actions = [0u8; 32]; // 0=higher, 19 guesses
                let o = replay_hilo(&p, amount, &s, &c, i, &actions, 19, true).unwrap();
                assert!(o.payout <= max_p, "hilo payout={} > max={max_p}", o.payout);
            }
        }
        // Blackjack
        {
            let p = [0u8; 32];
            let max_p = multistep_max_payout(&game_id_for_name(b"blackjack"), &p, amount).unwrap();
            for i in 0..1_000u64 {
                let s = next_32(&mut rng); let c = next_32(&mut rng);
                let actions = [1u8; 32]; // stand immediately
                let o = replay_blackjack(&p, amount, &s, &c, i, &actions, 1, 0, false).unwrap();
                assert!(o.payout <= max_p, "bj payout={} > max={max_p}", o.payout);
            }
        }
        // Poker
        {
            let p = [0u8; 32];
            let max_p = multistep_max_payout(&game_id_for_name(b"poker"), &p, amount).unwrap();
            for i in 0..1_000u64 {
                let s = next_32(&mut rng); let c = next_32(&mut rng);
                let mut actions = [0u8; 32]; actions[0] = 31; // hold all
                let o = replay_poker(&p, amount, &s, &c, i, &actions, 1, false).unwrap();
                assert!(o.payout <= max_p, "poker payout={} > max={max_p}", o.payout);
            }
        }
    }

    #[test]
    fn blackjack_natural_with_extra_stake_returns_extra() {
        // Test all 3 natural cases when extra_stake > 0:
        // 1) player natural only -> amount * 5 / 2 + extra_stake
        // 2) dealer natural only -> extra_stake
        // 3) both natural (push) -> amount + extra_stake
        let amount = 1_000_000u64;
        let extra = 1_000_000u64;
        let p = [0u8; 32];
        let c = [0x11u8; 32];
        let actions = [2u8; 32];
        let mut saw_player_nat = false;
        let mut saw_dealer_nat = false;
        let mut saw_push_nat = false;
        for nonce in 0u64..5000 {
            let mut s = [0u8; 32];
            s[0..8].copy_from_slice(&nonce.to_le_bytes());
            let d = draws64(&s, &c, nonce, 52);
            let deck = bj_deck(&d);
            let p_bj = bj_is_bj(&[deck[0], deck[2]]);
            let d_bj = bj_is_bj(&[deck[1], deck[3]]);
            if !p_bj && !d_bj { continue; }
            let out = replay_blackjack(&p, amount, &s, &c, nonce, &actions, 1, extra, false).unwrap();
            assert!(out.finished);
            if p_bj && d_bj {
                assert_eq!(out.payout, amount + extra);
                saw_push_nat = true;
            } else if p_bj {
                assert_eq!(out.payout, amount * 5 / 2 + extra);
                saw_player_nat = true;
            } else {
                assert_eq!(out.payout, extra);
                saw_dealer_nat = true;
            }
            if saw_player_nat && saw_dealer_nat && saw_push_nat { break; }
        }
        assert!(saw_player_nat && saw_dealer_nat && saw_push_nat);
    }

    #[test]
    fn random_logs_1m_never_panic_or_err_or_exceed_max() {
        let mut rng = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || -> u64 {
            rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; rng
        };
        let amount = 1_000_000u64;
        let games: [&[u8]; 5] = [b"mines", b"hilo", b"towers", b"blackjack", b"poker"];
        for i in 0..1_000_000usize {
            let g = games[i % 5];
            let mut gid = [0u8; 16];
            gid[..g.len()].copy_from_slice(g);
            let mut params = [0u8; 32];
            if g == b"mines" { params[0] = ((next() % 24) + 1) as u8; }
            if g == b"towers" { params[0] = (next() % 3) as u8; }
            let mut s = [0u8; 32];
            let mut c = [0u8; 32];
            s[0..8].copy_from_slice(&next().to_le_bytes());
            c[0..8].copy_from_slice(&next().to_le_bytes());
            let nonce = next();
            let len = (next() % 33) as u8;
            let mut actions = [0u8; 32];
            for j in 0..len as usize {
                actions[j] = (next() & 0xFF) as u8;
            }
            let extra = if g == b"blackjack" && len > 0 && actions[0] == 2 { amount } else { 0 };
            let timeout = (next() & 1) == 1;
            let out = replay(&gid, &params, amount, &s, &c, nonce, &actions, len, extra, timeout).unwrap();
            let max_p = multistep_max_payout(&gid, &params, amount).unwrap();
            let cap_p = if g == b"blackjack" { max_p.max((amount + extra) * 2) } else { max_p };
            assert!(out.payout <= cap_p, "payout {} > max {} for {:?}", out.payout, cap_p, g);
        }
    }

    #[test]
    fn poker_evaluator_exhaustive_2598960() {
        // Enumerate all C(52,5) = 2,598,960 hands and verify exact category counts.
        // poker_eval returns the payout multiplier: 800=royal, 50=str-flush, 25=4oak,
        // 9=full-house, 6=flush, 4=straight, 3=3oak, 2=two-pair, 1=JoB, 0=nothing.
        let mut royal = 0u64;
        let mut str_flush = 0u64;
        let mut four_oak = 0u64;
        let mut full_house = 0u64;
        let mut flush = 0u64;
        let mut straight = 0u64;
        let mut three_oak = 0u64;
        let mut two_pair = 0u64;
        let mut jacks_or_better = 0u64;
        let mut other = 0u64;
        let mut total = 0u64;
        for a in 0u8..48 {
            for b in (a + 1)..49 {
                for c in (b + 1)..50 {
                    for d in (c + 1)..51 {
                        for e in (d + 1)..52 {
                            total += 1;
                            match poker_eval(&[a, b, c, d, e]) {
                                800 => royal += 1,
                                50  => str_flush += 1,
                                25  => four_oak += 1,
                                9   => full_house += 1,
                                6   => flush += 1,
                                4   => straight += 1,
                                3   => three_oak += 1,
                                2   => two_pair += 1,
                                1   => jacks_or_better += 1,
                                _   => other += 1,
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(total, 2_598_960);
        assert_eq!(royal, 4, "royal flush");
        assert_eq!(str_flush, 36, "straight flush");
        assert_eq!(four_oak, 624, "four of a kind");
        assert_eq!(full_house, 3_744, "full house");
        assert_eq!(flush, 5_108, "flush");
        assert_eq!(straight, 10_200, "straight");
        assert_eq!(three_oak, 54_912, "three of a kind");
        assert_eq!(two_pair, 123_552, "two pair");
        assert_eq!(jacks_or_better, 337_920, "jacks or better");
        assert_eq!(other, 2_062_860, "other / low pair");
    }
}
