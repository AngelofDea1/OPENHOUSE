import { PublicKey, SystemProgram, TransactionInstruction } from '@solana/web3.js';
import { sha256 } from './fair';

const te = new TextEncoder();
export const DEFAULT_PROGRAM_ID = '7LECoyuUbynh66Frvi5yrbpQ1yYRc94epDvdY8p4oyvK';

// ---------- discriminators ----------
const cache = new Map<string, Uint8Array>();
async function disc(kind: 'global' | 'account' | 'event', name: string): Promise<Uint8Array> {
  const k = kind + ':' + name;
  let v = cache.get(k);
  if (!v) { v = (await sha256(te.encode(k))).slice(0, 8); cache.set(k, v); }
  return v;
}

// ---------- tiny borsh writer ----------
class W {
  parts: number[] = [];
  u8(v: number) { this.parts.push(v & 255); return this; }
  u16(v: number) { this.parts.push(v & 255, (v >> 8) & 255); return this; }
  u64(v: bigint) { for (let i = 0n; i < 8n; i++) this.parts.push(Number((v >> (8n * i)) & 255n)); return this; }
  i64(v: bigint) { return this.u64(BigInt.asUintN(64, v)); }
  bytes(b: Uint8Array) { for (const x of b) this.parts.push(x); return this; }
  key(k: PublicKey) { return this.bytes(k.toBytes()); }
  done() { return Uint8Array.from(this.parts); }
}

// ---------- addresses ----------
export class OpenHouse {
  readonly programId: PublicKey;
  constructor(programId: string | PublicKey = DEFAULT_PROGRAM_ID) {
    this.programId = typeof programId === 'string' ? new PublicKey(programId) : programId;
  }
  private pda(...seeds: Uint8Array[]) { return PublicKey.findProgramAddressSync(seeds, this.programId)[0]; }
  config() { return this.pda(te.encode('config')); }
  vault() { return this.pda(te.encode('vault')); }
  player(w: PublicKey) { return this.pda(te.encode('player'), w.toBytes()); }
  share(w: PublicKey) { return this.pda(te.encode('vault_share'), w.toBytes()); }
  commitment(hash: Uint8Array) { return this.pda(te.encode('seed_commitment'), hash); }

  private async ix(name: string, args: Uint8Array, keys: { pubkey: PublicKey; isSigner?: boolean; isWritable?: boolean }[]) {
    const d = await disc('global', name);
    const data = new Uint8Array(8 + args.length);
    data.set(d, 0); data.set(args, 8);
    return new TransactionInstruction({
      programId: this.programId, data: data as any,
      keys: keys.map((k) => ({ pubkey: k.pubkey, isSigner: !!k.isSigner, isWritable: !!k.isWritable })),
    });
  }
  private static S = SystemProgram.programId;

