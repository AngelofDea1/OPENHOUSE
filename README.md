# OpenHouse

OpenHouse is a provably fair, non-custodial Solana casino and public bankroll vault. Every bet is escrowed on-chain, settled against a pre-committed server seed and a player-chosen client seed, and verified deterministically by both the Anchor program and client-side replay.

- **Live Demo:** https://openhouse-ten.vercel.app
- **Demo video:** TODO add link

---

## Deployed Addresses (Solana Devnet)

| Component | Address | Explorer |
| --- | --- | --- |
| Program | `7LECoyuUbynh66Frvi5yrbpQ1yYRc94epDvdY8p4oyvK` | [View](https://explorer.solana.com/address/7LECoyuUbynh66Frvi5yrbpQ1yYRc94epDvdY8p4oyvK?cluster=devnet) |
| Config PDA | `FGPwyR1hQuAcwv5jQc1DQBqaLSjXqwCo3aCq83zPFGrL` | [View](https://explorer.solana.com/address/FGPwyR1hQuAcwv5jQc1DQBqaLSjXqwCo3aCq83zPFGrL?cluster=devnet) |
| Vault PDA | `CA2YwZaB3Ejvx8tuCfayGPctnRnJFGLScBKLTxGpWxDM` | [View](https://explorer.solana.com/address/CA2YwZaB3Ejvx8tuCfayGPctnRnJFGLScBKLTxGpWxDM?cluster=devnet) |
| Authority / Settler | `89t4acyUQy5YBqr11jHUav7odtu3oKkX12qZNaH2WPhv` | [View](https://explorer.solana.com/address/89t4acyUQy5YBqr11jHUav7odtu3oKkX12qZNaH2WPhv?cluster=devnet) |

---

## Live On-Chain Games (20)

All 20 games are live on-chain on devnet with deterministic outcomes derived from `SHA-256(server_seed || client_seed || nonce)`. See [docs/GAMES_SPEC.md](docs/GAMES_SPEC.md) for full payout tables and edge math.

### Single-Step Games
1. **Dice** — Roll under/over target (2%–98%): `99%` RTP (`1%` house edge).
2. **Coin Flip** — Heads or Tails (`1.98x` payout): `99%` RTP (`1%` house edge).
3. **Limbo** — Target multiplier `1.01x`–`1000x`: `99%` RTP (`1%` house edge).
4. **Crash** — Auto cash-out multiplier `1.01x`–`1000x`: `99%` RTP (`1%` house edge).
5. **Wheel** — 3 risk tiers (Low / Med / High): `~97%–99%` RTP.
6. **Plinko** — 8–16 rows, 3 risk tiers: `~97%–99%` RTP.
7. **Keno** — Pick 1–10 numbers out of 40: `~96%–99%` RTP.
8. **Roulette** — European single-zero wheel: `97.30%` RTP (`2.70%` house edge).
9. **Hot Reels** — 5-reel slot machine: `~96%` RTP.
10. **Slots 777** — Classic 3-reel slot: `~96%` RTP.
11. **Vault Break** — Safe-cracking bonus slot: `~96%` RTP.
12. **Scratch** — Instant scratch card: `~96%` RTP.
13. **Sic Bo** — 3-dice table game (Small/Big, Triple, Any Triple, Sum): `97.22%` RTP on Small/Big.
14. **Dragon Tiger** — 2-card draw (Dragon, Tiger, Tie): `~96.27%` RTP.
15. **Baccarat** — Player / Banker / Tie: `98.94%` (Banker), `98.76%` (Player).

### Multi-Step Interactive Games
16. **Mines** — 5x5 grid with 1–24 mines, step-by-step reveal & cashout: `99%` RTP (`1%` house edge).
17. **HiLo** — Predict Higher/Lower/Skip on a 52-card deck: `99%` RTP (`1%` house edge).
18. **Towers** — Climb 8-row tower (Easy/Med/Hard): `~98%–99%` RTP.
19. **Blackjack** — Hit, Stand, Double against dealer: `~99.5%` RTP (basic strategy).
20. **Video Poker** — Jacks or Better (5-card draw + hold): `~99.54%` RTP (optimal play).

---

## Architecture

```text
+-------------------+       +------------------------+       +----------------------+
|   index.html      | <---> |   services/settler     | <---> |  programs/openhouse  |
|  (Static Site +   | HTTP  |  (Express / Vercel     |  RPC  |  (Anchor Program on  |
|   vendor/bundle)  |       |   Serverless /api)     |       |   Solana Devnet)     |
+-------------------+       +------------------------+       +----------------------+
         |                              |                              ^
         +------------------------------+------------------------------+
                        @solana/web3.js + packages/core
                   (PDAs, instruction builders, fair.ts, replay)
```

- **`programs/openhouse/`**: Anchor smart contract managing the bankroll vault, player accounts, session keys, seed commitments, single-step settlement, and multi-step `play_action` / `force_settle` / `refund_bet`.
- **`packages/core/`**: Shared TypeScript library for PDA derivation, instruction encoding, account decoding, provably fair RNG, and deterministic multi-step replay. Bundled into `vendor/openhouse-core.js` for the browser.
- **`services/settler/`**: Node/Express API (deployed as `api/index.js` on Vercel) that commits server seed hashes, settles bets, executes multi-step actions, and provides `/api/admin/sweep` and `/api/chain/info`.
- **`index.html`**: Single-file frontend with wallet adapter support, live on-chain mode, practice mode, and client-side verification.

---

## Provably Fair

1. **Commit**: Before a bet is placed, the settler derives a server seed from `SEED_MASTER_SECRET` and commits `SHA-256(server_seed)` on-chain in a `SeedCommitment` PDA.
2. **Client Seed & Bet**: The player supplies a 32-byte `client_seed` and places a bet (`place_bet`), locking the stake and reserving `max_payout` from the Vault PDA.
3. **Reveal & On-Chain Verification**: The settler reveals `server_seed` in `settle_bet` (or `force_settle`). The Anchor program verifies `SHA-256(server_seed) == committed_hash`, computes the outcome deterministically on-chain, and transfers payout.
4. **Multi-Step Replay**: For Mines, HiLo, Towers, Blackjack, and Poker, each player move is recorded on-chain via `play_action`. At settlement, the program and `packages/core` replay the entire action log against `server_seed` + `client_seed` + `nonce`.

---

## Trust Model & Known Limitations

- **Settler Liveness**: The settler holds the pre-image `server_seed` and must submit `settle_bet`. A malicious or offline settler can withhold settlement, though it cannot alter the outcome once committed.
- **Mitigation**: Players can call `refund_bet` after the on-chain timeout window to reclaim their locked stake.
- **Future**: Moving to verifiable random functions (VRF) or bonded permissionless settlers is planned for mainnet.

---

## Solana SDK & Tooling

- **Anchor** (`anchor-lang`, `solana-program-test`) for the on-chain program and Rust integration tests.
- **`@solana/web3.js`** for RPC, transaction building, and instruction encoding.
- **PDAs** for Config, Vault, Player, and SeedCommitment accounts.
- **Wallet Adapters** (Phantom, Solflare, Backpack) + **Session Keys** for gasless/popup-free multi-step play.
- **Program Events** (`BetSettled`) for on-chain payout verification.

---

## Setup & Development

### Prerequisites
- Node.js 20+
- Rust & Cargo
- Solana CLI & Anchor CLI (`0.30+`)

### 1. Clone & Install
```bash
git clone https://github.com/AngelofDea1/OPENHOUSE.git
cd OPENHOUSE
npm install
cd packages/core && npm install && cd ../..
cd services/settler && npm install && cd ../..
```

### 2. Build Program & Bundles
```bash
anchor build --no-idl
cd packages/core && npm run build:browser && cd ../..
cd services/settler && npm run build && cd ../..
```

### 3. Run Tests
```bash
cargo test
cd packages/core && npm test && cd ../..
cd services/settler && npm test && cd ../..
```

### 4. Deploy & Initialize (Devnet)
```bash
solana config set --url devnet
anchor deploy --provider.cluster devnet
cd packages/core && npx tsx scripts/init-cluster.mjs
```

### 5. Environment Variables
Copy `.env.example` to `.env` and fill in your values:
```bash
cp .env.example .env
```
Required variables: `PROGRAM_ID`, `SOLANA_RPC_URL`, `SETTLER_SECRET_KEY`, `SEED_MASTER_SECRET`, `ALLOWED_ORIGINS`, `ADMIN_TOKEN`.

### 6. Run Settler & Site
```bash
# Terminal 1: Settler API
cd services/settler && npm start

# Terminal 2: Static site
npx serve .
```

---

## Security & Documentation

- [SECURITY.md](SECURITY.md) — Vulnerability reporting and scope
- [MAINNET.md](MAINNET.md) — Mainnet readiness checklist and safety gates
- [docs/GAMES_SPEC.md](docs/GAMES_SPEC.md) — Exact game math, payout tables, and edge specs
- [LICENSE](LICENSE) — MIT License
