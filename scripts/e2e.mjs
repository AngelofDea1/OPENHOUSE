// scripts/e2e.mjs
// End-to-end test against live Vercel settler service for EVERY game
import { Keypair, Connection, Transaction, sendAndConfirmTransaction } from '@solana/web3.js';
import * as fs from 'fs';
import {
  OpenHouse, gameIdBytes, gameKind, isMultistepGame,
  encodeDice, encodeFlip, encodeLimbo, encodeWheel, encodePlinko, encodeKeno, encodeRoulette,
  encodeSlots, encodeScratch, encodeSicBo, encodeDragon, encodeBaccarat,
  encodeMines, encodeHiLo, encodeTowers, encodeBlackjack, encodePoker,
  winPayout, multistepMaxPayout, decodePlayer
} from '../packages/core/src/index.ts';

const SETTLER_URL = process.env.SETTLER_URL || 'https://openhouse-ten.vercel.app/api';
const RPC_URL = process.env.SOLANA_RPC_URL || 'https://api.devnet.solana.com';
const PROGRAM_ID = process.env.PROGRAM_ID || '7LECoyuUbynh66Frvi5yrbpQ1yYRc94epDvdY8p4oyvK';

const keypairPath = process.env.AUTHORITY_KEYPAIR || (process.env.HOME + '/.config/solana/id.json');
const authority = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(keypairPath, 'utf8'))));
const conn = new Connection(RPC_URL, 'confirmed');
const oh = new OpenHouse(PROGRAM_ID);

console.log('Testing live API at:', SETTLER_URL);
console.log('Player wallet:    ', authority.publicKey.toBase58());

// Verify player has funds
const plAcc = await conn.getAccountInfo(oh.player(authority.publicKey));
if (!plAcc) {
  console.error('Player account not funded');
  process.exit(1);
}
const player = decodePlayer(plAcc.data);
console.log('Player balance:   ', Number(player.balance) / 1e9, 'SOL');

const games = [
  // 15 single-step verified games
  { id: 'dice', params: encodeDice(true, 5000), maxPayout: (a) => winPayout('dice', encodeDice(true, 5000), a) },
  { id: 'flip', params: encodeFlip(false), maxPayout: (a) => winPayout('flip', encodeFlip(false), a) },
  { id: 'limbo', params: encodeLimbo(200), maxPayout: (a) => winPayout('limbo', encodeLimbo(200), a) },
  { id: 'crash', params: encodeLimbo(200), maxPayout: (a) => winPayout('crash', encodeLimbo(200), a) },
  { id: 'wheel', params: encodeWheel(0), maxPayout: (a) => winPayout('wheel', encodeWheel(0), a) },
  { id: 'plinko', params: encodePlinko(8, 0), maxPayout: (a) => winPayout('plinko', encodePlinko(8, 0), a) },
  { id: 'keno', params: encodeKeno([1, 2, 3]), maxPayout: (a) => winPayout('keno', encodeKeno([1, 2, 3]), a) },
  { id: 'roulette', params: encodeRoulette([{ slot: 0, units: 1 }]), maxPayout: (a) => winPayout('roulette', encodeRoulette([{ slot: 0, units: 1 }]), a) },
  { id: 'hotreels', params: encodeSlots(), maxPayout: (a) => winPayout('hotreels', encodeSlots(), a) },
  { id: 'slots777', params: encodeSlots(), maxPayout: (a) => winPayout('slots777', encodeSlots(), a) },
  { id: 'vaultbreak', params: encodeSlots(), maxPayout: (a) => winPayout('vaultbreak', encodeSlots(), a) },
  { id: 'scratch', params: encodeScratch(), maxPayout: (a) => winPayout('scratch', encodeScratch(), a) },
  { id: 'sicbo', params: encodeSicBo(0), maxPayout: (a) => winPayout('sicbo', encodeSicBo(0), a) },
  { id: 'dragon', params: encodeDragon(0), maxPayout: (a) => winPayout('dragon', encodeDragon(0), a) },
  { id: 'baccarat', params: encodeBaccarat(0), maxPayout: (a) => winPayout('baccarat', encodeBaccarat(0), a) },

  // 5 multi-step games
  { id: 'mines', multistep: true, params: encodeMines(3), maxPayout: (a) => multistepMaxPayout('mines', encodeMines(3), a) },
  { id: 'hilo', multistep: true, params: encodeHiLo(), maxPayout: (a) => multistepMaxPayout('hilo', encodeHiLo(), a) },
  { id: 'towers', multistep: true, params: encodeTowers(1), maxPayout: (a) => multistepMaxPayout('towers', encodeTowers(1), a) },
  { id: 'blackjack', multistep: true, params: encodeBlackjack(), maxPayout: (a) => multistepMaxPayout('blackjack', encodeBlackjack(), a) },
  { id: 'poker', multistep: true, params: encodePoker(), maxPayout: (a) => multistepMaxPayout('poker', encodePoker(), a) },
];

const results = [];

