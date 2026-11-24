# OpenHouse games rulebook

This is the contract for putting every game on chain. An agent that follows it to the letter produces games that are random, verifiable, solvent and testable. Read `AUDIT.md` and `TECH.md` first. Where this file says MUST, skipping it is a bug.

## A. Universal rules (every game, no exceptions)

A1. Randomness only comes from the canonical stream, never from `Math.random`, the clock, slot numbers, or anything the settler or the player can choose after the fact.
`mac(round) = HMAC_SHA256(key = server_seed, msg = client_seed(32) || nonce(u64 LE) || round(u32 LE))`.
Draw `i` is the big endian u32 at bytes `[4*(i%8), 4*(i%8)+4)` of `mac(round = i / 8)`. Rust: `games::fair::draws`. TypeScript: `u32s`. Both MUST give identical numbers for the same inputs. A parity test with Node-generated vectors is required for every new game.

A2. Mapping a draw to a range of `n` outcomes is always `floor(u * n / 2^32)`, computed in integers as `(u as u64 * n as u64) >> 32`. Never use `%`, never use floats on chain.

A3. Seeds. The server seed is committed (hash on chain) before the bet. The client seed is chosen by the player inside the bet transaction. The seed is revealed only at settle and the program checks `sha256(seed) == commitment`. One commitment is used once.

A4. Money maths is integers only (u64, u128 for intermediate). Rounding is always floor in the house's favour. Multipliers are stored x100 (or exact integer multiples for roulette).

A5. Max payout. `place_bet` MUST compute the exact maximum total payout of the bet from the parameters and reserve it. The client must send the same number or the transaction fails. `settle_bet` MUST fail if the computed payout exceeds it. The k rule (risk of one bet is at most `k_bps` of free vault) applies to all games.

A6. RTP. Every payout table must be checked by a test that computes exact RTP with integers. Required: `98.0% < RTP < 100%` for every table and every parameter combination. House edge is never negative. Target about 99%. Roulette single zero is 97.30% by its nature, say so on its page.

A7. A game is "live" only when all of these exist: Rust rules + tests, TypeScript parity + tests, runtime test in `tests/flow.rs`, frontend wired through `settleBet`, listed in `LIVE_GAMES`, settler accepts it (`gameKind`). Until then it stays practice only and the site says so.

A8. Parameters are a fixed 32 byte array. Every byte is validated. Unknown or out of range input returns `InvalidGameParams`. No default values.

A9. Randomness proof tests (required per game): over at least 20,000 nonces, every outcome bucket must be within 5 standard deviations of expected, and the mean payout must be within tolerance of the RTP. See `draws_are_uniform_for_every_mapping`.

A10. Frontend animation may use `Math.random` for sound, particles and decoration only. Any value that decides money comes from the settle result. The page MUST re-derive the result from the revealed seed and warn on mismatch (`verifyLocally`).

A11. Honest limit that stays in every doc: the settler knows its seed before the bet and can refuse to settle. It cannot choose or predict the result (it does not know the client seed). The player gets a refund after 120 seconds. Closing this needs a VRF or bonded settlers.

## B. Games already specified and implemented

All multipliers are total payout, stake included, x100 unless stated.

| Game | game_id | params | draws | rule |
|---|---|---|---|---|
| Dice | `dice` | `[0]` 0 over 1 under, `[1..3]` target bp u16 LE 200..9800 | 1 | roll = `(u*10000)>>32`; under wins if roll < target; over wins if roll >= 10000 - target; pays `99*amount/ (target/100)` |
| Flip | `flip` | `[0]` 0 heads 1 tails | 1 | tails if `u >= 2^31`; pays 1.98x |
| Limbo / Crash | `limbo`, `crash` | `[0..4]` target x100 u32 LE 101..100000 | 1 | crash x100 = `(99<<32)/(2^32-u)`; wins if crash >= target; pays target |
| Wheel | `wheel` | `[0]` risk 0..2 | 1 | segment = `(u*24)>>32`; pays `WHEEL[risk][segment]` |
| Plinko | `plinko` | `[0]` rows in {8,10,12,14,16}, `[1]` risk 0..2 | rows | draw i goes right if `u >= 2^31`; k = number of rights; pays `PLINKO[rows][risk][k]` |
| Keno | `keno` | `[0..8]` u64 LE mask, number n is bit n-1, 1..10 picks of 1..40 | 10 | partial Fisher Yates over 1..40: for i in 0..10, `j = i + ((u_i*(40-i))>>32)`, swap, drawn[i] = a[i]; hits = popcount(picks AND drawn); pays `KENO[picks-1][hits]` |
| Roulette | `roulette` | up to 10 entries of `[slot u8, units u16 LE]`, units 0 ends | 1 | number = `(u*37)>>32` (0..36); stake must equal `sum(units) * 1,000,000` lamports; entry pays `units * 1,000,000 * mult(slot, number)` |

