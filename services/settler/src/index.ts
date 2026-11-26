/**
 * OpenHouse settler v2. Stateless: everything it needs lives on chain or is derived from the master secret.
 * It cannot move player funds. It can only commit seeds and settle bets, and the program re-checks every result.
 */
import express from 'express';
import cors from 'cors';
import { createHmac, createHash, randomBytes, timingSafeEqual } from 'node:crypto';
import { Connection, Keypair, PublicKey, Transaction, sendAndConfirmTransaction, ComputeBudgetProgram } from '@solana/web3.js';
import {
  OpenHouse, decodeConfig, decodePlayer, decodeCommitment, parseSettled,
  gameKind, isMultistepGame, resolve, winPayout, explainError, hex, fromHex,
} from '../../../packages/core/src/index';

const env = (k: string, d?: string) => process.env[k] ?? d;
const RPC = env('SOLANA_RPC_URL', 'https://api.devnet.solana.com')!;
const PROGRAM = env('PROGRAM_ID')!;
const CLUSTER = env('CLUSTER', 'devnet')!;
const GENESIS: Record<string, string> = {
  'devnet': 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG',
  'mainnet-beta': '5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d',
  'testnet': '4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY',
};
const ORIGINS = (env('ALLOWED_ORIGINS', '') || '').split(',').map((s) => s.trim()).filter(Boolean);

function loadSecrets() {
  const sk = env('SETTLER_SECRET_KEY'), master = env('SEED_MASTER_SECRET');
  if (!PROGRAM || !sk || !master || master.length < 32) return null;
  if (!GENESIS[CLUSTER]) return null;
  // Real money needs an explicit second switch, so a copied devnet env can never start signing on mainnet by accident.
  if (CLUSTER === 'mainnet-beta' && env('MAINNET_CONFIRM') !== 'yes-real-money') return null;
  try { return { kp: Keypair.fromSecretKey(Uint8Array.from(JSON.parse(sk))), master: Buffer.from(master) }; } catch { return null; }
}
const secrets = loadSecrets();
const conn = new Connection(RPC, 'confirmed');
const oh = new OpenHouse(PROGRAM || undefined);

/** per bet seed, never stored. Losing the master secret means losing the ability to settle, so back it up. */
const seedFor = (index: bigint) => {
  const b = Buffer.alloc(8); b.writeBigUInt64LE(index);
  return new Uint8Array(createHmac('sha256', secrets!.master).update('openhouse-seed').update(b).digest());
};
const sha = (b: Uint8Array) => new Uint8Array(createHash('sha256').update(b).digest());

// tiny in memory rate limiter, good enough per serverless instance; the on chain checks are the real guard
const hits = new Map<string, number[]>();
const limited = (k: string, max: number, windowMs: number) => {
  const now = Date.now(), arr = (hits.get(k) ?? []).filter((t) => now - t < windowMs);
  arr.push(now); hits.set(k, arr);
  if (hits.size > 5000) hits.clear();
  return arr.length > max;
};

const app = express();
app.disable('x-powered-by');
app.use(express.json({ limit: '2kb' }));
app.use(cors({ origin: ORIGINS.length ? ORIGINS : false, methods: ['GET', 'POST'] }));

const pk = (v: unknown) => { try { return new PublicKey(String(v)); } catch { return null; } };
const fail = (res: express.Response, code: number, error: string) => res.status(code).json({ ok: false, error });
const wrap = (fn: express.RequestHandler): express.RequestHandler => (req, res, next) =>
  Promise.resolve(fn(req, res, next)).catch((e) => { console.error(e); fail(res, 500, explainError(e)); });

app.use((req, res, next) => {
  if (req.path === '/health' || req.path === '/api/health') return next();
  if (!secrets) return fail(res, 503, 'Settler is not configured');
  next();
});

/** The RPC must be on the cluster we think it is on. Checked once, before anything is signed. */
let clusterOk = false;
app.use(wrap(async (req, res, next) => {
  if (clusterOk || req.path === '/health' || req.path === '/api/health') return next();
  const g = await conn.getGenesisHash();
  if (g !== GENESIS[CLUSTER]) return fail(res, 503, `RPC is not on ${CLUSTER}, refusing to sign`);
  clusterOk = true;
  next();
}));


app.get(['/health', '/api/health'], (_q, res) => res.json({ ok: true, configured: !!secrets, cluster: CLUSTER, program: PROGRAM || null }));

app.get(['/chain/info', '/api/chain/info'], wrap(async (_q, res) => {
  const [cfg, bal] = await Promise.all([conn.getAccountInfo(oh.config()), conn.getBalance(secrets!.kp.publicKey)]);
  if (!cfg) return fail(res, 404, 'Program is not initialised on this cluster');
  const c = decodeConfig(cfg.data);
  res.json({ ok: true, program: PROGRAM, settler: secrets!.kp.publicKey.toBase58(), settlerMatchesConfig: c.settler.equals(secrets!.kp.publicKey), settlerLamports: bal, paused: c.paused, minBet: c.minBet.toString(), kBps: c.kBps });
}));