  // ---------- player ----------
  depositPlayer(user: PublicKey, lamports: bigint) {
    return this.ix('deposit_player', new W().u64(lamports).done(), [
      { pubkey: this.vault(), isWritable: true }, { pubkey: this.player(user), isWritable: true },
      { pubkey: user, isSigner: true, isWritable: true }, { pubkey: OpenHouse.S }]);
  }
  withdrawPlayer(user: PublicKey, lamports: bigint) {
    return this.ix('withdraw_player', new W().u64(lamports).done(), [
      { pubkey: this.vault(), isWritable: true }, { pubkey: this.player(user), isWritable: true },
      { pubkey: user, isSigner: true, isWritable: true }]);
  }
  createSession(user: PublicKey, sessionKey: PublicKey, durationSeconds: bigint, maxSpend: bigint) {
    return this.ix('create_session', new W().key(sessionKey).i64(durationSeconds).u64(maxSpend).done(), [
      { pubkey: this.player(user), isWritable: true }, { pubkey: user, isSigner: true }]);
  }
  revokeSession(user: PublicKey) {
    return this.ix('revoke_session', new Uint8Array(0), [{ pubkey: this.player(user), isWritable: true }, { pubkey: user, isSigner: true }]);
  }
  setLimits(user: PublicKey, lamports: bigint) {
    return this.ix('set_limits', new W().u64(lamports).done(), [{ pubkey: this.player(user), isWritable: true }, { pubkey: user, isSigner: true }]);
  }
  selfExclude(user: PublicKey, days: number) {
    return this.ix('self_exclude', new W().u16(days).done(), [{ pubkey: this.player(user), isWritable: true }, { pubkey: user, isSigner: true }]);
  }
  /** `signer` is the player wallet or a registered session key. */
  placeBet(player: PublicKey, signer: PublicKey, a: { gameId: Uint8Array; amount: bigint; maxPayout: bigint; params: Uint8Array; clientSeed: Uint8Array; serverSeedHash: Uint8Array }) {
    const args = new W().bytes(a.gameId).u64(a.amount).u64(a.maxPayout).bytes(a.params).bytes(a.clientSeed).bytes(a.serverSeedHash).done();
    return this.ix('place_bet', args, [
      { pubkey: this.config() }, { pubkey: this.vault(), isWritable: true }, { pubkey: this.player(player), isWritable: true },
      { pubkey: this.commitment(a.serverSeedHash), isWritable: true }, { pubkey: signer, isSigner: true }]);
  }
  refundBet(caller: PublicKey, player: PublicKey) {
    return this.ix('refund_bet', new Uint8Array(0), [
      { pubkey: this.config() }, { pubkey: this.vault(), isWritable: true }, { pubkey: this.player(player), isWritable: true },
      { pubkey: caller, isSigner: true }]);
  }

  // ---------- liquidity providers ----------
  depositVault(user: PublicKey, lamports: bigint) {
    return this.ix('deposit_vault', new W().u64(lamports).done(), [
      { pubkey: this.vault(), isWritable: true }, { pubkey: this.share(user), isWritable: true },
      { pubkey: user, isSigner: true, isWritable: true }, { pubkey: OpenHouse.S }]);
  }
  withdrawVault(user: PublicKey, shares: bigint) {
    return this.ix('withdraw_vault', new W().u64(shares).done(), [
      { pubkey: this.vault(), isWritable: true }, { pubkey: this.share(user), isWritable: true },
      { pubkey: user, isSigner: true, isWritable: true }]);
  }

  // ---------- settler ----------
  commitSeed(settler: PublicKey, seedHash: Uint8Array, index: bigint) {
    return this.ix('commit_seed', new W().bytes(seedHash).u64(index).done(), [
      { pubkey: this.config() }, { pubkey: this.commitment(seedHash), isWritable: true },
      { pubkey: settler, isSigner: true, isWritable: true }, { pubkey: OpenHouse.S }]);
  }
  settleBet(settler: PublicKey, player: PublicKey, openSeedHash: Uint8Array, serverSeed: Uint8Array, payoutHint: bigint) {
    return this.ix('settle_bet', new W().bytes(serverSeed).u64(payoutHint).done(), [
      { pubkey: this.config() }, { pubkey: this.vault(), isWritable: true }, { pubkey: this.player(player), isWritable: true },
      { pubkey: this.commitment(openSeedHash), isWritable: true }, { pubkey: settler, isSigner: true, isWritable: true }]);
  }
  playAction(player: PublicKey, signer: PublicKey, action: number) {
    return this.ix('play_action', new W().u8(action).done(), [
      { pubkey: this.config() }, { pubkey: this.vault(), isWritable: true }, { pubkey: this.player(player), isWritable: true },
      { pubkey: signer, isSigner: true }]);
  }
  forceSettle(settler: PublicKey, caller: PublicKey, player: PublicKey, openSeedHash: Uint8Array, serverSeed: Uint8Array) {
    return this.ix('force_settle', new W().bytes(serverSeed).done(), [
      { pubkey: this.config() }, { pubkey: this.vault(), isWritable: true }, { pubkey: this.player(player), isWritable: true },
      { pubkey: this.commitment(openSeedHash), isWritable: true }, { pubkey: settler, isSigner: true, isWritable: true },
      { pubkey: caller, isSigner: true }]);
  }
  reclaimCommitment(settler: PublicKey, seedHash: Uint8Array) {
    return this.ix('reclaim_commitment', new Uint8Array(0), [
      { pubkey: this.config() }, { pubkey: this.commitment(seedHash), isWritable: true }, { pubkey: settler, isSigner: true, isWritable: true }]);
  }

