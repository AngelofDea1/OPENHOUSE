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
