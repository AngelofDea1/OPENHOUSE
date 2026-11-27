// One time setup of the program on a cluster. Works for devnet and mainnet-beta.
//   CLUSTER=devnet AUTHORITY_KEYPAIR=~/.config/solana/id.json SETTLER_PUBKEY=<pubkey> PROGRAM_ID=<id> \
//     npm run init            (from packages/core)
// Optional: RPC_URL, VAULT_SOL, K_BPS (default 100), FEE_BPS (default 2000), MIN_BET_LAMPORTS (default 1000000)
// Mainnet only: also set I_UNDERSTAND_REAL_MONEY=yes. Authority and settler must be different keys there.
import { Keypair, PublicKey, Transaction, sendAndConfirmTransaction } from '@solana/web3.js';
import { readFileSync } from 'node:fs';
import { OpenHouse, decodeConfig } from '../src/index.ts';
import { connect, need } from './lib-cluster.mjs';

const { cluster, rpc, conn } = await connect();
const authority = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(need('AUTHORITY_KEYPAIR'), 'utf8'))));
const settler = new PublicKey(need('SETTLER_PUBKEY'));
const oh = new OpenHouse(need('PROGRAM_ID'));
const kBps = Number(process.env.K_BPS || 100), feeBps = Number(process.env.FEE_BPS || 2000), minBet = BigInt(process.env.MIN_BET_LAMPORTS || 1_000_000);

if (!(kBps >= 1 && kBps <= 1000)) { console.error('K_BPS out of range 1..1000'); process.exit(1); }
if (!(feeBps >= 0 && feeBps <= 5000)) { console.error('FEE_BPS out of range 0..5000'); process.exit(1); }
if (cluster === 'mainnet-beta') {
  if (process.env.I_UNDERSTAND_REAL_MONEY !== 'yes') { console.error('Mainnet uses real money. Set I_UNDERSTAND_REAL_MONEY=yes after you finished MAINNET.md section 1.'); process.exit(1); }
  if (authority.publicKey.equals(settler)) { console.error('On mainnet the authority and the settler must be different keys.'); process.exit(1); }
}
const prog = await conn.getAccountInfo(oh.programId);
if (!prog || !prog.executable) { console.error('No deployed program at PROGRAM_ID on this cluster.'); process.exit(1); }

console.log('cluster  ', cluster, rpc.replace(/api-key=[^&]+/, 'api-key=***'));
console.log('program  ', oh.programId.toBase58());
console.log('authority', authority.publicKey.toBase58(), (await conn.getBalance(authority.publicKey)) / 1e9, 'SOL');
console.log('settler  ', settler.toBase58());
console.log('vault PDA', oh.vault().toBase58(), ' config PDA', oh.config().toBase58());
console.log('k_bps', kBps, 'fee_bps', feeBps, 'min_bet', minBet.toString());

const send = (ix) => sendAndConfirmTransaction(conn, new Transaction().add(ix), [authority], { commitment: 'confirmed' });
const cfgAcc = await conn.getAccountInfo(oh.config());
if (!cfgAcc) {
  console.log('initialize ->', await send(await oh.initialize(authority.publicKey, kBps, feeBps, minBet, settler)));
} else {
  const c = decodeConfig(cfgAcc.data);
  console.log('already initialised. settler on chain:', c.settler.toBase58());
  if (!c.settler.equals(settler)) console.log('WARNING settler differs from SETTLER_PUBKEY, use set_config to change it');
}
const sol = Number(process.env.VAULT_SOL || 0);
if (sol > 0) console.log(`deposit_vault ${sol} SOL ->`, await send(await oh.depositVault(authority.publicKey, BigInt(Math.round(sol * 1e9)))));
console.log('done. now run: npm run preflight');