for (const game of games) {
  process.stdout.write(`Testing ${game.id.padEnd(12)} ... `);
  try {
    // Step 1: Request seed from settler
    const seedRes = await fetch(`${SETTLER_URL}/seed/next`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ wallet: authority.publicKey.toBase58() })
    });
    const seedText = await seedRes.text();
    let seedData;
    try { seedData = JSON.parse(seedText); } catch { throw new Error(`HTTP ${seedRes.status}: ${seedText}`); }
    if (!seedData.ok) throw new Error(`seed/next failed: ${seedData.error}`);

    const serverSeedHash = Buffer.from(seedData.seedHash, 'hex');
    const clientSeed = Buffer.alloc(32);
    const amount = 1_000_000n; // 0.001 SOL
    const maxPayout = game.maxPayout(amount);

    // Step 2: Place bet on-chain
    const placeIx = await oh.placeBet(authority.publicKey, authority.publicKey, {
      gameId: gameIdBytes(game.id),
      amount,
      maxPayout,
      params: game.params,
      clientSeed,
      serverSeedHash,
    });
    await sendAndConfirmTransaction(conn, new Transaction().add(placeIx), [authority], { commitment: 'confirmed' });

    // Step 3: Settle bet
    let settleData;
    if (game.multistep) {
      if (game.id === 'mines') {
        // Must reveal at least one cell before cashout
        const ixAction = await oh.playAction(authority.publicKey, authority.publicKey, 0);
        await sendAndConfirmTransaction(conn, new Transaction().add(ixAction), [authority], { commitment: 'confirmed' });
        const ixCash = await oh.playAction(authority.publicKey, authority.publicKey, 255);
        await sendAndConfirmTransaction(conn, new Transaction().add(ixCash), [authority], { commitment: 'confirmed' });
      } else if (game.id === 'hilo') {
        const ixAction = await oh.playAction(authority.publicKey, authority.publicKey, 0); // guess higher
        await sendAndConfirmTransaction(conn, new Transaction().add(ixAction), [authority], { commitment: 'confirmed' });
        const ixCash = await oh.playAction(authority.publicKey, authority.publicKey, 255);
        await sendAndConfirmTransaction(conn, new Transaction().add(ixCash), [authority], { commitment: 'confirmed' });
      } else if (game.id === 'towers') {
        const ixAction = await oh.playAction(authority.publicKey, authority.publicKey, 0);
        await sendAndConfirmTransaction(conn, new Transaction().add(ixAction), [authority], { commitment: 'confirmed' });
        const ixCash = await oh.playAction(authority.publicKey, authority.publicKey, 255);
        await sendAndConfirmTransaction(conn, new Transaction().add(ixCash), [authority], { commitment: 'confirmed' });
      } else if (game.id === 'blackjack') {
        // Stand (action 1) to finish round if not already natural
        const ixAction = await oh.playAction(authority.publicKey, authority.publicKey, 1);
        try {
          await sendAndConfirmTransaction(conn, new Transaction().add(ixAction), [authority], { commitment: 'confirmed' });
        } catch (_) {}
      } else if (game.id === 'poker') {
        // Hold all 5 cards (mask 31) to complete hand
        const ixAction = await oh.playAction(authority.publicKey, authority.publicKey, 31);
        await sendAndConfirmTransaction(conn, new Transaction().add(ixAction), [authority], { commitment: 'confirmed' });
      }

      // Step endpoint: check finished and settled
      const stepRes = await fetch(`${SETTLER_URL}/bet/step`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ wallet: authority.publicKey.toBase58(), expectActions: 0 })
      });
      const stepText = await stepRes.text();
      try { settleData = JSON.parse(stepText); } catch { throw new Error(`HTTP ${stepRes.status}: ${stepText}`); }
      if (!settleData.ok || !settleData.finished) {
        // Call /bet/settle to complete
        const settleRes = await fetch(`${SETTLER_URL}/bet/settle`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ wallet: authority.publicKey.toBase58() })
        });
        const settleText = await settleRes.text();
        try { settleData = JSON.parse(settleText); } catch { throw new Error(`HTTP ${settleRes.status}: ${settleText}`); }
        if (!settleData.ok) throw new Error(`settle failed: ${settleData.error}`);
      }
    } else {
      const settleRes = await fetch(`${SETTLER_URL}/bet/settle`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ wallet: authority.publicKey.toBase58() })
      });
      const settleText = await settleRes.text();
      try { settleData = JSON.parse(settleText); } catch { throw new Error(`HTTP ${settleRes.status}: ${settleText}`); }
      if (!settleData.ok) throw new Error(`settle failed: ${settleData.error}`);
    }

    const payout = settleData.settlement ? settleData.settlement.payout : settleData.payout;
    if (payout === null || payout === undefined) throw new Error('payout was null/undefined');

    console.log(`PASS (payout: ${payout})`);
    results.push({ game: game.id, status: 'PASS', payout: String(payout), error: null });
  } catch (err) {
    console.log(`FAIL (${err.message})`);
    results.push({ game: game.id, status: 'FAIL', payout: null, error: err.message });
  }
}

console.log('\n========================================');
console.log('E2E TEST RESULTS TABLE');
console.log('========================================');
console.table(results);

const failed = results.filter(r => r.status === 'FAIL');
if (failed.length > 0) {
  console.error(`\n${failed.length} games failed.`);
  process.exit(1);
} else {
  console.log('\nAll games passed successfully.');
}
