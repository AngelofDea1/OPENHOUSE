import { u32s } from './fair';

export const CAP_X100 = 25_000n;

export interface MultistepOutcome {
  payout: bigint;
  finished: boolean;
  cashed: boolean;
  detail: Record<string, any>;
}

export type MultistepGame = 'mines' | 'hilo' | 'towers' | 'blackjack' | 'poker';

export function isMultistepGame(id: string): id is MultistepGame {
  return id === 'mines' || id === 'hilo' || id === 'towers' || id === 'blackjack' || id === 'poker';
}

export function encodeMines(mines: number): Uint8Array {
  if (!Number.isInteger(mines) || mines < 1 || mines > 24) throw new Error('mines must be 1..24');
  const p = new Uint8Array(32);
  p[0] = mines;
  return p;
}
export function encodeHiLo(): Uint8Array { return new Uint8Array(32); }
export function encodeTowers(difficulty: 0 | 1 | 2): Uint8Array {
  if (!Number.isInteger(difficulty) || difficulty < 0 || difficulty > 2) throw new Error('towers difficulty must be 0, 1, or 2');
  const p = new Uint8Array(32);
  p[0] = difficulty;
  return p;
}
export function encodeBlackjack(): Uint8Array { return new Uint8Array(32); }
export function encodePoker(): Uint8Array { return new Uint8Array(32); }

function minesValue(amount: bigint, m: number, k: number): { value: bigint; capped: boolean } {
  const cap = (amount * CAP_X100) / 100n;
  let v = (amount * 99n) / 100n;
  const mb = BigInt(m);
  for (let i = 0; i < k; i++) {
    const ib = BigInt(i);
    v = (v * (25n - ib)) / (25n - mb - ib);
    if (v >= cap) return { value: cap, capped: true };
  }
  return { value: v, capped: false };
}

function towersCols(diff: number): number {
  return diff === 0 ? 4 : diff === 1 ? 3 : 2;
}

function towersValue(amount: bigint, cols: number, k: number): { value: bigint; capped: boolean } {
  const cap = (amount * CAP_X100) / 100n;
  let v = (amount * 99n) / 100n;
  const cb = BigInt(cols);
  for (let i = 0; i < k; i++) {
    v = (v * cb) / (cb - 1n);
    if (v >= cap) return { value: cap, capped: true };
  }
  return { value: v, capped: false };
}

export function multistepMaxPayout(game: MultistepGame, params: Uint8Array, amount: bigint): bigint {
  if (game === 'mines') {
    const m = params[0];
    if (m < 1 || m > 24 || params.slice(1).some((b) => b !== 0)) throw new Error('invalid mines params');
    const safe = 25 - m;
    const { value } = minesValue(amount, m, safe);
    const cap = (amount * CAP_X100) / 100n;
    return value < cap ? value : cap;
  }
  if (game === 'hilo') {
    if (params.some((b) => b !== 0)) throw new Error('invalid hilo params');
    return (amount * CAP_X100) / 100n;
  }
  if (game === 'towers') {
    const diff = params[0];
    if (diff > 2 || params.slice(1).some((b) => b !== 0)) throw new Error('invalid towers params');
    const { value } = towersValue(amount, towersCols(diff), 8);
    const cap = (amount * CAP_X100) / 100n;
    return value < cap ? value : cap;
  }
  if (game === 'blackjack') {
    if (params.some((b) => b !== 0)) throw new Error('invalid blackjack params');
    return amount * 4n;
  }
  if (params.some((b) => b !== 0)) throw new Error('invalid poker params');
  return amount * 800n;
}

function bjDeck(d: number[]): number[] {
  const a = Array.from({ length: 52 }, (_, i) => i);
  for (let i = 51; i >= 1; i--) {
    const j = Number((BigInt(d[51 - i]) * BigInt(i + 1)) >> 32n);
    const tmp = a[i]; a[i] = a[j]; a[j] = tmp;
  }
  return a;
}

function bjCardVal(card: number): number {
  const rank = card % 13;
  if (rank === 0) return 11;
  if (rank >= 9) return 10;
  return rank + 1;
}

export function bjHandValue(cards: number[]): number {
  let sum = 0;
  let aces = 0;
  for (const c of cards) {
    const v = bjCardVal(c);
    sum += v;
    if (c % 13 === 0) aces++;
  }
  while (sum > 21 && aces > 0) {
    sum -= 10;
    aces--;
  }
  return sum;
}

