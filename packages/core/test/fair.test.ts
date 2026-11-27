import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  mac, u32s, hex, fromHex, clientSeedBytes, encodeDice, encodeFlip, encodeLimbo,
  encodeWheel, encodePlinko, encodeKeno, encodeRoulette, encodeSicBo, encodeDragon, encodeBaccarat,
  winPayout, resolve, resolveFromDraws, gameKind,
  type VerifiedGame,
} from '../src/fair.ts';
import { ERROR_NAMES, ERROR_MESSAGES, explainError } from '../src/errors.ts';

const V = JSON.parse(readFileSync(new URL('./vectors.json', import.meta.url), 'utf8'));
const server = fromHex(V.serverSeed), client = fromHex(V.clientSeed);

test('client seed text hashes like the vectors', async () => {
  assert.equal(hex(await clientSeedBytes('openhouse-test')), V.clientSeed);
});

test('HMAC stream matches the Node reference, which the Rust program also matches', async () => {
  for (const v of V.vectors) {
    assert.equal(hex(await mac(server, client, BigInt(v.nonce), 0)), v.mac0, `nonce ${v.nonce} round 0`);
    assert.equal(hex(await mac(server, client, BigInt(v.nonce), 1)), v.mac1, `nonce ${v.nonce} round 1`);
    assert.deepEqual(await u32s(server, client, BigInt(v.nonce), 4), v.u32);
  }
});

test('dice: payout is 99 percent over target and the win window has the right size', () => {
  const p = encodeDice(true, 5000);
  assert.equal(winPayout('dice', p, 1_000_000n), 1_980_000n);
  let wins = 0;
  for (let i = 0; i < 10000; i++) if (resolve('dice', p, Math.floor((i / 10000) * 2 ** 32)).win) wins++;
  assert.equal(wins, 5000);
  const over = encodeDice(false, 2500);
  let w2 = 0;
  for (let i = 0; i < 10000; i++) if (resolve('dice', over, Math.floor((i / 10000) * 2 ** 32)).win) w2++;
  assert.equal(w2, 2500);
});

test('flip: pays 1.98x and splits the range in half', () => {
  assert.equal(winPayout('flip', encodeFlip(false), 100n), 198n);
  assert.equal(resolve('flip', encodeFlip(false), 0).win, true);
  assert.equal(resolve('flip', encodeFlip(true), 0xffffffff).win, true);
  assert.equal(resolve('flip', encodeFlip(true), 0).win, false);
});

test('limbo: win chance is about 99 over target', () => {
  for (const t of [150, 200, 1000]) {
    const p = encodeLimbo(t);
    let wins = 0; const N = 200000;
    for (let i = 0; i < N; i++) if (resolve('limbo', p, Math.floor((i / N) * 2 ** 32)).win) wins++;
    const expected = (99 / t) * N;
    assert.ok(Math.abs(wins - expected) / expected < 0.02, `target ${t}: ${wins} vs ${expected}`);
  }
  assert.equal(winPayout('limbo', encodeLimbo(250), 1000n), 2500n);
});

test('parameter encoders reject out of range input', () => {
  assert.throws(() => encodeDice(true, 100));
  assert.throws(() => encodeDice(true, 9900));
  assert.throws(() => encodeLimbo(100));
  assert.throws(() => encodeLimbo(100001));
});

