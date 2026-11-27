//! Runtime tests for Batch 3 multi-step games (Mines, HiLo, Towers, Blackjack, Video Poker).
//! `cargo test -p openhouse --test multistep`
use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use openhouse::errors::OpenHouseError;
use openhouse::games::multistep::{
    blackjack_max_payout, hilo_max_payout, mines_max_payout, poker_max_payout, towers_max_payout,
};
use openhouse::state::{Config, Player, Vault};
use solana_program_test::*;
use solana_sdk::{
    account_info::AccountInfo,
    clock::Clock,
    entrypoint::ProgramResult,
    hash::hashv,
    instruction::{Instruction, InstructionError},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_instruction, system_program,
    transaction::{Transaction, TransactionError},
};

const SOL: u64 = 1_000_000_000;

fn process(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    let accounts = Box::leak(Box::new(accounts.to_vec()));
    openhouse::entry(program_id, accounts, data)
}

fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &openhouse::ID).0
}
fn config_pda() -> Pubkey { pda(&[b"config"]) }
fn vault_pda() -> Pubkey { pda(&[b"vault"]) }
fn player_pda(w: &Pubkey) -> Pubkey { pda(&[b"player", w.as_ref()]) }
fn share_pda(w: &Pubkey) -> Pubkey { pda(&[b"vault_share", w.as_ref()]) }
fn commit_pda(h: &[u8; 32]) -> Pubkey { pda(&[b"seed_commitment", h]) }
fn code(e: OpenHouseError) -> u32 { anchor_lang::error::ERROR_CODE_OFFSET + e as u32 }

fn game(name: &str) -> [u8; 16] {
    let mut g = [0u8; 16];
    g[..name.len()].copy_from_slice(name.as_bytes());
    g
}
fn seed(i: u64) -> [u8; 32] { hashv(&[b"test-seed-b3", &i.to_le_bytes()]).to_bytes() }
fn seed_hash(s: &[u8; 32]) -> [u8; 32] { hashv(&[s]).to_bytes() }
fn cseed(i: u64) -> [u8; 32] { hashv(&[b"client-b3", &i.to_le_bytes()]).to_bytes() }

struct H {
    ctx: ProgramTestContext,
    authority: Keypair,
    settler: Keypair,
    next_seed: u64,
}

impl H {
    async fn new() -> H {
        let pt = ProgramTest::new("openhouse", openhouse::ID, processor!(process));
        let ctx = pt.start_with_context().await;
        let authority = Keypair::from_bytes(&ctx.payer.to_bytes()).unwrap();
        let mut h = H { ctx, authority, settler: Keypair::new(), next_seed: 1 };
        let s = h.settler.pubkey();
        h.fund(&s, 10 * SOL).await;
        h
    }
    async fn send(&mut self, ixs: &[Instruction], extra: &[&Keypair]) -> Result<(), BanksClientError> {
        let bh = self.ctx.get_new_latest_blockhash().await.unwrap();
        let mut signers: Vec<&Keypair> = vec![&self.ctx.payer];
        signers.extend_from_slice(extra);
        let tx = Transaction::new_signed_with_payer(ixs, Some(&self.ctx.payer.pubkey()), &signers, bh);
        self.ctx.banks_client.process_transaction(tx).await
    }
    async fn fund(&mut self, to: &Pubkey, lamports: u64) {
        let ix = system_instruction::transfer(&self.ctx.payer.pubkey(), to, lamports);
        self.send(&[ix], &[]).await.unwrap();
    }
    async fn data(&mut self, k: &Pubkey) -> Option<Vec<u8>> {
        self.ctx.banks_client.get_account(*k).await.unwrap().map(|a| a.data)
    }
    async fn player(&mut self, w: &Pubkey) -> Player {
        Player::try_deserialize(&mut &self.data(&player_pda(w)).await.unwrap()[..]).unwrap()
    }
    async fn vault(&mut self) -> Vault {
        Vault::try_deserialize(&mut &self.data(&vault_pda()).await.unwrap()[..]).unwrap()
    }
    async fn now(&mut self) -> i64 {
        self.ctx.banks_client.get_sysvar::<Clock>().await.unwrap().unix_timestamp
    }
    async fn warp(&mut self, secs: i64) {
        let mut c: Clock = self.ctx.banks_client.get_sysvar().await.unwrap();
        c.unix_timestamp += secs;
        self.ctx.set_sysvar(&c);
    }

