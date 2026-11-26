# OpenHouse

A Solana casino with a public on chain vault, about 1% edge, no bonuses, and fairness the program checks itself.

- `programs/openhouse` Anchor program (Rust) with unit and runtime tests
- `packages/core` shared TypeScript client and game maths, also bundled for the browser
- `services/settler` stateless settler (commits seeds, settles bets), deployed as the `/api` function
- `index.html`, `vendor/` the site
- `docs/GAMES_SPEC.md` exact rules for every game, `MAINNET.md` mainnet gates and runbook, `.env.example` all settings

Live on chain today (devnet): Dice, Flip, Limbo, Wheel, Plinko, Keno, Roulette, Hot Reels, Slots 777, Vault Break, Scratch, Sic Bo, Dragon Tiger, Baccarat. Mines, HiLo, Towers, Blackjack and Poker are practice only until ported.
Tests: `cargo test`, then `cd packages/core && npm install && npm test`.

Health check of a deployment: `cd packages/core && CLUSTER=devnet PROGRAM_ID=<id> SETTLER_URL=<site>/api npm run preflight`

## Scripts

| Script | Where | Command |
|---|---|---|
| Initialize cluster | `packages/core` | `CLUSTER=devnet RPC_URL=... PROGRAM_ID=... AUTHORITY_KEYPAIR=... SETTLER_PUBKEY=... npm run init` |
| Preflight check | `packages/core` | `CLUSTER=devnet PROGRAM_ID=... SETTLER_URL=... npm run preflight` |
| Solvency monitor | `packages/core` | `CLUSTER=devnet RPC_URL=... PROGRAM_ID=... npm run monitor` |
| Headless smoke test | repo root | `SMOKE_URL=https://openhouse-ten.vercel.app node scripts/smoke.mjs` |

## Environment Variables

| Variable | Used by | Description |
|---|---|---|
| `CLUSTER` | settler, core | `devnet` or `mainnet-beta` |
| `SOLANA_RPC_URL` / `RPC_URL` | settler, core | Solana JSON-RPC endpoint |
| `PROGRAM_ID` | settler, core | Deployed OpenHouse program ID |
| `SETTLER_URL` | core | Settler base URL (e.g. `https://openhouse-ten.vercel.app/api`) |
| `SETTLER_SECRET_KEY` | settler | Base58/JSON keypair for settler |
| `SEED_MASTER_SECRET` | settler | HMAC master secret for seed commitments |
| `ADMIN_TOKEN` | settler | Bearer token for `/api/admin/sweep` |
| `CRON_SECRET` | settler | Vercel cron Bearer token for `/api/admin/sweep` |
| `WEBHOOK_URL` | monitor | Optional Discord/Slack webhook URL on failure |