function pokerEval(hand: number[]): number {
  const ranks = hand.map((c) => c % 13);
  const suits = hand.map((c) => Math.floor(c / 13));
  const flush = suits.every((s) => s === suits[0]);
  const sorted = [...ranks].sort((a, b) => a - b);
  let straight = false;
  let royal = false;
  if (sorted[0] === 0 && sorted[1] === 9 && sorted[2] === 10 && sorted[3] === 11 && sorted[4] === 12) {
    straight = true;
    royal = true;
  } else if (sorted[4] - sorted[0] === 4 && new Set(sorted).size === 5) {
    straight = true;
  }
  const counts = new Array(13).fill(0);
  for (const r of ranks) counts[r]++;
  const freqs = counts.filter((c) => c > 0).sort((a, b) => b - a);

  if (royal && flush) return 800;
  if (straight && flush) return 50;
  if (freqs[0] === 4) return 25;
  if (freqs[0] === 3 && freqs[1] === 2) return 9;
  if (flush) return 6;
  if (straight) return 4;
  if (freqs[0] === 3) return 3;
  if (freqs[0] === 2 && freqs[1] === 2) return 2;
  if (freqs[0] === 2) {
    for (let r = 0; r < 13; r++) {
      if (counts[r] === 2 && (r === 0 || r >= 10)) return 1;
    }
  }
  return 0;
}