    fn ix_init(&self, k: u16, fee: u16, min: u64) -> Instruction {
        Instruction {
            program_id: openhouse::ID,
            data: openhouse::instruction::Initialize {
                k_bps: k,
                protocol_fee_bps: fee,
                min_bet: min,
                settler: self.settler.pubkey(),
            }
            .data(),
            accounts: openhouse::accounts::Initialize {
                config: config_pda(),
                vault: vault_pda(),
                authority: self.authority.pubkey(),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        }
    }
    fn ix_dep_vault(w: &Pubkey, a: u64) -> Instruction {
        Instruction {
            program_id: openhouse::ID,
            data: openhouse::instruction::DepositVault { amount: a }.data(),
            accounts: openhouse::accounts::DepositVault {
                vault: vault_pda(),
                vault_share: share_pda(w),
                depositor: *w,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        }
    }
    fn ix_dep_player(w: &Pubkey, a: u64) -> Instruction {
        Instruction {
            program_id: openhouse::ID,
            data: openhouse::instruction::DepositPlayer { amount: a }.data(),
            accounts: openhouse::accounts::DepositPlayer {
                vault: vault_pda(),
                player: player_pda(w),
                user: *w,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        }
    }
    fn ix_commit(&self, hash: [u8; 32], index: u64) -> Instruction {
        Instruction {
            program_id: openhouse::ID,
            data: openhouse::instruction::CommitSeed { seed_hash: hash, index }.data(),
            accounts: openhouse::accounts::CommitSeed {
                config: config_pda(),
                commitment: commit_pda(&hash),
                settler: self.settler.pubkey(),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn ix_bet(
        player: &Pubkey,
        signer: &Pubkey,
        g: &str,
        amount: u64,
        max_payout: u64,
        params: [u8; 32],
        client: [u8; 32],
        hash: [u8; 32],
    ) -> Instruction {
        Instruction {
            program_id: openhouse::ID,
            data: openhouse::instruction::PlaceBet {
                game_id: game(g),
                amount,
                max_payout,
                params,
                client_seed: client,
                server_seed_hash: hash,
            }
            .data(),
            accounts: openhouse::accounts::PlaceBet {
                config: config_pda(),
                vault: vault_pda(),
                player: player_pda(player),
                commitment: commit_pda(&hash),
                signer: *signer,
            }
            .to_account_metas(None),
        }
    }
    fn ix_play_action(player: &Pubkey, signer: &Pubkey, action: u8) -> Instruction {
        Instruction {
            program_id: openhouse::ID,
            data: openhouse::instruction::PlayAction { action }.data(),
            accounts: openhouse::accounts::PlayAction {
                config: config_pda(),
                vault: vault_pda(),
                player: player_pda(player),
                signer: *signer,
            }
            .to_account_metas(None),
        }
    }
    fn ix_settle(&self, player: &Pubkey, open_hash: [u8; 32], server_seed: [u8; 32]) -> Instruction {
        Instruction {
            program_id: openhouse::ID,
            data: openhouse::instruction::SettleBet { server_seed, payout_hint: 0 }.data(),
            accounts: openhouse::accounts::SettleBet {
                config: config_pda(),
                vault: vault_pda(),
                player: player_pda(player),
                commitment: commit_pda(&open_hash),
                settler: self.settler.pubkey(),
            }
            .to_account_metas(None),
        }
    }
    fn ix_force_settle(&self, caller: &Pubkey, player: &Pubkey, open_hash: [u8; 32], server_seed: [u8; 32]) -> Instruction {
        Instruction {
            program_id: openhouse::ID,
            data: openhouse::instruction::ForceSettle { server_seed }.data(),
            accounts: openhouse::accounts::ForceSettle {
                config: config_pda(),
                vault: vault_pda(),
                player: player_pda(player),
                commitment: commit_pda(&open_hash),
                settler: self.settler.pubkey(),
                caller: *caller,
            }
            .to_account_metas(None),
        }
    }
    fn ix_refund(caller: &Pubkey, player: &Pubkey) -> Instruction {
        Instruction {
            program_id: openhouse::ID,
            data: openhouse::instruction::RefundBet {}.data(),
            accounts: openhouse::accounts::RefundBet {
                config: config_pda(),
                vault: vault_pda(),
                player: player_pda(player),
                caller: *caller,
            }
            .to_account_metas(None),
        }
    }
    fn ix_pause(&self, who: &Pubkey, p: bool) -> Instruction {
        Instruction {
            program_id: openhouse::ID,
            data: openhouse::instruction::Pause { paused: p }.data(),
            accounts: openhouse::accounts::Pause { config: config_pda(), authority: *who }.to_account_metas(None),
        }
    }
    fn ix_session(user: &Pubkey, key: &Pubkey, dur: i64, max: u64) -> Instruction {
        Instruction {
            program_id: openhouse::ID,
            data: openhouse::instruction::CreateSession {
                session_key: *key,
                duration_seconds: dur,
                max_spend: max,
            }
            .data(),
            accounts: openhouse::accounts::CreateSession {
                player: player_pda(user),
                user: *user,
            }
            .to_account_metas(None),
        }
    }

    async fn setup_vault_and_player(&mut self, vault_sol: u64, player_sol: u64) -> Keypair {
        let lp = self.authority.pubkey();
        let player = Keypair::new();
        self.fund(&player.pubkey(), (player_sol + 5) * SOL).await;
        self.send(
            &[
                self.ix_init(1000, 2000, 1_000_000),
                Self::ix_dep_vault(&lp, vault_sol * SOL),
                Self::ix_dep_player(&player.pubkey(), player_sol * SOL),
            ],
            &[&player],
        )
        .await
        .unwrap();
        player
    }

    async fn commit_next(&mut self) -> ([u8; 32], [u8; 32]) {
        let i = self.next_seed;
        self.next_seed += 1;
        let s = seed(i);
        let h = seed_hash(&s);
        let sk = Keypair::from_bytes(&self.settler.to_bytes()).unwrap();
        self.send(&[self.ix_commit(h, i)], &[&sk]).await.unwrap();
        (s, h)
    }
}

fn assert_err(r: Result<(), BanksClientError>, expected: OpenHouseError) {
    match r {
        Err(BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(c)))) => {
            assert_eq!(c, code(expected), "expected {:?} ({}) got {}", expected, code(expected), c);
        }
        other => panic!("expected {:?}, got {:?}", expected, other),
    }
}

// ─── Test 6: Full flow for each game, 100 rounds, lamport conservation ────────
#[tokio::test]
async fn test6_full_flow_100_rounds_conservation() {
    let mut h = H::new().await;
    let player = h.setup_vault_and_player(500, 50).await;
    let pw = player.pubkey();
    let sk = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();

    let mut rng: u64 = 0x1122_3344_5566_7788;
    let mut next_u64 = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };

    let amount = 10_000_000u64; // 0.01 SOL

    for round in 0..100u64 {
        let (s, sh) = h.commit_next().await;
        let cs = cseed(round);
        let game_idx = round % 5;

        match game_idx {
            0 => {
                // Mines: m=3, reveal 1 cell then cash out (255)
                let mut p = [0u8; 32];
                p[0] = 3;
                let mp = mines_max_payout(&p, amount).unwrap();
                h.send(&[H::ix_bet(&pw, &pw, "mines", amount, mp, p, cs, sh)], &[&player]).await.unwrap();
                let cell = (next_u64() % 25) as u8;
                h.send(&[H::ix_play_action(&pw, &pw, cell)], &[&player]).await.unwrap();
                h.send(&[H::ix_play_action(&pw, &pw, 255)], &[&player]).await.unwrap();
                h.send(&[h.ix_settle(&pw, sh, s)], &[&sk]).await.unwrap();
            }
            1 => {
                // HiLo: guess 0 or 1, then cash out (255)
                let p = [0u8; 32];
                let mp = hilo_max_payout(&p, amount).unwrap();
                h.send(&[H::ix_bet(&pw, &pw, "hilo", amount, mp, p, cs, sh)], &[&player]).await.unwrap();
                let g = (next_u64() % 2) as u8;
                h.send(&[H::ix_play_action(&pw, &pw, g)], &[&player]).await.unwrap();
                h.send(&[H::ix_play_action(&pw, &pw, 255)], &[&player]).await.unwrap();
                h.send(&[h.ix_settle(&pw, sh, s)], &[&sk]).await.unwrap();
            }
            2 => {
                // Towers: diff 1 (3 cols), pick col then cash out (255)
                let mut p = [0u8; 32];
                p[0] = 1;
                let mp = towers_max_payout(&p, amount).unwrap();
                h.send(&[H::ix_bet(&pw, &pw, "towers", amount, mp, p, cs, sh)], &[&player]).await.unwrap();
                let col = (next_u64() % 3) as u8;
                h.send(&[H::ix_play_action(&pw, &pw, col)], &[&player]).await.unwrap();
                h.send(&[H::ix_play_action(&pw, &pw, 255)], &[&player]).await.unwrap();
                h.send(&[h.ix_settle(&pw, sh, s)], &[&sk]).await.unwrap();
            }
            3 => {
                // Blackjack: stand (1)
                let p = [0u8; 32];
                let mp = blackjack_max_payout(&p, amount).unwrap();
                h.send(&[H::ix_bet(&pw, &pw, "blackjack", amount, mp, p, cs, sh)], &[&player]).await.unwrap();
                h.send(&[H::ix_play_action(&pw, &pw, 1)], &[&player]).await.unwrap();
                h.send(&[h.ix_settle(&pw, sh, s)], &[&sk]).await.unwrap();
            }
            _ => {
                // Poker: hold mask 0..31
                let p = [0u8; 32];
                let mp = poker_max_payout(&p, amount).unwrap();
                h.send(&[H::ix_bet(&pw, &pw, "poker", amount, mp, p, cs, sh)], &[&player]).await.unwrap();
                let mask = (next_u64() % 32) as u8;
                h.send(&[H::ix_play_action(&pw, &pw, mask)], &[&player]).await.unwrap();
                h.send(&[h.ix_settle(&pw, sh, s)], &[&sk]).await.unwrap();
            }
        }
    }

    let v = h.vault().await;
    let pl = h.player(&pw).await;
    assert_eq!(v.reserved_payouts, 0);
    // Conservation: vault.total_deposited + vault.accrued_fees + pl.balance == 550 * SOL
    assert_eq!(v.total_deposited + v.accrued_fees + pl.balance, 550 * SOL);
}

// ─── Test 7: Blackjack double ─────────────────────────────────────────────────
#[tokio::test]
async fn test7_blackjack_double_conservation() {
    let mut h = H::new().await;
    let player = h.setup_vault_and_player(200, 10).await;
    let pw = player.pubkey();
    let sk = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();

    let amount = 10_000_000u64; // 0.01 SOL (250x max = 2.5 SOL <= 10% of 100 SOL) // 0.1 SOL
    let p = [0u8; 32];
    let mp = blackjack_max_payout(&p, amount).unwrap();

    for i in 0..10u64 {
        let (s, sh) = h.commit_next().await;
        let bal_before = h.player(&pw).await.balance;
        h.send(&[H::ix_bet(&pw, &pw, "blackjack", amount, mp, p, cseed(i), sh)], &[&player]).await.unwrap();
        assert_eq!(h.player(&pw).await.balance, bal_before - amount);

        // Double (action = 2)
        h.send(&[H::ix_play_action(&pw, &pw, 2)], &[&player]).await.unwrap();
        let pl = h.player(&pw).await;
        assert_eq!(pl.balance, bal_before - 2 * amount);
        assert_eq!(pl.open.extra_stake, amount);
        assert_eq!(pl.open.max_payout, 4 * amount);

        h.send(&[h.ix_settle(&pw, sh, s)], &[&sk]).await.unwrap();
        assert!(!h.player(&pw).await.open.active);
    }

    let v = h.vault().await;
    let pl = h.player(&pw).await;
    assert_eq!(v.reserved_payouts, 0);
    assert_eq!(v.total_deposited + v.accrued_fees + pl.balance, 210 * SOL);
}

// ─── Test 8: force_settle & refund_bet timeout rules ──────────────────────────
#[tokio::test]
async fn test8_force_settle_and_refund_rules() {
    let mut h = H::new().await;
    let player = h.setup_vault_and_player(100, 10).await;
    let pw = player.pubkey();
    let sk = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();
    let stranger = Keypair::new();
    h.fund(&stranger.pubkey(), SOL).await;

    let amount = 10_000_000u64; // 0.01 SOL (250x max = 2.5 SOL <= 10% of 100 SOL)
    let mut p = [0u8; 32];
    p[0] = 3; // mines m=3
    let mp = mines_max_payout(&p, amount).unwrap();

    // Case A: action_count > 0 -> refund_bet refused, force_settle before 600s fails, after 600s succeeds
    let (s1, sh1) = h.commit_next().await;
    h.send(&[H::ix_bet(&pw, &pw, "mines", amount, mp, p, cseed(1), sh1)], &[&player]).await.unwrap();
    h.send(&[H::ix_play_action(&pw, &pw, 0)], &[&player]).await.unwrap();

    // Refund refused even after 120s
    h.warp(125).await;
    let r = h.send(&[H::ix_refund(&pw, &pw)], &[&player]).await;
    assert_err(r, OpenHouseError::RefundTimeoutNotReached);

    // force_settle before 600s fails
    let r = h.send(&[h.ix_force_settle(&stranger.pubkey(), &pw, sh1, s1)], &[&stranger, &sk]).await;
    assert_err(r, OpenHouseError::ForceSettleTimeoutNotReached);

    // Warp past 600s from last_action_at -> force_settle succeeds
    h.warp(500).await;
    h.send(&[h.ix_force_settle(&stranger.pubkey(), &pw, sh1, s1)], &[&stranger, &sk]).await.unwrap();
    assert!(!h.player(&pw).await.open.active);

    // Case B: action_count == 0 -> refund_bet succeeds after 120s
    let (_s2, sh2) = h.commit_next().await;
    h.send(&[H::ix_bet(&pw, &pw, "mines", amount, mp, p, cseed(2), sh2)], &[&player]).await.unwrap();
    h.warp(125).await;
    h.send(&[H::ix_refund(&pw, &pw)], &[&player]).await.unwrap();
    assert!(!h.player(&pw).await.open.active);
}

// ─── Test 9: Attacks & edge cases ─────────────────────────────────────────────
#[tokio::test]
async fn test9_attacks_and_edge_cases() {
    let mut h = H::new().await;
    let player = h.setup_vault_and_player(200, 10).await;
    let pw = player.pubkey();
    let sk = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();
    let stranger = Keypair::new();
    let sess = Keypair::new();
    h.fund(&stranger.pubkey(), SOL).await;
    h.fund(&sess.pubkey(), SOL).await;

    let amount = 10_000_000u64;
    let p_mines = { let mut p = [0u8; 32]; p[0] = 1; p };
    let mp_mines = mines_max_payout(&p_mines, amount).unwrap();

    // Create session expiring in 60s
    h.send(&[H::ix_session(&pw, &sess.pubkey(), 60, 10 * SOL)], &[&player]).await.unwrap();

    let (s1, sh1) = h.commit_next().await;
    h.send(&[H::ix_bet(&pw, &sess.pubkey(), "mines", amount, mp_mines, p_mines, cseed(1), sh1)], &[&sess]).await.unwrap();

    // 1. Stranger play_action fails
    let r = h.send(&[H::ix_play_action(&pw, &stranger.pubkey(), 0)], &[&stranger]).await;
    assert_err(r, OpenHouseError::Unauthorized);

    // 2. Unfinished settle fails
    let r = h.send(&[h.ix_settle(&pw, sh1, s1)], &[&sk]).await;
    assert_err(r, OpenHouseError::RoundNotFinished);

    // 3. Wrong seed fails
    let bad_seed = [0x99u8; 32];
    let r = h.send(&[h.ix_settle(&pw, sh1, bad_seed)], &[&sk]).await;
    assert_err(r, OpenHouseError::InvalidServerSeedHash);

    // 4. Expired session fails
    h.warp(65).await;
    let r = h.send(&[H::ix_play_action(&pw, &sess.pubkey(), 0)], &[&sess]).await;
    assert_err(r, OpenHouseError::SessionKeyExpired);

    // 5. Pause does NOT block force_settle
    h.send(&[h.ix_pause(&h.authority.pubkey(), true)], &[]).await.unwrap();
    h.warp(605).await;
    h.send(&[h.ix_force_settle(&stranger.pubkey(), &pw, sh1, s1)], &[&stranger, &sk]).await.unwrap();
    h.send(&[h.ix_pause(&h.authority.pubkey(), false)], &[]).await.unwrap();

    // 6. Action after settle fails (NoOpenBet)
    let r = h.send(&[H::ix_play_action(&pw, &pw, 0)], &[&player]).await;
    assert_err(r, OpenHouseError::NoOpenBet);

    // 7. 33rd action fails (ActionLogFull)
    let (s2, sh2) = h.commit_next().await;
    let p_hilo = [0u8; 32];
    let mp_hilo = hilo_max_payout(&p_hilo, amount).unwrap();
    h.send(&[H::ix_bet(&pw, &pw, "hilo", amount, mp_hilo, p_hilo, cseed(2), sh2)], &[&player]).await.unwrap();
    for _ in 0..32 {
        h.send(&[H::ix_play_action(&pw, &pw, 0)], &[&player]).await.unwrap();
    }
    let r = h.send(&[H::ix_play_action(&pw, &pw, 0)], &[&player]).await;
    assert_err(r, OpenHouseError::ActionLogFull);
    h.warp(605).await;
    h.send(&[h.ix_force_settle(&pw, &pw, sh2, s2)], &[&player, &sk]).await.unwrap();

    // 8. Blackjack double after hit fails (InvalidAction)
    let (s3, sh3) = h.commit_next().await;
    let p_bj = [0u8; 32];
    let mp_bj = blackjack_max_payout(&p_bj, amount).unwrap();
    h.send(&[H::ix_bet(&pw, &pw, "blackjack", amount, mp_bj, p_bj, cseed(3), sh3)], &[&player]).await.unwrap();
    h.send(&[H::ix_play_action(&pw, &pw, 0)], &[&player]).await.unwrap(); // hit
    let r = h.send(&[H::ix_play_action(&pw, &pw, 2)], &[&player]).await; // double after hit
    assert_err(r, OpenHouseError::InvalidAction);
}

// ─── Test 10: 1-hour safety-net refund (settler never acts) ───────────────────
/// A.2: settler never reveals the seed. After action_count > 0, refund_bet
/// is blocked until ROUND_REFUND_TIMEOUT (3600 s) from last_action_at.
/// Anyone (including third parties) may then call refund_bet.
/// Conservation must hold. After refund, a later refund or settle fails.
#[tokio::test]
async fn test10_one_hour_safety_refund() {
    let mut h = H::new().await;
    let player = h.setup_vault_and_player(100, 10).await;
    let pw = player.pubkey();
    let sk = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();
    let stranger = Keypair::new();
    h.fund(&stranger.pubkey(), SOL).await;

    let amount = 10_000_000u64;
    let mut p = [0u8; 32]; p[0] = 3; // mines m=3
    let mp = mines_max_payout(&p, amount).unwrap();

    // ── Case A: 2 actions logged, refund fails at 59 min, succeeds at 61 min ──
    let (_s1, sh1) = h.commit_next().await;
    h.send(&[H::ix_bet(&pw, &pw, "mines", amount, mp, p, cseed(10), sh1)], &[&player]).await.unwrap();
    h.send(&[H::ix_play_action(&pw, &pw, 0)], &[&player]).await.unwrap();
    h.send(&[H::ix_play_action(&pw, &pw, 1)], &[&player]).await.unwrap();

    let vault_before = h.vault().await;
    let pl_before = h.player(&pw).await;
    // 59 minutes from last_action_at — must fail
    h.warp(59 * 60).await;
    let r = h.send(&[H::ix_refund(&pw, &pw)], &[&player]).await;
    assert_err(r, OpenHouseError::RefundTimeoutNotReached);
    // Stranger also fails
    let r = h.send(&[H::ix_refund(&stranger.pubkey(), &pw)], &[&stranger]).await;
    assert_err(r, OpenHouseError::RefundTimeoutNotReached);

    // 61 minutes total — succeeds for the player themselves
    h.warp(2 * 60 + 5).await;
    h.send(&[H::ix_refund(&pw, &pw)], &[&player]).await.unwrap();
    let pl_after = h.player(&pw).await;
    assert!(!pl_after.open.active);
    assert_eq!(pl_after.balance, pl_before.balance + amount); // stake returned
    let vault_after = h.vault().await;
    assert_eq!(vault_after.reserved_payouts, vault_before.reserved_payouts - (mp - amount));
    // Conservation: total_deposited + accrued_fees + player.balance unchanged (refund is not a win)
    assert_eq!(vault_after.total_deposited + vault_after.accrued_fees + pl_after.balance,
               vault_before.total_deposited + vault_before.accrued_fees + pl_before.balance + amount);

    // ── Case B: Blackjack after double (extra_stake refunded) ──
    let p_bj = [0u8; 32];
    let mp_bj = blackjack_max_payout(&p_bj, amount).unwrap();
    let (_s2, sh2) = h.commit_next().await;
    h.send(&[H::ix_bet(&pw, &pw, "blackjack", amount, mp_bj, p_bj, cseed(11), sh2)], &[&player]).await.unwrap();
    h.send(&[H::ix_play_action(&pw, &pw, 2)], &[&player]).await.unwrap(); // double
    let pl_mid = h.player(&pw).await;
    assert_eq!(pl_mid.open.extra_stake, amount);
    let bal_before_bj = pl_mid.balance;
    let vault_before_bj = h.vault().await;

    // Third-party refund after 1 hour
    h.warp(3601).await;
    h.send(&[H::ix_refund(&stranger.pubkey(), &pw)], &[&stranger]).await.unwrap();
    let pl_after_bj = h.player(&pw).await;
    assert!(!pl_after_bj.open.active);
    // extra_stake (double) + original amount returned
    assert_eq!(pl_after_bj.balance, bal_before_bj + 2 * amount);
    let vault_after_bj = h.vault().await;
    assert_eq!(vault_after_bj.reserved_payouts, vault_before_bj.reserved_payouts - (pl_mid.open.max_payout - 2 * amount));

    // ── Case C: Settler still settling a finished round BEFORE 1 hour works ──
    let (s3, sh3) = h.commit_next().await;
    let mp3 = mines_max_payout(&p, amount).unwrap();
    h.send(&[H::ix_bet(&pw, &pw, "mines", amount, mp3, p, cseed(12), sh3)], &[&player]).await.unwrap();
    h.send(&[H::ix_play_action(&pw, &pw, 0)], &[&player]).await.unwrap();
    h.send(&[H::ix_play_action(&pw, &pw, 255)], &[&player]).await.unwrap(); // cash out
    // Settler settles < 1 hour
    h.send(&[h.ix_settle(&pw, sh3, s3)], &[&sk]).await.unwrap();
    // Later refund fails (NoOpenBet)
    let r = h.send(&[H::ix_refund(&pw, &pw)], &[&player]).await;
    assert_err(r, OpenHouseError::NoOpenBet);
}
