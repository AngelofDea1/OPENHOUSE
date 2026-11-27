import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import {
  replay, encodeMines, encodeTowers, encodeHiLo, encodeBlackjack, encodePoker,
} from '../../../packages/core/src/index';
import { buildStepView, fetchSettledEvent } from '../src/index';

const sha256 = (b: Uint8Array) => new Uint8Array(createHash('sha256').update(b).digest());

test('mines: no leak of unrevealed cells and finished on bomb', async () => {
  const seed = sha256(Buffer.from('mines-step-seed-1'));
  const clientSeed = sha256(Buffer.from('client-1'));
  const params = encodeMines(3);
  const actions = new Uint8Array(32);

  // 0 actions: nothing revealed
  const r0 = await replay('mines', params, 1_000_000_000n, seed, clientSeed, 1n, actions, 0, 0n, false);
  const v0 = buildStepView('mines', params, 1_000_000_000n, actions, 0, r0);
  assert.equal(r0.finished, false);
  assert.deepEqual(v0.revealed, []);
  assert.equal(v0.lastAction, null);
  assert.equal('mines' in v0, false);

  // 1 action: pick a safe cell or bomb
  const minesSet = new Set<number>(r0.detail.mines);
  const safeCell = Array.from({ length: 25 }, (_, i) => i).find((i) => !minesSet.has(i))!;
  const bombCell = r0.detail.mines[0];
  actions[0] = safeCell;
  const r1 = await replay('mines', params, 1_000_000_000n, seed, clientSeed, 1n, actions, 1, 0n, false);
  const v1 = buildStepView('mines', params, 1_000_000_000n, actions, 1, r1);
  assert.equal(r1.finished, false);
  assert.equal(v1.revealed.length, 1);
  assert.equal(v1.lastAction.cell, safeCell);
  assert.equal(v1.lastAction.safe, true);
  assert.ok(v1.multiplier > 1);

  // 2nd action: hit bomb -> finished
  actions[1] = bombCell;
  const r2 = await replay('mines', params, 1_000_000_000n, seed, clientSeed, 1n, actions, 2, 0n, false);
  const v2 = buildStepView('mines', params, 1_000_000_000n, actions, 2, r2);
  assert.equal(r2.finished, true);
  assert.equal(v2.lastAction.bomb, true);
});

test('blackjack: hides dealer hole card until finished, natural at 0 actions finishes immediately', async () => {
  const clientSeed = sha256(Buffer.from('bj-client'));
  const params = encodeBlackjack();
  const actions = new Uint8Array(32);

  // Find a non-natural seed and a natural seed
  let nonNatSeed: Uint8Array | null = null;
  let natSeed: Uint8Array | null = null;
  for (let i = 0; i < 200; i++) {
    const s = sha256(Buffer.from(`bj-seed-${i}`));
    const r = await replay('blackjack', params, 1_000_000_000n, s, clientSeed, 1n, actions, 0, 0n, false);
    if (r.finished && r.detail.natural && !natSeed) natSeed = s;
    if (!r.finished && !nonNatSeed) nonNatSeed = s;
    if (nonNatSeed && natSeed) break;
  }
  assert.ok(nonNatSeed, 'found non-natural seed');
  assert.ok(natSeed, 'found natural seed');

  const rUnfin = await replay('blackjack', params, 1_000_000_000n, nonNatSeed!, clientSeed, 1n, actions, 0, 0n, false);
  const vUnfin = buildStepView('blackjack', params, 1_000_000_000n, actions, 0, rUnfin);
  assert.equal(rUnfin.finished, false);
  assert.equal(vUnfin.playerCards.length, 2);
  assert.equal(vUnfin.dealerCards.length, 1); // hole card hidden!

  const rNat = await replay('blackjack', params, 1_000_000_000n, natSeed!, clientSeed, 1n, actions, 0, 0n, false);
  const vNat = buildStepView('blackjack', params, 1_000_000_000n, actions, 0, rNat);
  assert.equal(rNat.finished, true);
  assert.equal(vNat.natural, true);
  assert.ok(vNat.dealerCards.length >= 2);
});

test('poker: dealt 5 cards at 0 actions, finalHand only after draw', async () => {
  const seed = sha256(Buffer.from('poker-seed-1'));
  const clientSeed = sha256(Buffer.from('poker-client-1'));
  const params = encodePoker();
  const actions = new Uint8Array(32);

  const r0 = await replay('poker', params, 1_000_000_000n, seed, clientSeed, 1n, actions, 0, 0n, false);
  const v0 = buildStepView('poker', params, 1_000_000_000n, actions, 0, r0);
  assert.equal(r0.finished, false);
  assert.equal(v0.dealt.length, 5);
  assert.equal(v0.finalHand, null);

  actions[0] = 0b11111; // hold all 5
  const r1 = await replay('poker', params, 1_000_000_000n, seed, clientSeed, 1n, actions, 1, 0n, false);
  const v1 = buildStepView('poker', params, 1_000_000_000n, actions, 1, r1);
  assert.equal(r1.finished, true);
  assert.equal(v1.finalHand.length, 5);
});

test('hilo & towers: step views match core replay', async () => {
  const seed = sha256(Buffer.from('ht-seed-1'));
  const clientSeed = sha256(Buffer.from('ht-client-1'));
  const actions = new Uint8Array(32);

  const rh0 = await replay('hilo', encodeHiLo(), 1_000_000_000n, seed, clientSeed, 1n, actions, 0, 0n, false);
  const vh0 = buildStepView('hilo', encodeHiLo(), 1_000_000_000n, actions, 0, rh0);
  assert.equal(vh0.cards.length, 1);
  assert.equal(vh0.multiplier, 1);

  const rt0 = await replay('towers', encodeTowers(1), 1_000_000_000n, seed, clientSeed, 1n, actions, 0, 0n, false);
  const vt0 = buildStepView('towers', encodeTowers(1), 1_000_000_000n, actions, 0, rt0);
  assert.equal(vt0.safeFloors, 0);
  assert.equal(vt0.lastFloor, null);
});

test('fetchSettledEvent retries when getTransaction returns null once then returns event', async () => {
  // Build a minimal valid BetSettled event log
  // Event discriminator for event:BetSettled
  const disc = Buffer.from(sha256(Buffer.from('event:BetSettled')).subarray(0, 8));
  // Total length at least 209 bytes:
  // 8 disc + 32 player + 8 nonce + 16 gameId + 8 amount + 8 payout + 32 params + 32 clientSeed + 32 serverSeed + 32 serverSeedHash + 1 verified
  const body = Buffer.alloc(209);
  disc.copy(body, 0);
  // player at 8..40
  // nonce at 40..48
  body.writeBigUInt64LE(1n, 40);
  // gameId at 48..64
  Buffer.from('dice').copy(body, 48);
  // amount at 64..72
  body.writeBigUInt64LE(1_000_000n, 64);
  // payout at 72..80
  body.writeBigUInt64LE(1_980_000n, 72);
  // verified at 208
  body.writeUInt8(1, 208);
  const log = `Program data: ${body.toString('base64')}`;

  let calls = 0;
  const ev = await fetchSettledEvent(
    'fake-sig',
    async () => {
      calls++;
      if (calls === 1) return null;
      return { meta: { logMessages: [log] } };
    },
    1,
  );
  assert.equal(calls, 2);
  assert.ok(ev);
  assert.equal(ev.payout, 1_980_000n);
});