export async function replay(
  game: MultistepGame,
  params: Uint8Array,
  amount: bigint,
  serverSeed: Uint8Array,
  clientSeed: Uint8Array,
  nonce: bigint,
  actions: Uint8Array,
  actionCount: number,
  extraStake = 0n,
  timeout = false,
): Promise<MultistepOutcome> {
  if (game === 'mines') {
    const m = params[0];
    if (m < 1 || m > 24 || params.slice(1).some((b) => b !== 0)) throw new Error('invalid mines params');
    const safeTotal = 25 - m;
    const d = await u32s(serverSeed, clientSeed, nonce, 25);
    const a = Array.from({ length: 25 }, (_, i) => i);
    for (let i = 0; i < m; i++) {
      const j = i + Number((BigInt(d[i]) * BigInt(25 - i)) >> 32n);
      const tmp = a[i]; a[i] = a[j]; a[j] = tmp;
    }
    const mines = new Set(a.slice(0, m));
    const revealed = new Set<number>();
    let safeCount = 0;
    let finished = false;
    let cashed = false;
    let payout = 0n;

    for (let idx = 0; idx < actionCount; idx++) {
      if (finished) break;
      const act = actions[idx];
      if (act === 255) {
        if (safeCount === 0) throw new Error('invalid action');
        payout = minesValue(amount, m, safeCount).value;
        finished = true;
        cashed = true;
      } else if (act <= 24) {
        if (revealed.has(act)) throw new Error('invalid action');
        revealed.add(act);
        if (mines.has(act)) {
          payout = 0n;
          finished = true;
        } else {
          safeCount++;
          const { value, capped } = minesValue(amount, m, safeCount);
          if (capped || safeCount === safeTotal) {
            payout = value;
            finished = true;
            cashed = true;
          }
        }
      } else {
        throw new Error('invalid action');
      }
    }

    if (!finished && timeout) {
      payout = safeCount === 0 ? amount : minesValue(amount, m, safeCount).value;
      finished = true;
      cashed = true;
    }
    return { payout, finished, cashed, detail: { mines: Array.from(mines), safeCount } };
  }

  if (game === 'hilo') {
    if (params.some((b) => b !== 0)) throw new Error('invalid hilo params');
    const d = await u32s(serverSeed, clientSeed, nonce, 40);
    const ranks = Array.from({ length: 20 }, (_, i) => Number((BigInt(d[i]) * 13n) >> 32n));
    const suits = Array.from({ length: 20 }, (_, i) => Number((BigInt(d[20 + i]) * 4n) >> 32n));
    const cap = (amount * CAP_X100) / 100n;
    let value = (amount * 99n) / 100n;
    let streak = 0;
    let finished = false;
    let cashed = false;
    let payout = 0n;

    for (let idx = 0; idx < actionCount; idx++) {
      if (finished) break;
      const act = actions[idx];
      if (act === 255) {
        if (streak === 0) throw new Error('invalid action');
        payout = value;
        finished = true;
        cashed = true;
      } else if (act === 0 || act === 1) {
        if (streak >= 19) throw new Error('invalid action');
        const cur = ranks[streak];
        const next = ranks[streak + 1];
        const won = act === 0 ? next >= cur : next <= cur;
        if (!won) {
          payout = 0n;
          finished = true;
        } else {
          const fav = BigInt(act === 0 ? 13 - cur : cur + 1);
          value = (value * 13n) / fav;
          if (value >= cap) {
            value = cap;
            payout = cap;
            finished = true;
            cashed = true;
          } else {
            streak++;
            if (streak === 19) {
              payout = value;
              finished = true;
              cashed = true;
            }
          }
        }
      } else {
        throw new Error('invalid action');
      }
    }

    if (!finished && timeout) {
      payout = streak === 0 ? amount : value;
      finished = true;
      cashed = true;
    }
    return { payout, finished, cashed, detail: { ranks, suits, streak } };
  }

  if (game === 'towers') {
    const diff = params[0];
    if (diff > 2 || params.slice(1).some((b) => b !== 0)) throw new Error('invalid towers params');
    const cols = towersCols(diff);
    const d = await u32s(serverSeed, clientSeed, nonce, 8);
    const bombs = Array.from({ length: 8 }, (_, f) => Number((BigInt(d[f]) * BigInt(cols)) >> 32n));
    let safeFloors = 0;
    let finished = false;
    let cashed = false;
    let payout = 0n;

    for (let idx = 0; idx < actionCount; idx++) {
      if (finished) break;
      const act = actions[idx];
      if (act === 255) {
        if (safeFloors === 0) throw new Error('invalid action');
        payout = towersValue(amount, cols, safeFloors).value;
        finished = true;
        cashed = true;
      } else if (act < cols) {
        if (safeFloors >= 8) throw new Error('invalid action');
        if (act === bombs[safeFloors]) {
          payout = 0n;
          finished = true;
        } else {
          safeFloors++;
          const { value, capped } = towersValue(amount, cols, safeFloors);
          if (capped || safeFloors === 8) {
            payout = value;
            finished = true;
            cashed = true;
          }
        }
      } else {
        throw new Error('invalid action');
      }
    }

    if (!finished && timeout) {
      payout = safeFloors === 0 ? amount : towersValue(amount, cols, safeFloors).value;
      finished = true;
      cashed = true;
    }
    return { payout, finished, cashed, detail: { bombs, safeFloors } };
  }

  if (game === 'blackjack') {
    if (params.some((b) => b !== 0)) throw new Error('invalid blackjack params');
    const d = await u32s(serverSeed, clientSeed, nonce, 52);
    const deck = bjDeck(d);
    const playerCards = [deck[0], deck[2]];
    const dealerCards = [deck[1], deck[3]];
    let ptr = 4;

    const pNat = bjHandValue(playerCards) === 21;
    const dNat = bjHandValue(dealerCards) === 21;
    if (pNat || dNat) {
      const payout = pNat && dNat ? amount : pNat ? (amount * 5n) / 2n : 0n;
      return { payout, finished: true, cashed: payout > 0n, detail: { playerCards, dealerCards, natural: true } };
    }

    const totalStake = amount + extraStake;
    let doubled = false;
    let stood = false;
    let finished = false;
    let payout = 0n;

    for (let idx = 0; idx < actionCount; idx++) {
      if (finished) break;
      const act = actions[idx];
      if (act === 0) {
        if (stood) break;
        playerCards.push(deck[ptr++]);
        const pv = bjHandValue(playerCards);
        if (pv > 21) {
          payout = 0n;
          finished = true;
        } else if (pv === 21) {
          stood = true;
        }
      } else if (act === 1) {
        stood = true;
      } else if (act === 2) {
        if (idx !== 0 || doubled) throw new Error('invalid action');
        doubled = true;
        stood = true;
        playerCards.push(deck[ptr++]);
        if (bjHandValue(playerCards) > 21) {
          payout = 0n;
          finished = true;
        }
      } else {
        throw new Error('invalid action');
      }
    }

    if (!finished && !stood && timeout) stood = true;
    if (!finished && stood) {
      while (bjHandValue(dealerCards) < 17) dealerCards.push(deck[ptr++]);
      const pv = bjHandValue(playerCards);
      const dv = bjHandValue(dealerCards);
      if (dv > 21 || pv > dv) payout = totalStake * 2n;
      else if (pv === dv) payout = totalStake;
      else payout = 0n;
      finished = true;
    }

    return { payout, finished, cashed: payout > 0n, detail: { playerCards, dealerCards } };
  }

  // poker
  if (params.some((b) => b !== 0)) throw new Error('invalid poker params');
  const d = await u32s(serverSeed, clientSeed, nonce, 52);
  const deck = bjDeck(d);
  const dealt = deck.slice(0, 5);
  let mask = 31;
  let finished = false;
  if (actionCount > 0) {
    mask = actions[0];
    if (mask > 31) throw new Error('invalid action');
    finished = true;
  } else if (timeout) {
    mask = 31;
    finished = true;
  }
  const finalHand = [...dealt];
  let ptr = 5;
  for (let i = 0; i < 5; i++) {
    if (((mask >> i) & 1) === 0) finalHand[i] = deck[ptr++];
  }
  const mult = pokerEval(finalHand);
  const payout = finished ? amount * BigInt(mult) : 0n;
  return { payout, finished, cashed: payout > 0n, detail: { dealt, finalHand, mult } };
}
