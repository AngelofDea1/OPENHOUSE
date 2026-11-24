# Mainnet structure

Devnet stays the live network until every gate in section 1 is done. Nothing in this repo sends real money by default:
the site config has `mainnetEnabled:false`, and the settler refuses to sign on `mainnet-beta` without `MAINNET_CONFIRM=yes-real-money`.

## 0. What is already wired

| Piece | Devnet today | Mainnet switch |
|---|---|---|
| Site `index.html` | `window.OH_CONFIG` (top of file): cluster, rpc, programId, settler, mainnetEnabled | change those five values, nothing else |
| Wrong network guard | wallet actions check the RPC genesis hash before sending | same |
| Settler | `CLUSTER`, `SOLANA_RPC_URL`, `PROGRAM_ID` | plus `MAINNET_CONFIRM=yes-real-money` |
| Setup | `npm run init` in `packages/core` | needs `I_UNDERSTAND_REAL_MONEY=yes`, authority and settler must differ |
| Health | `npm run preflight` | same, turns warnings into failures where it matters |

## 1. Gates. Do not deploy to mainnet before all are true

1. Games: every game on the site is verified on chain, or clearly marked practice only. No game is both live and unverified.
2. Withholding fix: settler cannot pick which bets to settle (VRF, or several bonded settlers). Until then a settler can stall a losing-for-house bet and force the 120 second refund.
3. Independent audit of `programs/openhouse` finished, findings fixed, report published.
4. Fresh program keypair, generated offline. Never reuse the devnet ID. Put the new ID in `declare_id!` (programs/openhouse/src/lib.rs), `Anchor.toml`, `OH_CONFIG.programId`, `PROGRAM_ID` env, then rebuild.
5. Keys split: authority is a multisig (Squads), settler is a hot key with only gas SOL, fee wallet is separate. Upgrade authority is the multisig, then revoked after the audit period.
6. Legal: licence for your target regions, geo-blocking live, terms and responsible play pages reviewed by a lawyer, age check. This is not legal advice, get counsel.
7. Load test and solvency monitor running on devnet for at least two weeks with no unexplained drift.
8. Private RPC with a paid plan, server side only. Public `api.mainnet-beta.solana.com` is rate limited and not for production.
9. A written incident plan: who can pause, how to refund, how to announce.

## 2. Build and deploy (when gates pass)

```
solana-keygen new -o program-keypair.json        # offline machine, back it up twice
solana address -k program-keypair.json            # this is the new PROGRAM_ID
# set declare_id!, rebuild reproducibly
anchor build --verifiable                         # or: solana-verify build
solana program deploy target/deploy/openhouse.so --program-id program-keypair.json \
  --upgrade-authority <MULTISIG_PUBKEY> --url <PRIVATE_MAINNET_RPC>
solana-verify verify-from-repo ...                # publishes that the bytecode matches this repo
```

Then, from `packages/core`:

```
CLUSTER=mainnet-beta RPC_URL=<private rpc> PROGRAM_ID=<new id> \
AUTHORITY_KEYPAIR=<keypair that signs initialize> SETTLER_PUBKEY=<settler pubkey> \
K_BPS=100 FEE_BPS=2000 I_UNDERSTAND_REAL_MONEY=yes npm run init
```

Move authority to the multisig straight away with `propose_authority`, then `accept_authority` from the multisig.
Seed the vault small (for example 1 to 5 SOL), keep betting paused (`pause`) until preflight is clean, then unpause.

## 3. Switch the site and settler

Vercel env: `CLUSTER=mainnet-beta`, `SOLANA_RPC_URL`, `PROGRAM_ID`, `SETTLER_SECRET_KEY` (new key, funded 0.5 SOL), `SEED_MASTER_SECRET` (new), `ADMIN_TOKEN` (new), `MAINNET_CONFIRM=yes-real-money`.
In `index.html` set `cluster:"mainnet-beta"`, `rpc` (the browser needs an RPC too: use a separate rate limited key that is safe to be public, or proxy it), `programId`, and last of all `mainnetEnabled:true`.
Remove the "devnet preview" wording on the FAQ and home pages in the same commit.

## 4. First day on mainnet

- `npm run preflight` with `SETTLER_URL` set: zero FAIL.
- Cap the first week: small vault, k_bps stays 100, low max bet comes from the vault automatically.
- Watch: settler gas balance, reserved vs total deposited, any bet open longer than 2 minutes, sweep job output.
- Pause command ready in the multisig before the first public bet.

## 5. Rollback

`pause` stops new bets, open bets can still settle or refund after 120 seconds, depositors can withdraw free liquidity. Program upgrades go through the multisig only.