  // ---------- authority ----------
  initialize(authority: PublicKey, kBps: number, feeBps: number, minBet: bigint, settler: PublicKey) {
    return this.ix('initialize', new W().u16(kBps).u16(feeBps).u64(minBet).key(settler).done(), [
      { pubkey: this.config(), isWritable: true }, { pubkey: this.vault(), isWritable: true },
      { pubkey: authority, isSigner: true, isWritable: true }, { pubkey: OpenHouse.S }]);
  }
  pause(authority: PublicKey, paused: boolean) {
    return this.ix('pause', new W().u8(paused ? 1 : 0).done(), [{ pubkey: this.config(), isWritable: true }, { pubkey: authority, isSigner: true }]);
  }
  setConfig(authority: PublicKey, kBps: number, feeBps: number, minBet: bigint, feeWallet: PublicKey, settler: PublicKey) {
    return this.ix('set_config', new W().u16(kBps).u16(feeBps).u64(minBet).key(feeWallet).key(settler).done(), [
      { pubkey: this.config(), isWritable: true }, { pubkey: authority, isSigner: true }]);
  }
  withdrawFees(authority: PublicKey, feeWallet: PublicKey, lamports: bigint) {
    return this.ix('withdraw_fees', new W().u64(lamports).done(), [
      { pubkey: this.config() }, { pubkey: this.vault(), isWritable: true }, { pubkey: feeWallet, isWritable: true }, { pubkey: authority, isSigner: true }]);
  }
  proposeAuthority(authority: PublicKey, next: PublicKey) {
    return this.ix('propose_authority', new W().key(next).done(), [{ pubkey: this.config(), isWritable: true }, { pubkey: authority, isSigner: true }]);
  }
  acceptAuthority(next: PublicKey) {
    return this.ix('accept_authority', new Uint8Array(0), [{ pubkey: this.config(), isWritable: true }, { pubkey: next, isSigner: true }]);
  }
}

// ---------- account decoders (fixed layouts, checked by programs/openhouse/src/state.rs tests) ----------
const view = (d: Uint8Array) => new DataView(d.buffer, d.byteOffset, d.byteLength);
const key = (d: Uint8Array, o: number) => new PublicKey(d.slice(o, o + 32));
const u64 = (d: Uint8Array, o: number) => view(d).getBigUint64(o, true);
const i64 = (d: Uint8Array, o: number) => view(d).getBigInt64(o, true);

export async function hasDisc(d: Uint8Array, name: string) {
  const x = await disc('account', name);
  return x.every((b, i) => d[i] === b);
}

export interface ConfigAcc { authority: PublicKey; pendingAuthority: PublicKey; settler: PublicKey; feeWallet: PublicKey; kBps: number; feeBps: number; minBet: bigint; paused: boolean }
export const decodeConfig = (d: Uint8Array): ConfigAcc => ({
  authority: key(d, 8), pendingAuthority: key(d, 40), settler: key(d, 72), feeWallet: key(d, 104),
  kBps: view(d).getUint16(136, true), feeBps: view(d).getUint16(138, true), minBet: u64(d, 140), paused: d[148] === 1,
});

export interface VaultAcc { totalShares: bigint; totalDeposited: bigint; reserved: bigint; accruedFees: bigint; netPnl: bigint; feeHwm: bigint }
export const decodeVault = (d: Uint8Array): VaultAcc => ({
  totalShares: u64(d, 8), totalDeposited: u64(d, 16), reserved: u64(d, 24), accruedFees: u64(d, 32), netPnl: i64(d, 40), feeHwm: i64(d, 48),
});

