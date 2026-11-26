import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { fromHex, u32s } from '../src/fair';
import { replay, isMultistepGame, multistepMaxPayout } from '../src/multistep';

interface Vec {
  game: 'mines' | 'hilo' | 'towers' | 'blackjack' | 'poker';
  params: string;
  amount: number;
  server: string;
  client: string;
  nonce: number;
  actions: string;
  actionCount: number;
  extraStake: number;
  payout: number;
  finished: boolean;
}

const here = dirname(fileURLToPath(import.meta.url));
const vectors: Vec[] = JSON.parse(readFileSync(join(here, 'rust-vectors-b3.json'), 'utf8'));

test('batch3 rust parity vectors (mines, hilo, towers, blackjack, poker)', async () => {
  assert.ok(vectors.length > 500, `expected >500 vectors, got ${vectors.length}`);
  for (let i = 0; i < vectors.length; i++) {
    const v = vectors[i];
    assert.ok(isMultistepGame(v.game));
    const o = await replay(
      v.game,
      fromHex(v.params),
      BigInt(v.amount),
      fromHex(v.server),
      fromHex(v.client),
      BigInt(v.nonce),
      fromHex(v.actions),
      v.actionCount,
      BigInt(v.extraStake),
      false,
    );
    assert.equal(o.payout, BigInt(v.payout), `payout mismatch at #${i} (${v.game})`);
    assert.equal(o.finished, v.finished, `finished mismatch at #${i} (${v.game})`);
  }
});

test('draw stream supports counts 24, 40, 51', async () => {
  const s = new Uint8Array(32).fill(0x11);
  const c = new Uint8Array(32).fill(0x22);
  for (const n of [24, 40, 51]) {
    const d = await u32s(s, c, 7n, n);
    assert.equal(d.length, n);
  }
});

test('multistepMaxPayout sanity', () => {
  const pm = new Uint8Array(32); pm[0] = 3;
  assert.equal(multistepMaxPayout('mines', pm, 1_000_000n), 250_000_000n);
  assert.equal(multistepMaxPayout('hilo', new Uint8Array(32), 1_000_000n), 250_000_000n);
  assert.equal(multistepMaxPayout('blackjack', new Uint8Array(32), 1_000_000n), 4_000_000n);
  assert.equal(multistepMaxPayout('poker', new Uint8Array(32), 1_000_000n), 800_000_000n);
});