/** Commit a fresh seed for a player who has a funded account. Returns only the hash. */
app.post(['/seed/next', '/api/seed/next'], wrap(async (req, res) => {
  const wallet = pk(req.body?.wallet);
  if (!wallet) return fail(res, 400, 'wallet is required');
  const ip = String(req.headers['x-forwarded-for'] ?? req.socket.remoteAddress ?? '');
  if (limited('ip:' + ip, 60, 60_000) || limited('w:' + wallet.toBase58(), 20, 60_000)) return fail(res, 429, 'Too many requests, slow down');
  const [cfgAcc, plAcc] = await Promise.all([conn.getAccountInfo(oh.config()), conn.getAccountInfo(oh.player(wallet))]);
  if (!cfgAcc) return fail(res, 404, 'Program is not initialised');
  const cfg = decodeConfig(cfgAcc.data);
  if (!cfg.settler.equals(secrets!.kp.publicKey)) return fail(res, 503, 'This settler is not the configured settler');
  if (cfg.paused) return fail(res, 409, 'Betting is paused');
  if (!plAcc) return fail(res, 409, 'Deposit to your player account first');
  const p = decodePlayer(plAcc.data);
  if (p.open.active) return fail(res, 409, 'Finish your open bet first');
  if (p.balance < cfg.minBet) return fail(res, 409, 'Deposit to your player account first');
  const index = randomBytes(8).readBigUInt64LE();
  const hash = sha(seedFor(index));
  const ix = await oh.commitSeed(secrets!.kp.publicKey, hash, index);
  const sig = await sendAndConfirmTransaction(conn, new Transaction().add(ix), [secrets!.kp], { commitment: 'confirmed' });
  res.json({ ok: true, seedHash: hex(hash), index: index.toString(), tx: sig });
}));

/** Reads the open bet from chain, reveals the seed, lets the program verify and pay. */
app.post(['/bet/settle', '/api/bet/settle'], wrap(async (req, res) => {
  const wallet = pk(req.body?.wallet);
  if (!wallet) return fail(res, 400, 'wallet is required');
  if (limited('s:' + wallet.toBase58(), 30, 60_000)) return fail(res, 429, 'Too many requests, slow down');
  const plAcc = await conn.getAccountInfo(oh.player(wallet));
  if (!plAcc) return fail(res, 404, 'No player account');
  const p = decodePlayer(plAcc.data);
  const o = p.open;
  if (!o.active) return fail(res, 409, 'There is no open bet');
  const kind = gameKind(o.gameId);
  const isMulti = isMultistepGame(o.gameId);
  if (!kind && !isMulti) return fail(res, 501, 'This game is not verified on chain yet, so it cannot be settled here');
  const cAcc = await conn.getAccountInfo(oh.commitment(o.serverSeedHash));
  if (!cAcc) return fail(res, 409, 'Seed commitment is missing, use refund after two minutes');
  const c = decodeCommitment(cAcc.data);
  const seed = seedFor(c.index);
  if (hex(sha(seed)) !== hex(o.serverSeedHash)) return fail(res, 500, 'Seed derivation mismatch, check SEED_MASTER_SECRET');
  const ix = await oh.settleBet(secrets!.kp.publicKey, wallet, o.serverSeedHash, seed, 0n);
  const tx = new Transaction().add(ComputeBudgetProgram.setComputeUnitLimit({ units: isMulti ? 400_000 : 200_000 }), ix);
  const sig = await sendAndConfirmTransaction(conn, tx, [secrets!.kp], { commitment: 'confirmed' });
  const t = await conn.getTransaction(sig, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 });
  const ev = (await parseSettled(t?.meta?.logMessages ?? []))[0];
  res.json({
    ok: true, tx: sig, game: o.gameId, amount: o.amount.toString(), nonce: o.nonce.toString(),
    payout: ev ? ev.payout.toString() : null, win: ev ? ev.payout > 0n : null,
    serverSeed: hex(seed), serverSeedHash: hex(o.serverSeedHash), clientSeed: hex(o.clientSeed), params: hex(o.params),
    verifiedOnChain: ev?.verifiedOnChain ?? true,
  });
}));