test('error table is in the same order as the Rust enum', () => {
  const src = readFileSync(new URL('../../../programs/openhouse/src/errors.rs', import.meta.url), 'utf8');
  const names = [...src.matchAll(/#\[msg\("([^"]*)"\)\]\s*(\w+),/g)];
  assert.deepEqual(names.map((m) => m[2]), [...ERROR_NAMES]);
  // The friendly sentences in errors.ts are written for players and differ from the Rust messages on purpose. Count must still match.
  assert.equal(ERROR_MESSAGES.length, names.length);
  assert.ok(ERROR_MESSAGES.every((m) => m.length > 3));
});

test('explainError maps program codes to plain sentences', () => {
  assert.equal(explainError({ message: 'custom program error: 0x1770' }), ERROR_MESSAGES[0]);
  assert.equal(explainError({ message: 'User rejected the request' }), 'You cancelled the request in your wallet');
});

test('parity: 200 Rust vectors per game (wheel, plinko, keno, roulette) match TS maxPayout and resolveFromDraws', async () => {
  const rv = JSON.parse(readFileSync(new URL('./rust-vectors.json', import.meta.url), 'utf8'));
  assert.equal(rv.length, 800);
  const drawCount: Record<string, number> = { wheel: 1, plinko: 16, keno: 10, roulette: 1 };
  for (let i = 0; i < rv.length; i++) {
    const v = rv[i];
    const kind = v.game as VerifiedGame;
    const params = fromHex(v.params);
    const amount = BigInt(v.amount);
    const s = fromHex(v.server);
    const c = fromHex(v.client);
    const nonce = BigInt(v.nonce);
    assert.equal(winPayout(kind, params, amount).toString(), v.maxPayout, `maxPayout mismatch at ${i} (${kind})`);
    const draws = await u32s(s, c, nonce, kind === 'plinko' ? params[0] : drawCount[kind]);
    const out = resolveFromDraws(kind, params, amount, draws);
    assert.equal(out.payout.toString(), v.settlePayout, `settlePayout mismatch at ${i} (${kind})`);
  }
});

test('uniformity: 20,000 nonces per game stay within 5 standard deviations', async () => {
  const N = 20_000;
  const wheelBins = new Array(24).fill(0);
  const rouBins = new Array(37).fill(0);
  const plinkoRight = [0, 0];
  const kenoCounts = new Array(40).fill(0);
  const kenoParams = encodeKeno([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
  const plinkoParams = encodePlinko(8, 0);
  const wheelParams = encodeWheel(0);
  const rouParams = encodeRoulette([{ slot: 0, units: 1 }]);
  for (let nonce = 0; nonce < N; nonce++) {
    const draws = await u32s(server, client, BigInt(nonce), 10);
    const w = resolveFromDraws('wheel', wheelParams, 1_000_000n, [draws[0]]);
    wheelBins[w.detail.segment]++;
    const r = resolveFromDraws('roulette', rouParams, 1_000_000n, [draws[0]]);
    rouBins[r.detail.number]++;
    const p = resolveFromDraws('plinko', plinkoParams, 1_000_000n, draws.slice(0, 8));
    for (const bit of p.detail.path) plinkoRight[bit]++;
    const k = resolveFromDraws('keno', kenoParams, 1_000_000n, draws);
    for (const d of k.detail.drawn) kenoCounts[d - 1]++;
  }
  const check5Sigma = (bins: number[], p: number, label: string, trials = N) => {
    const mean = trials * p;
    const sd = Math.sqrt(trials * p * (1 - p));
    for (let i = 0; i < bins.length; i++) {
      assert.ok(Math.abs(bins[i] - mean) <= 5 * sd, `${label} bin ${i}: ${bins[i]} vs ${mean.toFixed(1)} +/- ${(5 * sd).toFixed(1)}`);
    }
  };
  check5Sigma(wheelBins, 1 / 24, 'wheel');
  check5Sigma(rouBins, 1 / 37, 'roulette');
  check5Sigma(plinkoRight, 0.5, 'plinko bit', N * 8);
  check5Sigma(kenoCounts, 10 / 40, 'keno number');
});

test('batch 1 encoders reject invalid input', () => {
  assert.throws(() => encodeWheel(3));
  assert.throws(() => encodeWheel(-1));
  assert.throws(() => encodePlinko(9, 0));
  assert.throws(() => encodePlinko(8, 3));
  assert.throws(() => encodeKeno([]));
  assert.throws(() => encodeKeno([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]));
  assert.throws(() => encodeKeno([1, 1]));
  assert.throws(() => encodeKeno([41]));
  assert.throws(() => encodeRoulette([]));
  assert.throws(() => encodeRoulette([{ slot: 37, units: 1 }, { slot: 37, units: 2 }]));
  assert.throws(() => encodeRoulette([{ slot: 49, units: 1 }]));
  assert.throws(() => encodeRoulette(Array.from({ length: 11 }, (_, i) => ({ slot: i, units: 1 }))));
});

test('batch 2 rust parity vectors: 200 cases per game', async () => {
  const vectors = JSON.parse(readFileSync(new URL('./rust-vectors-b2.json', import.meta.url), 'utf8'));
  const drawCounts: Record<string, number> = {
    hotreels: 3, slots777: 3, vaultbreak: 3, scratch: 10, sicbo: 3, dragon: 4, baccarat: 12,
  };
  for (const v of vectors) {
    const k = gameKind(v.game)!;
    assert.ok(k, `missing gameKind for ${v.game}`);
    const p = fromHex(v.params);
    const s = fromHex(v.server);
    const c = fromHex(v.client);
    const amt = BigInt(v.amount);
    assert.equal(winPayout(k, p, amt).toString(), String(v.maxPayout), `${v.game} maxPayout`);
    const draws = await u32s(s, c, BigInt(v.nonce), drawCounts[v.game]);
    const out = resolveFromDraws(k, p, amt, draws);
    assert.equal(out.payout.toString(), String(v.payout), `${v.game} nonce ${v.nonce}`);
  }
});

test('batch 2 uniformity and scratch card layout (20,000 nonces)', async () => {
  const s = new Uint8Array(32).fill(0x5a);
  const c = new Uint8Array(32).fill(0x3c);
  const N = 20_000;
  const slotBins = new Array(24).fill(0);
  const diceBins = new Array(6).fill(0);
  const tierBins = new Array(7).fill(0);
  const rankBins = new Array(13).fill(0);
  const p0 = new Uint8Array(32);

  for (let i = 0; i < N; i++) {
    const d = await u32s(s, c, BigInt(i), 10);
    slotBins[Number((BigInt(d[0]) * 24n) >> 32n)]++;
    diceBins[Number((BigInt(d[1]) * 6n) >> 32n)]++;
    rankBins[Number((BigInt(d[2]) * 13n) >> 32n)]++;

    const sc = resolveFromDraws('scratch', p0, 1_000_000n, d);
    const tier = sc.detail.tier as number;
    const cells = sc.detail.cells as number[];
    tierBins[tier >= 0 ? tier : 6]++;
    assert.equal(cells.length, 9);
    const counts = new Array(6).fill(0);
    for (const sym of cells) {
      assert.ok(sym >= 0 && sym < 6);
      counts[sym]++;
    }
    if (tier >= 0) {
      assert.equal(counts[tier], 3, `winning card tier ${tier} must appear 3 times`);
      for (let k = 0; k < 6; k++) {
        if (k !== tier) assert.ok(counts[k] < 3, `non-winning symbol ${k} appeared >=3 times`);
      }
    } else {
      for (let k = 0; k < 6; k++) assert.ok(counts[k] < 3, `losing card symbol ${k} appeared >=3 times`);
    }
  }
  const check5Sigma = (bins: number[], p: number, name: string) => {
    const exp = N * p;
    const sd = Math.sqrt(N * p * (1 - p));
    for (let i = 0; i < bins.length; i++) {
      assert.ok(Math.abs(bins[i] - exp) <= 5 * sd, `${name} bin ${i}: ${bins[i]} vs ${exp}`);
    }
  };
  check5Sigma(slotBins, 1 / 24, 'slot stop');
  check5Sigma(diceBins, 1 / 6, 'sicbo die');
  check5Sigma(rankBins, 1 / 13, 'card rank');
  const probs = [0.12, 0.11, 0.04, 0.015, 0.004, 0.0005, 0.7105];
  for (let i = 0; i < probs.length; i++) {
    const exp = N * probs[i];
    const sd = Math.sqrt(N * probs[i] * (1 - probs[i]));
    assert.ok(Math.abs(tierBins[i] - exp) <= 5 * sd, `scratch tier ${i}: ${tierBins[i]} vs ${exp}`);
  }
});

test('batch 2 encoders reject invalid input', () => {
  assert.throws(() => encodeSicBo(8 as any));
  assert.throws(() => encodeSicBo(-1 as any));
  assert.throws(() => encodeDragon(3 as any));
  assert.throws(() => encodeBaccarat(3 as any));
});
