import { Connection, PublicKey } from '@solana/web3.js';
import { OpenHouse, decodeConfig, decodeVault, decodePlayer, PLAYER_SIZE } from '../src/index.ts';

const RPC_URL = process.env.RPC_URL || process.env.SOLANA_RPC_URL || 'https://api.devnet.solana.com';
const PROGRAM_ID = process.env.PROGRAM_ID;
const SETTLER_URL = process.env.SETTLER_URL || '';
const WEBHOOK_URL = process.env.WEBHOOK_URL || '';
const CLUSTER = process.env.CLUSTER || 'devnet';

async function main() {
  if (!PROGRAM_ID) {
    console.log(JSON.stringify({ ok: false, error: 'PROGRAM_ID is required' }));
    process.exit(1);
  }
  const conn = new Connection(RPC_URL, 'confirmed');
  const oh = new OpenHouse(PROGRAM_ID);
  const [cfgInfo, vaultInfo, vaultLamports, playerAccs] = await Promise.all([
    conn.getAccountInfo(oh.config()),
    conn.getAccountInfo(oh.vault()),
    conn.getBalance(oh.vault()),
    conn.getProgramAccounts(new PublicKey(PROGRAM_ID), { filters: [{ dataSize: PLAYER_SIZE }] }),
  ]);

  const failures = [];
  if (!cfgInfo || !vaultInfo) {
    failures.push('Missing config or vault account');
  }

  const cfg = cfgInfo ? decodeConfig(cfgInfo.data) : null;
  const vault = vaultInfo ? decodeVault(vaultInfo.data) : null;

  let settlerSol = null;
  if (cfg) {
    const settlerLamports = await conn.getBalance(cfg.settler);
    settlerSol = settlerLamports / 1e9;
    if (settlerSol < 0.2) failures.push(`Settler gas low: ${settlerSol.toFixed(4)} SOL`);
    if (cfg.paused) failures.push('Config is paused');
    if (CLUSTER === 'mainnet-beta' && cfg.authority.equals(cfg.settler)) {
      failures.push('Authority equals settler on mainnet');
    }
  }

  if (vault) {
    const rent = await conn.getMinimumBalanceForRentExemption(vaultInfo.data.length);
    if (BigInt(vaultLamports) < vault.totalDeposited + vault.accruedFees + BigInt(rent)) {
      failures.push('Vault lamports < totalDeposited + accruedFees + rent');
    }
    if (vault.reserved > vault.totalDeposited) {
      failures.push('Vault reserved > totalDeposited');
    }
  }

  const now = Math.floor(Date.now() / 1000);
  let openBets = 0;
  let staleBets = 0;
  for (const acc of playerAccs) {
    try {
      const p = decodePlayer(acc.account.data);
      if (p.open.active) {
        openBets++;
        if (now - Number(p.open.createdAt) > 600) staleBets++;
      }
    } catch {}
  }
  if (staleBets > 0) failures.push(`${staleBets} open bet(s) older than 10 minutes`);

  const report = {
    ok: failures.length === 0,
    ts: new Date().toISOString(),
    cluster: CLUSTER,
    openBets,
    staleBets,
    settlerSol,
    failures,
  };
  console.log(JSON.stringify(report));

  if (!report.ok && WEBHOOK_URL) {
    await fetch(WEBHOOK_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ text: `OpenHouse monitor alert (${CLUSTER}): ${failures.join('; ')}`, content: `OpenHouse monitor alert (${CLUSTER}): ${failures.join('; ')}` }),
    }).catch(() => {});
  }
  process.exit(report.ok ? 0 : 1);
}

main().catch((e) => {
  console.log(JSON.stringify({ ok: false, error: String(e?.message || e) }));
  process.exit(1);
});