/** Force-settle a timed-out multi-step round. */
app.post(['/bet/force-settle', '/api/bet/force-settle'], wrap(async (req, res) => {
  const wallet = pk(req.body?.wallet);
  if (!wallet) return fail(res, 400, 'wallet is required');
  if (limited('fs:' + wallet.toBase58(), 10, 60_000)) return fail(res, 429, 'Too many requests, slow down');
  const plAcc = await conn.getAccountInfo(oh.player(wallet));
  if (!plAcc) return fail(res, 404, 'No player account');
  const p = decodePlayer(plAcc.data);
  const o = p.open;
  if (!o.active) return fail(res, 409, 'There is no open bet');
  if (!isMultistepGame(o.gameId)) return fail(res, 400, 'Not a multi-step game');
  const cAcc = await conn.getAccountInfo(oh.commitment(o.serverSeedHash));
  if (!cAcc) return fail(res, 409, 'Seed commitment is missing');
  const c = decodeCommitment(cAcc.data);
  const seed = seedFor(c.index);
  if (hex(sha(seed)) !== hex(o.serverSeedHash)) return fail(res, 500, 'Seed derivation mismatch, check SEED_MASTER_SECRET');
  const ix = await oh.forceSettle(secrets!.kp.publicKey, secrets!.kp.publicKey, wallet, o.serverSeedHash, seed);
  const tx = new Transaction().add(ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }), ix);
  const sig = await sendAndConfirmTransaction(conn, tx, [secrets!.kp], { commitment: 'confirmed' });
  const t = await conn.getTransaction(sig, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 });
  const ev = (await parseSettled(t?.meta?.logMessages ?? []))[0];
  res.json({
    ok: true, tx: sig, game: o.gameId, amount: o.amount.toString(), nonce: o.nonce.toString(),
    payout: ev ? ev.payout.toString() : null, win: ev ? ev.payout > 0n : null,
    serverSeed: hex(seed), serverSeedHash: hex(o.serverSeedHash), clientSeed: hex(o.clientSeed), params: hex(o.params),
    verifiedOnChain: ev?.verifiedOnChain ?? true,
  });
}));

/** History from transaction logs, nothing is stored off chain. */
app.get(['/bets', '/api/bets'], wrap(async (req, res) => {
  const wallet = pk(req.query.wallet);
  if (!wallet) return fail(res, 400, 'wallet is required');
  const sigs = await conn.getSignaturesForAddress(oh.player(wallet), { limit: 25 });
  const out: any[] = [];
  for (const s of sigs) {
    if (s.err) continue;
    const t = await conn.getTransaction(s.signature, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 });
    for (const e of await parseSettled(t?.meta?.logMessages ?? []))
      if (e.player === wallet.toBase58())
        out.push({ tx: s.signature, time: s.blockTime, game: e.gameId, amount: e.amount.toString(), payout: e.payout.toString(), nonce: e.nonce.toString(), serverSeed: hex(e.serverSeed), serverSeedHash: hex(e.serverSeedHash), clientSeed: hex(e.clientSeed), params: hex(e.params) });
  }
  res.json({ ok: true, bets: out });
}));

/** Cron target. Takes back rent from commitments that were used or went stale. Needs ADMIN_TOKEN or CRON_SECRET. */
function isAuthorizedCron(authHeader: string | undefined): boolean {
  const given = Buffer.from(String(authHeader ?? ''));
  for (const tok of [env('ADMIN_TOKEN'), env('CRON_SECRET')]) {
    if (!tok) continue;
    const want = Buffer.from(`Bearer ${tok}`);
    if (given.length === want.length && timingSafeEqual(given, want)) return true;
  }
  return false;
}
app.all(['/admin/sweep', '/api/admin/sweep'], wrap(async (req, res) => {
  if (req.method !== 'GET' && req.method !== 'POST') return fail(res, 405, 'Method not allowed');
  if (!isAuthorizedCron(req.headers.authorization)) return fail(res, 401, 'Not allowed');
  const accs = await conn.getProgramAccounts(oh.programId, { filters: [{ dataSize: 90 }] });
  const now = Math.floor(Date.now() / 1000);
  let reclaimed = 0;
  for (const a of accs) {
    let c; try { c = decodeCommitment(a.account.data); } catch { continue; }
    if (!c.settler.equals(secrets!.kp.publicKey)) continue;
    if (!(c.used || now >= Number(c.committedAt) + 86_400)) continue;
    try {
      const ix = await oh.reclaimCommitment(secrets!.kp.publicKey, c.seedHash);
      await sendAndConfirmTransaction(conn, new Transaction().add(ix), [secrets!.kp], { commitment: 'confirmed' });
      reclaimed++;
    } catch (e) { console.error('reclaim failed', explainError(e)); }
    if (reclaimed >= 25) break;
  }
  res.json({ ok: true, reclaimed });
}));

app.use((_q, res) => fail(res, 404, 'Not found'));

if (require.main === module) {
  const port = Number(env('PORT', '8787'));
  app.listen(port, () => console.log(`settler on :${port} program ${PROGRAM || '(unset)'} configured=${!!secrets}`));
}
export default app;