export const decodeShare = (d: Uint8Array) => ({ owner: key(d, 8), shares: u64(d, 40) });

export interface OpenBetAcc {
  active: boolean; gameId: string; amount: bigint; maxPayout: bigint; params: Uint8Array;
  clientSeed: Uint8Array; serverSeedHash: Uint8Array; nonce: bigint; createdAt: bigint;
  actions: Uint8Array; actionCount: number; lastActionAt: bigint; extraStake: bigint;
}
export interface PlayerAcc {
  wallet: PublicKey; balance: bigint; nonce: bigint; sessionKey: PublicKey; sessionExpires: bigint; sessionMaxSpend: bigint; sessionSpent: bigint;
  dailyLossLimit: bigint; dailyLossDay: bigint; dailyLossAccumulated: bigint; pendingLimit: bigint; pendingLimitAt: bigint; excludedUntil: bigint; open: OpenBetAcc;
}
export const PLAYER_SIZE = 355;
export function decodePlayer(d: Uint8Array): PlayerAcc {
  const g = d.slice(161, 177);
  const n = g.indexOf(0);
  return {
    wallet: key(d, 8), balance: u64(d, 40), nonce: u64(d, 48), sessionKey: key(d, 56), sessionExpires: i64(d, 88), sessionMaxSpend: u64(d, 96), sessionSpent: u64(d, 104),
    dailyLossLimit: u64(d, 112), dailyLossDay: i64(d, 120), dailyLossAccumulated: u64(d, 128), pendingLimit: u64(d, 136), pendingLimitAt: i64(d, 144), excludedUntil: i64(d, 152),
    open: {
      active: d[160] === 1, gameId: new TextDecoder().decode(n < 0 ? g : g.slice(0, n)), amount: u64(d, 177), maxPayout: u64(d, 185),
      params: d.slice(193, 225), clientSeed: d.slice(225, 257), serverSeedHash: d.slice(257, 289), nonce: u64(d, 289), createdAt: i64(d, 297),
      actions: d.slice(305, 337), actionCount: d[337], lastActionAt: i64(d, 338), extraStake: u64(d, 346),
    },
  };
}

export const decodeCommitment = (d: Uint8Array) => ({ settler: key(d, 8), seedHash: d.slice(40, 72), index: u64(d, 72), committedAt: i64(d, 80), used: d[88] === 1 });

// ---------- events from transaction logs ----------
export interface SettledEvent { player: string; nonce: bigint; gameId: string; amount: bigint; payout: bigint; params: Uint8Array; clientSeed: Uint8Array; serverSeed: Uint8Array; serverSeedHash: Uint8Array; verifiedOnChain: boolean }

export async function parseSettled(logs: string[]): Promise<SettledEvent[]> {
  const want = await disc('event', 'BetSettled');
  const out: SettledEvent[] = [];
  for (const line of logs) {
    const m = line.match(/^Program data: (.+)$/);
    if (!m) continue;
    let raw: Uint8Array;
    try { raw = Uint8Array.from(atob(m[1]), (c) => c.charCodeAt(0)); } catch { continue; }
    if (raw.length < 8 + 32 + 8 + 16 + 8 + 8 + 32 * 4 + 1 || !want.every((b, i) => raw[i] === b)) continue;
    const g = raw.slice(48, 64);
    const n = g.indexOf(0);
    out.push({
      player: key(raw, 8).toBase58(), nonce: u64(raw, 40), gameId: new TextDecoder().decode(n < 0 ? g : g.slice(0, n)),
      amount: u64(raw, 64), payout: u64(raw, 72), params: raw.slice(80, 112), clientSeed: raw.slice(112, 144),
      serverSeed: raw.slice(144, 176), serverSeedHash: raw.slice(176, 208), verifiedOnChain: raw[208] === 1,
    });
  }
  return out;
}