Roulette slots: 0..36 straight (36x), 37 red, 38 black, 39 odd, 40 even, 41 low 1-18, 42 high 19-36 (all 2x), 43 to 45 dozens, 46 to 48 columns (3x). Zero loses every outside bet. Red numbers: 1,3,5,7,9,12,14,16,18,19,21,23,25,27,30,32,34,36. No duplicate slots in one bet. Max payout = max over the 37 numbers of the total.

Tables are generated by `packages/core/scripts/gen-tables.mjs` into `programs/openhouse/src/games/tables.rs` and `packages/core/src/tables.ts`. Never edit them by hand. Change the formula, regenerate, re-run the RTP tests.

Status of the above: Rust implemented and unit tested. TypeScript port, runtime test, settler `gameKind` and frontend wiring are still to do (section D).

## C. Games still to port, exactly how

For every game below: copy the rules and tables from `index.html` (function names and lines given), do not invent new ones. The frontend rules are the product rules.

### C1. Single draw games (same pattern as Wheel and Roulette)

| Game | id | frontend source | notes |
|---|---|---|---|
| Slots 777, Hot Reels, Vault Break | `slots777`, `hotreels`, `vaultbreak` | `slotGame(...)` near line 8672, its config tables (reel strips, paytable, paylines) | Reels: one draw per reel, `index = (u*strip_len)>>32`. Paytable becomes a const table. Max payout = best line combination given the paylines, computed by brute force over all strip positions in a test and hard coded. Top multipliers in the cap table: 500x, 275x, 650x. |
| Scratch | `scratch` | `GAME_UI.scratch` line 8874 | Cells drawn independently, one draw per cell, same mapping. Payout table const. Cap 200x. |
| Sic Bo | `sicbo` | `GAME_UI.sicbo` line 8833 | Three dice: draws 0..3, `die = ((u*6)>>32)+1`. Bet slots like roulette entries (single number, totals, big/small, triples). Same `[slot, units]` encoding, same unit rule. Cap 12x per the table. Triple rule for big/small MUST match the UI. |
| Dragon Tiger | `dragon` | line 8808 | Two cards, each card = rank `((u*13)>>32)` 0..12 and a suit from the next draw; compare ranks. Tie rule and tie payout from the UI. Cap 12.8x. |
| Baccarat | `baccarat` | line 8750 | Third card rules exactly as in the UI (player draws on 0-5, banker table). Draw cards in a fixed order: P1, B1, P2, B2, P3, B3 using draws 0..5 (card = rank draw only, ignore suits). Banker commission as in the UI. Cap 9x. |

Rules for cards (all card games): deck is infinite (independent draws), `rank = (u*13)>>32` with 0 = ace ... 12 = king, suit = `(u2*4)>>32` using the draw after. If the UI uses a finite shuffled deck, use the same Fisher Yates as Keno over 52 cards and say so in the code comment.

### C2. Multi step games (Mines, HiLo, Towers, Blackjack, Poker)

These need a player decision between draws. The only design allowed is action log replay:

1. `OpenBet` gets `actions: [u8; 32]` and `action_count: u8` (and a `last_action_at: i64`). This changes the Player account size. It needs a new program id or a versioned migration, plan it, test the layout test.
2. New instruction `play_action(action: u8)`, signed by the player or their session key, valid only while a bet is open, appends the action, fails when the log is full or the game says the round is over.
3. The game rules are a pure function `replay(kind, params, amount, server_seed, client_seed, nonce, actions) -> Result<Outcome { payout, finished }>` in `games/`. Cards, mines and tower layouts all come from the draw stream by index, so the same seed and the same actions always give the same result. The program never needs the server seed until settle.
4. `settle_bet` replays the full log and pays. If the log ends with a cash out action, the payout is that step's multiplier. If the replay says the round is lost, payout is 0. An unfinished round at settle time is an error unless the timeout path below applies.
5. Cash out is itself an action. Reserved risk is the maximum payout over every possible continuation, computed up front (top of the multiplier ladder times the stake) and the k rule applies to it.
6. Abandoned rounds: after `ROUND_TIMEOUT` (use 10 minutes) of no action, anyone may call `force_settle` which auto cashes out at the current step for Mines, HiLo and Towers, and auto stands for Blackjack and Poker. The refund path stays for a settler that never reveals.
7. Hidden information: the player UI needs to know whether a tile is safe. The settler may tell the player each step off chain, but this is only a convenience. The chain recomputes everything at settle, so a lying settler cannot change the payout, only waste the player's time. The page MUST compare the chain result with what the settler said and show the chain result.

