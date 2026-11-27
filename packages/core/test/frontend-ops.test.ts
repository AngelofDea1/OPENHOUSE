import test from 'node:test';
import assert from 'node:assert/strict';
import { decodePlayer, PLAYER_SIZE } from '../src/program.js';

function maxBetLamports(vault: { totalDeposited: bigint; reserved: bigint }, cfg: { kBps: number; paused: boolean }, maxPayoutX100: number): bigint {
  if (!vault || !cfg || cfg.paused) return 0n;
  const free = vault.totalDeposited > vault.reserved ? vault.totalDeposited - vault.reserved : 0n;
  if (free <= 0n) return 0n;
  const maxRisk = (free * BigInt(cfg.kBps)) / 10000n;
  const riskX100 = BigInt(Math.round(maxPayoutX100)) - 100n;
  if (riskX100 <= 0n) return maxRisk;
  return (maxRisk * 100n) / riskX100;
}

function mapHistoryRow(b: { amount?: number | string; payout?: number | string; time?: number; game?: string; nonce?: number; placeTx?: string; settleTx?: string; tx?: string }) {
  const amt = b.amount != null ? Number(b.amount) / 1e9 : 0;
  const pay = b.payout != null ? Number(b.payout) / 1e9 : 0;
  const mult = amt > 0 ? +(pay / amt).toFixed(2) : 0;
  const ts = b.time ? Number(b.time) * 1000 : 0;
  const gName = b.game ? b.game.charAt(0).toUpperCase() + b.game.slice(1) : 'Game';
  return { t: ts, g: gName, bet: amt, m: mult, pay, n: b.nonce ?? 0, placeTx: b.placeTx || '', settleTx: b.settleTx || b.tx || '' };
}

test('maxBetLamports: free 50 SOL, k 100, Dice at 49.5% (2.00x)', () => {
  const vault = { totalDeposited: 50_000_000_000n, reserved: 0n };
  const cfg = { kBps: 100, paused: false };
  // 50 SOL * 100 / 10000 = 0.5 SOL maxRisk; Dice 49.5% -> 99/49.5 = 2.00x (200) -> riskX100 = 100 -> 0.5 SOL
  assert.equal(maxBetLamports(vault, cfg, 200), 500_000_000n);
  assert.equal(maxBetLamports(vault, { kBps: 100, paused: true }, 200), 0n);
});

test('mapHistoryRow maps settler bet payload', () => {
  const row = mapHistoryRow({ amount: 100_000_000, payout: 198_000_000, time: 1700000000, game: 'dice', nonce: 7, placeTx: 'tx1', settleTx: 'tx2' });
  assert.equal(row.g, 'Dice');
  assert.equal(row.bet, 0.1);
  assert.equal(row.pay, 0.198);
  assert.equal(row.m, 1.98);
  assert.equal(row.n, 7);
  assert.equal(row.placeTx, 'tx1');
  assert.equal(row.settleTx, 'tx2');
});

test('decodePlayer decodes a PLAYER_SIZE (355) byte buffer', () => {
  assert.equal(PLAYER_SIZE, 355);
  const buf = new Uint8Array(PLAYER_SIZE);
  const p = decodePlayer(buf);
  assert.equal(p.balance, 0n);
  assert.equal(p.nonce, 0n);
  assert.equal(p.open.active, false);
});

test('maxBetLamports table for all 14 live games (50 SOL free, k=100 bps)', () => {
  const vault = { totalDeposited: 50_000_000_000n, reserved: 0n };
  const cfg = { kBps: 100, paused: false };
  const games = [
    ['Dice (49.5%)', 200],
    ['Flip', 198],
    ['Limbo (2x)', 200],
    ['Wheel (high)', 1500],
    ['Plinko (16 high)', 100000],
    ['Keno (10 picks)', 100000],
    ['Roulette (straight)', 3600],
    ['Hot Reels', 27500],
    ['Slots 777', 50000],
    ['Vault Break', 65000],
    ['Scratch', 20000],
    ['Sic Bo', 1200],
    ['Dragon Tiger', 1280],
    ['Baccarat', 900],
  ] as const;
  console.log('\n| Game | Max Payout x | Max Bet (SOL) |');
  console.log('|---|---|---|');
  for (const [name, multX100] of games) {
    const lam = maxBetLamports(vault, cfg, multX100);
    assert.ok(lam > 0n);
    console.log(`| ${name} | ${(multX100 / 100).toFixed(2)}x | ${(Number(lam) / 1e9).toFixed(4)} |`);
  }
});
