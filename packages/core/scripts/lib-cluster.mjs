// Shared by init-cluster.mjs and preflight.mjs. Refuses to talk to the wrong network.
import { Connection } from '@solana/web3.js';

export const GENESIS = {
  'devnet': 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG',
  'mainnet-beta': '5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d',
  'testnet': '4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY',
};
export const DEFAULT_RPC = { 'devnet': 'https://api.devnet.solana.com', 'mainnet-beta': 'https://api.mainnet-beta.solana.com' };

export const need = (k) => { const v = process.env[k]; if (!v) { console.error(`Set ${k}`); process.exit(1); } return v; };

export async function connect() {
  const cluster = process.env.CLUSTER || 'devnet';
  if (!GENESIS[cluster]) { console.error(`CLUSTER must be one of ${Object.keys(GENESIS).join(', ')}`); process.exit(1); }
  const rpc = process.env.RPC_URL || DEFAULT_RPC[cluster];
  if (!rpc) { console.error('Set RPC_URL'); process.exit(1); }
  const conn = new Connection(rpc, 'confirmed');
  const g = await conn.getGenesisHash();
  if (g !== GENESIS[cluster]) { console.error(`RPC is not on ${cluster} (genesis ${g}). Nothing was sent.`); process.exit(1); }
  return { cluster, rpc, conn };
}
