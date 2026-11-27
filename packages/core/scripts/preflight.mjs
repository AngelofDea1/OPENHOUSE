// Read only health check of a deployment. Safe to run any time, on any cluster. Exit code 1 if a blocking check fails.
//   CLUSTER=devnet PROGRAM_ID=<id> SETTLER_URL=https://openhouse-ten.vercel.app/api npm run preflight
import { PublicKey } from '@solana/web3.js';
import { OpenHouse, decodeConfig, decodeVault } from '../src/index.ts';
import { connect, need } from './lib-cluster.mjs';

const { cluster, conn } = await connect();
const oh = new OpenHouse(need('PROGRAM_ID'));
const BPF_UPGRADEABLE = new PublicKey('BPFLoaderUpgradeab1e11111111111111111111111');
let bad = 0, warn = 0;
const ok = (m) => console.log('  ok   ', m);
const no = (m) => { bad++; console.log('  FAIL ', m); };
const wn = (m) => { warn++; console.log('  warn ', m); };

console.log(`preflight on ${cluster}, program ${oh.programId.toBase58()}`);
const prog = await conn.getAccountInfo(oh.programId);
prog?.executable ? ok('program is deployed and executable') : no('program is not deployed here');

if (prog) {
  const [pd] = PublicKey.findProgramAddressSync([oh.programId.toBytes()], BPF_UPGRADEABLE);
  const pda = await conn.getAccountInfo(pd);
  if (pda && pda.data.length >= 45 && pda.data[12] === 1) {
    const upg = new PublicKey(pda.data.slice(13, 45)).toBase58();
    cluster === 'mainnet-beta' ? wn(`upgrade authority is ${upg}. Must be the multisig, or none once audited`) : ok(`upgrade authority ${upg}`);
  } else ok('program is immutable (no upgrade authority)');
}

const cfgAcc = await conn.getAccountInfo(oh.config());
if (!cfgAcc) { no('config not initialised'); process.exit(1); }
const c = decodeConfig(cfgAcc.data);
console.log('  authority', c.authority.toBase58(), '\n  settler  ', c.settler.toBase58(), '\n  fee wallet', c.feeWallet.toBase58());
console.log('  k_bps', c.kBps, 'fee_bps', c.feeBps, 'min_bet', c.minBet.toString(), 'paused', c.paused);
c.authority.equals(c.settler) ? (cluster === 'mainnet-beta' ? no('authority and settler are the same key') : wn('authority and settler are the same key (fine on devnet, not on mainnet)')) : ok('authority and settler are different keys');
c.kBps > 100 ? (cluster === 'mainnet-beta' ? no(`k_bps is ${c.kBps}, the public promise is 100`) : wn(`k_bps is ${c.kBps} (temporary testing value, default is 100)`)) : ok(`k_bps ${c.kBps}`);
c.pendingAuthority.equals(PublicKey.default) ? ok('no authority change pending') : wn('an authority transfer is pending: ' + c.pendingAuthority.toBase58());
c.paused ? wn('betting is paused') : ok('betting is live');

const vAcc = await conn.getAccountInfo(oh.vault());
if (!vAcc) no('vault account missing');
else {
  const v = decodeVault(vAcc.data), rent = await conn.getMinimumBalanceForRentExemption(vAcc.data.length);
  const free = v.totalDeposited - v.reserved; // same formula as place_bet
  console.log('  vault lamports', vAcc.lamports, 'deposited', v.totalDeposited.toString(), 'reserved', v.reserved.toString(), 'fees', v.accruedFees.toString());
  free > 0n ? ok(`free vault ${Number(free) / 1e9} SOL, max risk per bet ${Number(free) * c.kBps / 10000 / 1e9} SOL`) : no('vault has no free liquidity');
  BigInt(vAcc.lamports) >= BigInt(rent) + v.totalDeposited + v.accruedFees ? ok('vault lamports cover deposits, fees and rent') : no('vault holds LESS than it owes depositors and the fee wallet');
}

if (process.env.SETTLER_URL) {
  try {
    const r = await (await fetch(process.env.SETTLER_URL.replace(/\/$/, '') + '/chain/info')).json();
    r.ok ? ok(`settler reachable, matches config: ${r.settlerMatchesConfig}, gas ${(r.settlerLamports / 1e9).toFixed(3)} SOL`) : no('settler answered: ' + r.error);
    if (r.ok && !r.settlerMatchesConfig) no('settler key differs from config.settler');
    if (r.ok && r.settlerLamports < 0.2e9) wn('settler gas wallet below 0.2 SOL');
  } catch (e) { no('settler not reachable: ' + e.message); }
} else wn('SETTLER_URL not set, skipped settler check');

console.log(bad ? `\n${bad} blocking problem(s)` : `\nall blocking checks passed${warn ? `, ${warn} warning(s)` : ''}`);
process.exit(bad ? 1 : 0);