Per game specifics:
- Mines (`mines`, line 8183): mine positions from `minePos` (Fisher Yates over 25 cells, draw i for position i, exactly as the UI function). Params `[0]` mine count 1..24. Actions: reveal cell `0..24`, cash out `255`. Multiplier ladder from the UI formula (probability of surviving k reveals, times 0.99), stored as a table of 25 x 24 integers and RTP checked per mine count.
- HiLo (`hilo`, line 8390): card `i` from draws `2i, 2i+1` as in the UI `iCard`. Actions: higher, lower, skip (if the UI has it), cash out. Ladder from the UI.
- Towers (`towers`, line 8430): per level layout from one draw per level using the UI's difficulty (tiles per row, safe tiles). Actions: pick tile `0..n`, cash out.
- Blackjack (`blackjack`, line 8679): cards in order from the stream (player 1, dealer up, player 2, dealer hole, then hits). Actions: hit, stand, double, split only if the UI has it. Payout rules (3:2, dealer stands on, insurance) exactly as the UI. If the UI has no insurance or split, the program must not either. Cap 5x per hand includes doubles.
- Video poker (`poker`, line 8781): deal five cards from draws 0..4, hold mask action (5 bits), replacement cards from the next draws in order. Paytable from the UI, cap 800x.

### C3. Everything else
Crash (`crash`, line 8242) is the same rule as Limbo on chain. The animation is cosmetic. Do not give it a different distribution.

## D. Work order and definition of done

Per game, in this order, and do not move on until each step is green:
1. Rules and tables (generator if the table is computed).
2. Rust `max_payout` and `settle_payout` arms. `cargo test --lib` including RTP and uniformity tests.
3. TypeScript: `encodeX`, `resolve`, `winPayout`, parity test against Rust (export vectors from Rust, check them in TS, or Node vectors checked by both).
4. `tests/flow.rs` runtime test: 200 mixed bets, vault lamports reconcile after every step, payouts equal the TS prediction, plus the attack cases for that game's params (bad params, stake mismatch).
5. Settler: `gameKind` accepts it; `/bet/settle` unchanged.
6. Frontend: build params in `toParams`, add id to `LIVE_GAMES`, replace the local `fdraw` flow in the game's `go` handler with `settleBet`, handle `d.cancelled`, show the on-chain result, remove practice mode for it.
7. Headless Chrome test: page loads, no console errors, bet button without a wallet asks to connect.
8. Rebuild `vendor/openhouse-core.js`, recompute its SRI hash in `index.html`, rebuild the settler bundle and `api/index.js`.
9. Devnet run by the owner using the checklist below. Only then the next batch.

Batches: (1) Wheel, Roulette, Plinko, Keno. (2) Slots x3, Scratch, Sic Bo, Dragon Tiger, Baccarat. (3) Mines, HiLo, Towers, Blackjack, Poker, plus the VRF or bonded settler work.

## E. Owner test checklist per batch (devnet, Phantom)

For each new game:
1. Connect, deposit 0.5 SOL, bet the smallest allowed amount. A result appears, balance changes by the right amount.
2. On the Fair page, paste the revealed server seed, client seed and nonce. The result matches what the game showed.
3. Play 30 rounds. Wins and losses both appear, no two identical long streaks that look stuck.
4. Make a bet over the vault cap. It is refused with a plain sentence.
5. Open the bet transaction in the Solana explorer. It shows `place_bet` then `settle_bet` from the OpenHouse program.
6. Close the tab right after placing a bet. After 2 minutes the bet can be settled or refunded, and the balance is right.
7. Withdraw everything. Wallet receives it.
Report any failing step with a screenshot and the transaction signature.
