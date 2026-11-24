// Canonical provably fair scheme. Same bytes in Rust (programs/openhouse/src/games/fair.rs),
// in the settler and in the browser verifier.
//
//   mac(round) = HMAC_SHA256(key = serverSeed (32 bytes),
//                            msg = clientSeed (32 bytes) || nonce (u64 LE) || round (u32 LE))
//   float i of a round = big endian u32 at bytes [4i, 4i+4) of the mac, divided by 2^32
//   clientSeed = SHA256(text the player typed)

import { WHEEL, PLINKO, PLINKO_ROWS, KENO } from './tables';

const te = new TextEncoder();
const subtle = () => (globalThis.crypto as Crypto).subtle;

export async function sha256(data: Uint8Array): Promise<Uint8Array> {
  return new Uint8Array(await subtle().digest('SHA-256', data as BufferSource));
}
export async function hmacSha256(key: Uint8Array, msg: Uint8Array): Promise<Uint8Array> {
  const k = await subtle().importKey('raw', key as BufferSource, { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']);
  return new Uint8Array(await subtle().sign('HMAC', k, msg as BufferSource));
}
export const hex = (b: Uint8Array) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
export function fromHex(s: string): Uint8Array {
  if (!/^([0-9a-fA-F]{2})*$/.test(s)) throw new Error('bad hex');
  const o = new Uint8Array(s.length / 2);
  for (let i = 0; i < o.length; i++) o[i] = parseInt(s.slice(2 * i, 2 * i + 2), 16);
  return o;
}
export const clientSeedBytes = (text: string) => sha256(te.encode(text));

export async function mac(serverSeed: Uint8Array, clientSeed: Uint8Array, nonce: bigint, round: number): Promise<Uint8Array> {
  const msg = new Uint8Array(44);
  msg.set(clientSeed, 0);
  const dv = new DataView(msg.buffer);
  dv.setBigUint64(32, nonce, true);
  dv.setUint32(40, round, true);
  return hmacSha256(serverSeed, msg);
}

/** Raw u32 stream, 8 per round. */
export async function u32s(serverSeed: Uint8Array, clientSeed: Uint8Array, nonce: bigint, count: number): Promise<number[]> {
  const out: number[] = [];
  for (let round = 0; out.length < count; round++) {
    const m = await mac(serverSeed, clientSeed, nonce, round);
    const dv = new DataView(m.buffer, m.byteOffset, m.byteLength);
    for (let i = 0; i < 8 && out.length < count; i++) out.push(dv.getUint32(i * 4, false));
  }
  return out;
}
export const floats = async (s: Uint8Array, c: Uint8Array, n: bigint, count: number) =>
  (await u32s(s, c, n, count)).map((u) => u / 4294967296);

// ---------------- verified games (mirror programs/openhouse/src/games/mod.rs) ----------------
export type VerifiedGame =
  | 'dice' | 'flip' | 'limbo' | 'wheel' | 'plinko' | 'keno' | 'roulette'
  | 'hotreels' | 'slots777' | 'vaultbreak' | 'scratch' | 'sicbo' | 'dragon' | 'baccarat';
export const VERIFIED: readonly string[] = [
  'dice', 'flip', 'limbo', 'crash', 'wheel', 'plinko', 'keno', 'roulette',
  'hotreels', 'slots777', 'vaultbreak', 'scratch', 'sicbo', 'dragon', 'baccarat',
];
export const DICE_MIN_BP = 200, DICE_MAX_BP = 9800, LIMBO_MIN_X100 = 101, LIMBO_MAX_X100 = 100_000;
export const ROULETTE_UNIT = 1_000_000n;
const ROULETTE_RED = new Set([1, 3, 5, 7, 9, 12, 14, 16, 18, 19, 21, 23, 25, 27, 30, 32, 34, 36]);

export const gameKind = (id: string): VerifiedGame | null =>
  id === 'dice' ? 'dice'
    : id === 'flip' ? 'flip'
    : id === 'limbo' || id === 'crash' ? 'limbo'
    : id === 'wheel' ? 'wheel'
    : id === 'plinko' ? 'plinko'
    : id === 'keno' ? 'keno'
    : id === 'roulette' ? 'roulette'
    : id === 'hotreels' ? 'hotreels'
    : id === 'slots777' ? 'slots777'
    : id === 'vaultbreak' ? 'vaultbreak'
    : id === 'scratch' ? 'scratch'
    : id === 'sicbo' ? 'sicbo'
    : id === 'dragon' ? 'dragon'
    : id === 'baccarat' ? 'baccarat'
    : null;

export function gameIdBytes(id: string): Uint8Array {
  const b = te.encode(id);
  if (b.length === 0 || b.length > 16) throw new Error('bad game id');
  const o = new Uint8Array(16);
  o.set(b);
  return o;
}

export function encodeDice(rollUnder: boolean, targetBp: number): Uint8Array {
  if (!Number.isInteger(targetBp) || targetBp < DICE_MIN_BP || targetBp > DICE_MAX_BP) throw new Error('dice chance must be 2% to 98%');
  const p = new Uint8Array(32);
  p[0] = rollUnder ? 1 : 0;
  new DataView(p.buffer).setUint16(1, targetBp, true);
  return p;
}
export function encodeFlip(tails: boolean): Uint8Array {
  const p = new Uint8Array(32);
  p[0] = tails ? 1 : 0;
  return p;
}
export function encodeLimbo(targetX100: number): Uint8Array {
  if (!Number.isInteger(targetX100) || targetX100 < LIMBO_MIN_X100 || targetX100 > LIMBO_MAX_X100) throw new Error('target must be 1.01x to 1000x');
  const p = new Uint8Array(32);
  new DataView(p.buffer).setUint32(0, targetX100, true);
  return p;
}
export function encodeWheel(risk: number): Uint8Array {
  if (!Number.isInteger(risk) || risk < 0 || risk > 2) throw new Error('wheel risk must be 0, 1, or 2');
  const p = new Uint8Array(32);
  p[0] = risk;
  return p;
}
export function encodePlinko(rows: number, risk: number): Uint8Array {
  if (!Number.isInteger(rows) || !PLINKO_ROWS.includes(rows)) throw new Error('plinko rows must be 8, 10, 12, 14, or 16');
  if (!Number.isInteger(risk) || risk < 0 || risk > 2) throw new Error('plinko risk must be 0, 1, or 2');
  const p = new Uint8Array(32);
  p[0] = rows;
  p[1] = risk;
  return p;
}
export function encodeKeno(picks: number[]): Uint8Array {
  if (!Array.isArray(picks) || picks.length < 1 || picks.length > 10) throw new Error('keno requires 1 to 10 picks');
  let mask = 0n;
  for (const n of picks) {
    if (!Number.isInteger(n) || n < 1 || n > 40) throw new Error('keno picks must be 1 to 40');
    const bit = 1n << BigInt(n - 1);
    if ((mask & bit) !== 0n) throw new Error('duplicate keno pick');
    mask |= bit;
  }
  const p = new Uint8Array(32);
  new DataView(p.buffer).setBigUint64(0, mask, true);
  return p;
}
export interface RouletteEntry { slot: number; units: number }
export function encodeRoulette(entries: RouletteEntry[]): Uint8Array {
  if (!Array.isArray(entries) || entries.length < 1 || entries.length > 10) throw new Error('roulette allows 1 to 10 bets');
  const seen = new Set<number>();
  const p = new Uint8Array(32);
  const view = new DataView(p.buffer);
  for (let i = 0; i < entries.length; i++) {
    const { slot, units } = entries[i];
    if (!Number.isInteger(slot) || slot < 0 || slot > 48) throw new Error('invalid roulette slot');
    if (!Number.isInteger(units) || units < 1 || units > 65535) throw new Error('invalid roulette units');
    if (seen.has(slot)) throw new Error('duplicate roulette slot');
    seen.add(slot);
    p[3 * i] = slot;
    view.setUint16(3 * i + 1, units, true);
  }
  return p;
}

const dv = (p: Uint8Array) => new DataView(p.buffer, p.byteOffset, p.byteLength);

function popcount40(mask: bigint): number {
  let m = mask, c = 0;
  while (m > 0n) { c += Number(m & 1n); m >>= 1n; }
  return c;
}

export function rouletteMult(slot: number, n: number): bigint {
  if (slot <= 36) return slot === n ? 36n : 0n;
  if (n === 0) return 0n;
  const red = ROULETTE_RED.has(n);
  let hit = false;
  switch (slot) {
    case 37: hit = red; break;
    case 38: hit = !red; break;
    case 39: hit = n % 2 === 1; break;
    case 40: hit = n % 2 === 0; break;
    case 41: hit = n <= 18; break;
    case 42: hit = n >= 19; break;
    case 43: hit = n <= 12; break;
    case 44: hit = n > 12 && n <= 24; break;
    case 45: hit = n > 24; break;
    case 46: hit = n % 3 === 1; break;
    case 47: hit = n % 3 === 2; break;
    case 48: hit = n % 3 === 0; break;
  }
  if (!hit) return 0n;
  return slot <= 42 ? 2n : 3n;
}

function parseRouletteEntries(params: Uint8Array, amount: bigint): RouletteEntry[] {
  const view = dv(params);
  const out: RouletteEntry[] = [];
  const seen = new Set<number>();
  let totalUnits = 0n;
  for (let i = 0; i < 10; i++) {
    const slot = params[3 * i];
    const units = view.getUint16(3 * i + 1, true);
    if (units === 0) break;
    if (slot > 48 || seen.has(slot)) throw new Error('invalid roulette params');
    seen.add(slot);
    out.push({ slot, units });
    totalUnits += BigInt(units);
  }
  if (out.length === 0 || totalUnits * ROULETTE_UNIT !== amount) throw new Error('roulette stake must equal units * 0.001 SOL');
  return out;
}

function kenoMask(params: Uint8Array): { mask: bigint; count: number } {
  const mask = dv(params).getBigUint64(0, true);
  const count = popcount40(mask);
  if ((mask >> 40n) !== 0n || count < 1 || count > 10) throw new Error('invalid keno picks');
  return { mask, count };
}

function plinkoTable(params: Uint8Array): number[] {
  const rows = params[0], risk = params[1];
  if (!PLINKO_ROWS.includes(rows) || risk > 2) throw new Error('invalid plinko params');
  return PLINKO[rows][risk];
}

/** Maximum total payout (stake included) used for place_bet reservation. */
export function winPayout(kind: VerifiedGame, params: Uint8Array, amount: bigint): bigint {
  if (kind === 'dice') return (amount * 9900n) / BigInt(dv(params).getUint16(1, true));
  if (kind === 'flip') return (amount * 198n) / 100n;
  if (kind === 'limbo') return (amount * BigInt(dv(params).getUint32(0, true))) / 100n;
  if (kind === 'wheel') {
    const risk = params[0];
    if (risk > 2) throw new Error('invalid wheel risk');
    return (amount * BigInt(Math.max(...WHEEL[risk]))) / 100n;
  }
  if (kind === 'plinko') return (amount * BigInt(Math.max(...plinkoTable(params)))) / 100n;
  if (kind === 'keno') {
    const { count } = kenoMask(params);
    return (amount * BigInt(Math.max(...KENO[count - 1]))) / 100n;
  }
  if (kind === 'hotreels') return (amount * 27500n) / 100n;
  if (kind === 'slots777') return (amount * 33000n) / 100n;
  if (kind === 'vaultbreak') return (amount * 13000n) / 100n;
  if (kind === 'scratch') return (amount * 20000n) / 100n;
  if (kind === 'sicbo') {
    if (params[0] > 7) throw new Error('invalid sicbo bet');
    return (amount * (params[0] <= 1 ? 203n : 1100n)) / 100n;
  }
  if (kind === 'dragon') {
    if (params[0] > 2) throw new Error('invalid dragon bet');
    return (amount * (params[0] === 2 ? 1280n : 214n)) / 100n;
  }
  if (kind === 'baccarat') {
    if (params[0] > 2) throw new Error('invalid baccarat bet');
    const m = params[0] === 0 ? 200n : params[0] === 1 ? 195n : 900n;
    return (amount * m) / 100n;
  }
  const entries = parseRouletteEntries(params, amount);
  let best = 0n;
  for (let n = 0; n < 37; n++) {
    let sum = 0n;
    for (const e of entries) sum += BigInt(e.units) * ROULETTE_UNIT * rouletteMult(e.slot, n);
    if (sum > best) best = sum;
  }
  return best;
}

export interface Outcome { win: boolean; detail: Record<string, number | string | boolean> }

/** The exact decision the program makes from the first u32 of the stream (single-draw games). */
export function resolve(kind: VerifiedGame, params: Uint8Array, u: number): Outcome {
  const U = BigInt(u);
  if (kind === 'dice') {
    const under = params[0] === 1;
    const t = BigInt(dv(params).getUint16(1, true));
    const roll = (U * 10000n) >> 32n;
    const win = under ? roll < t : roll >= 10000n - t;
    return { win, detail: { roll: Number(roll) / 100 } };
  }
  if (kind === 'flip') {
    const tails = u >= 0x80000000;
    return { win: tails === (params[0] === 1), detail: { result: tails ? 'tails' : 'heads' } };
  }
  if (kind === 'wheel') {
    const risk = params[0];
    if (risk > 2) throw new Error('invalid wheel risk');
    const segment = Number((U * 24n) >> 32n);
    const mult = WHEEL[risk][segment];
    return { win: mult > 0, detail: { segment, multiplier: mult / 100 } };
  }
  if (kind === 'roulette') {
    const number = Number((U * 37n) >> 32n);
    return { win: true, detail: { number } };
  }
  const t = BigInt(dv(params).getUint32(0, true));
  const crash = (99n << 32n) / ((1n << 32n) - U);
  return { win: crash >= t, detail: { crashPoint: Number(crash > 100000000n ? 100000000n : crash) / 100 } };
}

export interface DrawOutcome { payout: bigint; detail: Record<string, any> }

/** Exact settlement payout and animation detail from the canonical draw list. */
export function resolveFromDraws(kind: VerifiedGame, params: Uint8Array, amount: bigint, draws: number[]): DrawOutcome {
  if (kind === 'dice' || kind === 'flip' || kind === 'limbo') {
    const o = resolve(kind, params, draws[0]);
    return { payout: o.win ? winPayout(kind, params, amount) : 0n, detail: o.detail };
  }
  if (kind === 'wheel') {
    const risk = params[0];
    if (risk > 2) throw new Error('invalid wheel risk');
    const segment = Number((BigInt(draws[0]) * 24n) >> 32n);
    const multX100 = WHEEL[risk][segment];
    return { payout: (amount * BigInt(multX100)) / 100n, detail: { segment, multiplier: multX100 / 100 } };
  }
  if (kind === 'plinko') {
    const rows = params[0];
    const table = plinkoTable(params);
    const path: (0 | 1)[] = [];
    let slot = 0;
    for (let i = 0; i < rows; i++) {
      const bit: 0 | 1 = draws[i] >= 0x80000000 ? 1 : 0;
      path.push(bit);
      slot += bit;
    }
    const multX100 = table[slot];
    return { payout: (amount * BigInt(multX100)) / 100n, detail: { path, slot, multiplier: multX100 / 100 } };
  }
  if (kind === 'keno') {
    const { mask, count } = kenoMask(params);
    const a = Array.from({ length: 40 }, (_, i) => i + 1);
    const drawn: number[] = [];
    let drawnMask = 0n;
    for (let i = 0; i < 10; i++) {
      const j = i + Number((BigInt(draws[i]) * BigInt(40 - i)) >> 32n);
      const tmp = a[i]; a[i] = a[j]; a[j] = tmp;
      drawn.push(a[i]);
      drawnMask |= 1n << BigInt(a[i] - 1);
    }
    const hits = popcount40(mask & drawnMask);
    const multX100 = KENO[count - 1][hits];
    return { payout: (amount * BigInt(multX100)) / 100n, detail: { drawn, hits, multiplier: multX100 / 100 } };
  }
  if (kind === 'hotreels' || kind === 'slots777' || kind === 'vaultbreak') {
    const stops = [0, 1, 2].map((i) => Number((BigInt(draws[i]) * 24n) >> 32n));
    const multX100 =
      kind === 'hotreels' ? hotreelsX100(stops) :
      kind === 'slots777' ? slots777X100(stops) :
      vaultbreakX100(stops);
    return { payout: (amount * BigInt(multX100)) / 100n, detail: { stops, multiplier: multX100 / 100 } };
  }
  if (kind === 'scratch') {
    let r = Number((BigInt(draws[0]) * 10_000_000n) >> 32n);
    let tier = -1;
    for (let i = 0; i < 6; i++) {
      if (r < SCRATCH_PROBS[i]) { tier = i; break; }
      r -= SCRATCH_PROBS[i];
    }
    const multX100 = tier >= 0 ? SCRATCH_X100[tier] : 0;
    const pos = [0, 1, 2, 3, 4, 5, 6, 7, 8];
    for (let i = 8; i > 0; i--) {
      const j = Number((BigInt(draws[9 - i]) * BigInt(i + 1)) >> 32n);
      [pos[i], pos[j]] = [pos[j], pos[i]];
    }
    const cells = new Array(9).fill(-1);
    const cnt = [0, 0, 0, 0, 0, 0];
    if (tier >= 0) {
      for (let i = 0; i < 3; i++) cells[pos[i]] = tier;
      cnt[tier] = 3;
    }
    const start = tier >= 0 ? 3 : 0;
    for (let idx = start; idx < 9; idx++) {
      let s = Number((BigInt(draws[1 + ((idx - start) % 8)]) * 6n) >> 32n);
      let g = 0;
      while (cnt[s] >= 2 && g++ < 6) s = (s + 1) % 6;
      cells[pos[idx]] = s;
      cnt[s]++;
    }
    return { payout: (amount * BigInt(multX100)) / 100n, detail: { tier, cells, multiplier: multX100 / 100 } };
  }
  if (kind === 'sicbo') {
    if (params[0] > 7) throw new Error('invalid sicbo bet');
    const dice = [0, 1, 2].map((i) => Number((BigInt(draws[i]) * 6n) >> 32n) + 1);
    const sum = dice[0] + dice[1] + dice[2];
    const triple = dice[0] === dice[1] && dice[1] === dice[2];
    let multX100 = 0;
    if (params[0] === 0) multX100 = !triple && sum <= 10 ? 203 : 0;
    else if (params[0] === 1) multX100 = !triple && sum >= 11 ? 203 : 0;
    else {
      const n = params[0] - 1;
      const k = dice.filter((d) => d === n).length;
      multX100 = k === 1 ? 200 : k === 2 ? 350 : k === 3 ? 1100 : 0;
    }
    return { payout: (amount * BigInt(multX100)) / 100n, detail: { dice, multiplier: multX100 / 100 } };
  }
  if (kind === 'dragon') {
    if (params[0] > 2) throw new Error('invalid dragon bet');
    const d = { r: Number((BigInt(draws[0]) * 13n) >> 32n), s: Number((BigInt(draws[1]) * 4n) >> 32n) };
    const t = { r: Number((BigInt(draws[2]) * 13n) >> 32n), s: Number((BigInt(draws[3]) * 4n) >> 32n) };
    let multX100 = 0;
    if (d.r > t.r) multX100 = params[0] === 0 ? 214 : 0;
    else if (t.r > d.r) multX100 = params[0] === 1 ? 214 : 0;
    else multX100 = params[0] === 2 ? 1280 : 0;
    return { payout: (amount * BigInt(multX100)) / 100n, detail: { d, t, multiplier: multX100 / 100 } };
  }
  if (kind === 'baccarat') {
    if (params[0] > 2) throw new Error('invalid baccarat bet');
    const cards = Array.from({ length: 6 }, (_, i) => ({
      r: Number((BigInt(draws[2 * i]) * 13n) >> 32n),
      s: Number((BigInt(draws[2 * i + 1]) * 4n) >> 32n),
    }));
    const bcv = (c: { r: number }) => (c.r >= 9 ? 0 : c.r + 1);
    const bcTot = (h: { r: number }[]) => h.reduce((s, c) => s + bcv(c), 0) % 10;
    const P = [cards[0], cards[2]];
    const B = [cards[1], cards[3]];
    let nx = 4;
    const pt0 = bcTot(P), bt0 = bcTot(B);
    if (!(pt0 >= 8 || bt0 >= 8)) {
      let p3: { r: number; s: number } | null = null;
      if (pt0 <= 5) { p3 = cards[nx++]; P.push(p3); }
      const bt2 = bcTot(B);
      let drawB = false;
      if (!p3) drawB = bt2 <= 5;
      else {
        const v = bcv(p3);
        drawB = bt2 <= 2 || (bt2 === 3 && v !== 8) || (bt2 === 4 && v >= 2 && v <= 7) || (bt2 === 5 && v >= 4 && v <= 7) || (bt2 === 6 && v >= 6 && v <= 7);
      }
      if (drawB) B.push(cards[nx++]);
    }
    const p = bcTot(P), b = bcTot(B);
    const w = p > b ? 'P' : b > p ? 'B' : 'T';
    let multX100 = 0;
    if (w === 'P') multX100 = params[0] === 0 ? 200 : 0;
    else if (w === 'B') multX100 = params[0] === 1 ? 195 : 0;
    else multX100 = params[0] === 2 ? 900 : 100;
    return { payout: (amount * BigInt(multX100)) / 100n, detail: { P, B, p, b, w, multiplier: multX100 / 100 } };
  }
  const entries = parseRouletteEntries(params, amount);
  const number = Number((BigInt(draws[0]) * 37n) >> 32n);
  let payout = 0n;
  for (const e of entries) payout += BigInt(e.units) * ROULETTE_UNIT * rouletteMult(e.slot, number);
  const multiplier = amount > 0n ? Number((payout * 10000n) / amount) / 10000 : 0;
  return { payout, detail: { number, multiplier } };
}

export function encodeSlots(): Uint8Array { return new Uint8Array(32); }
export function encodeScratch(): Uint8Array { return new Uint8Array(32); }
export function encodeSicBo(bet: 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7): Uint8Array {
  if (!Number.isInteger(bet) || bet < 0 || bet > 7) throw new Error('invalid sicbo bet');
  const p = new Uint8Array(32);
  p[0] = bet;
  return p;
}
export function encodeDragon(bet: 0 | 1 | 2): Uint8Array {
  if (!Number.isInteger(bet) || bet < 0 || bet > 2) throw new Error('invalid dragon bet');
  const p = new Uint8Array(32);
  p[0] = bet;
  return p;
}
export function encodeBaccarat(bet: 0 | 1 | 2): Uint8Array {
  if (!Number.isInteger(bet) || bet < 0 || bet > 2) throw new Error('invalid baccarat bet');
  const p = new Uint8Array(32);
  p[0] = bet;
  return p;
}

const HOTREELS_STRIPS = [
  [0,1,2,3,4,0,1,2,5,0,1,2,3,4,0,1,2,0,5,3,4,0,1,2],
  [0,1,2,3,4,1,0,2,5,1,0,2,3,4,1,0,2,1,5,3,0,4,1,2],
  [0,1,2,3,4,0,1,5,2,0,3,1,4,0,2,3,1,0,5,4,2,0,1,3],
];
const HOTREELS_P3 = [1100, 900, 1800, 3500, 7000, 27500];
const SLOTS777_STRIPS = [
  [0,1,2,0,3,0,1,4,2,0,3,1,2,0,0,3,1,2,0,4,1,0,3,2],
  [0,1,2,3,0,1,4,2,0,1,3,0,2,1,0,3,2,1,0,4,0,1,3,2],
  [0,1,2,0,3,0,1,4,2,0,3,1,2,0,0,3,1,2,0,4,1,0,3,2],
];
const SLOTS777_P3 = [500, 1000, 1800, 4500, 33000];
const VAULTBREAK_STRIPS = [
  [0,1,2,0,3,1,4,2,0,1,0,2,3,5,1,0,2,1,0,4,3,0,1,2],
  [0,1,2,1,4,0,3,2,1,0,1,2,4,0,5,1,2,0,3,1,4,0,2,1],
  [0,1,2,0,3,2,1,4,0,2,0,1,3,5,2,0,1,2,0,4,3,1,0,2],
];
const VAULTBREAK_P3_LINE = [200, 300, 540, 1080, 2560, 13000];
const SCRATCH_PROBS = [1_200_000, 1_100_000, 400_000, 150_000, 40_000, 5_000];
const SCRATCH_X100 = [100, 200, 500, 1000, 5000, 20000];

function hotreelsX100(stops: number[]): number {
  const s0 = HOTREELS_STRIPS[0][stops[0]], s1 = HOTREELS_STRIPS[1][stops[1]], s2 = HOTREELS_STRIPS[2][stops[2]];
  if (s0 === s1 && s1 === s2) return HOTREELS_P3[s0];
  if (s0 === 0 && s1 === 0) return 300;
  if (s0 === 0) return 50;
  return 0;
}
function slots777X100(stops: number[]): number {
  const s0 = SLOTS777_STRIPS[0][stops[0]], s1 = SLOTS777_STRIPS[1][stops[1]], s2 = SLOTS777_STRIPS[2][stops[2]];
  if (s0 === s1 && s1 === s2) return SLOTS777_P3[s0];
  if (s0 === 0 && s1 === 0) return 100;
  if (s0 === 0) return 30;
  return 0;
}
function vaultbreakX100(stops: number[]): number {
  const LINES = [[0, 0, 0], [1, 1, 1], [2, 2, 2], [0, 1, 2], [2, 1, 0]];
  let total = 0;
  for (const line of LINES) {
    const s0 = VAULTBREAK_STRIPS[0][(stops[0] + line[0] + 23) % 24];
    const s1 = VAULTBREAK_STRIPS[1][(stops[1] + line[1] + 23) % 24];
    const s2 = VAULTBREAK_STRIPS[2][(stops[2] + line[2] + 23) % 24];
    if (s0 === s1 && s1 === s2) total += VAULTBREAK_P3_LINE[s0];
  }
  return total;
}
