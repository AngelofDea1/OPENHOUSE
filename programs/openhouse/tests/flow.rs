//! Runs the real program inside a Solana runtime (solana-program-test, native processor).
//! `cargo test --test flow`
use anchor_lang::Space;
use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use openhouse::errors::OpenHouseError;
use openhouse::games::{self, fair, Kind};
use openhouse::state::{Config, Player, Vault};
use solana_program_test::*;
use solana_sdk::{
    account_info::AccountInfo, clock::Clock, entrypoint::ProgramResult, hash::hashv, instruction::{Instruction, InstructionError},
    pubkey::Pubkey, signature::Keypair, signer::Signer, system_instruction, system_program, transaction::{Transaction, TransactionError},
};

const SOL: u64 = 1_000_000_000;

fn process(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    let accounts = Box::leak(Box::new(accounts.to_vec()));
    openhouse::entry(program_id, accounts, data)
}

fn pda(seeds: &[&[u8]]) -> Pubkey { Pubkey::find_program_address(seeds, &openhouse::ID).0 }
fn config_pda() -> Pubkey { pda(&[b"config"]) }
fn vault_pda() -> Pubkey { pda(&[b"vault"]) }
fn player_pda(w: &Pubkey) -> Pubkey { pda(&[b"player", w.as_ref()]) }
fn share_pda(w: &Pubkey) -> Pubkey { pda(&[b"vault_share", w.as_ref()]) }
fn commit_pda(h: &[u8; 32]) -> Pubkey { pda(&[b"seed_commitment", h]) }
fn code(e: OpenHouseError) -> u32 { anchor_lang::error::ERROR_CODE_OFFSET + e as u32 }

fn game(name: &str) -> [u8; 16] { let mut g = [0u8; 16]; g[..name.len()].copy_from_slice(name.as_bytes()); g }
fn dice(under: bool, t: u16) -> [u8; 32] { let mut p = [0u8; 32]; p[0] = under as u8; p[1..3].copy_from_slice(&t.to_le_bytes()); p }
fn flip(tails: bool) -> [u8; 32] { let mut p = [0u8; 32]; p[0] = tails as u8; p }
fn limbo(x100: u32) -> [u8; 32] { let mut p = [0u8; 32]; p[..4].copy_from_slice(&x100.to_le_bytes()); p }
fn seed(i: u64) -> [u8; 32] { hashv(&[b"test-seed", &i.to_le_bytes()]).to_bytes() }
fn seed_hash(s: &[u8; 32]) -> [u8; 32] { hashv(&[s]).to_bytes() }
fn cseed(i: u64) -> [u8; 32] { hashv(&[b"client", &i.to_le_bytes()]).to_bytes() }

struct H { ctx: ProgramTestContext, authority: Keypair, settler: Keypair, next_seed: u64 }

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
    async fn lamports(&mut self, k: &Pubkey) -> u64 { self.ctx.banks_client.get_balance(*k).await.unwrap() }
    async fn data(&mut self, k: &Pubkey) -> Option<Vec<u8>> { self.ctx.banks_client.get_account(*k).await.unwrap().map(|a| a.data) }
    async fn player(&mut self, w: &Pubkey) -> Player { Player::try_deserialize(&mut &self.data(&player_pda(w)).await.unwrap()[..]).unwrap() }
    async fn vault(&mut self) -> Vault { Vault::try_deserialize(&mut &self.data(&vault_pda()).await.unwrap()[..]).unwrap() }
    async fn config(&mut self) -> Config { Config::try_deserialize(&mut &self.data(&config_pda()).await.unwrap()[..]).unwrap() }
    async fn set_time(&mut self, ts: i64) {
        let mut c: Clock = self.ctx.banks_client.get_sysvar().await.unwrap();
        c.unix_timestamp = ts;
        self.ctx.set_sysvar(&c);
    }
    async fn now(&mut self) -> i64 { self.ctx.banks_client.get_sysvar::<Clock>().await.unwrap().unix_timestamp }
    async fn warp(&mut self, secs: i64) { let n = self.now().await; self.set_time(n + secs).await }

    // ---- instruction builders (anchor generated, the same ones the TypeScript client is checked against) ----
    fn ix_init(&self, k: u16, fee: u16, min: u64) -> Instruction {
        Instruction { program_id: openhouse::ID, data: openhouse::instruction::Initialize { k_bps: k, protocol_fee_bps: fee, min_bet: min, settler: self.settler.pubkey() }.data(),
            accounts: openhouse::accounts::Initialize { config: config_pda(), vault: vault_pda(), authority: self.authority.pubkey(), system_program: system_program::ID }.to_account_metas(None) }
    }
    fn ix_dep_vault(w: &Pubkey, a: u64) -> Instruction {
        Instruction { program_id: openhouse::ID, data: openhouse::instruction::DepositVault { amount: a }.data(),
            accounts: openhouse::accounts::DepositVault { vault: vault_pda(), vault_share: share_pda(w), depositor: *w, system_program: system_program::ID }.to_account_metas(None) }
    }
    fn ix_wd_vault(w: &Pubkey, shares: u64) -> Instruction {
        Instruction { program_id: openhouse::ID, data: openhouse::instruction::WithdrawVault { shares }.data(),
            accounts: openhouse::accounts::WithdrawVault { vault: vault_pda(), vault_share: share_pda(w), depositor: *w }.to_account_metas(None) }
    }
    fn ix_dep_player(w: &Pubkey, a: u64) -> Instruction {
        Instruction { program_id: openhouse::ID, data: openhouse::instruction::DepositPlayer { amount: a }.data(),
            accounts: openhouse::accounts::DepositPlayer { vault: vault_pda(), player: player_pda(w), user: *w, system_program: system_program::ID }.to_account_metas(None) }
    }
    fn ix_wd_player(w: &Pubkey, a: u64) -> Instruction {
        Instruction { program_id: openhouse::ID, data: openhouse::instruction::WithdrawPlayer { amount: a }.data(),
            accounts: openhouse::accounts::WithdrawPlayer { vault: vault_pda(), player: player_pda(w), user: *w }.to_account_metas(None) }
    }
    fn ix_commit(&self, hash: [u8; 32], index: u64) -> Instruction {
        Instruction { program_id: openhouse::ID, data: openhouse::instruction::CommitSeed { seed_hash: hash, index }.data(),
            accounts: openhouse::accounts::CommitSeed { config: config_pda(), commitment: commit_pda(&hash), settler: self.settler.pubkey(), system_program: system_program::ID }.to_account_metas(None) }
    }
    #[allow(clippy::too_many_arguments)]
    fn ix_bet(player: &Pubkey, signer: &Pubkey, g: &str, amount: u64, max_payout: u64, params: [u8; 32], client: [u8; 32], hash: [u8; 32]) -> Instruction {
        Instruction { program_id: openhouse::ID,
            data: openhouse::instruction::PlaceBet { game_id: game(g), amount, max_payout, params, client_seed: client, server_seed_hash: hash }.data(),
            accounts: openhouse::accounts::PlaceBet { config: config_pda(), vault: vault_pda(), player: player_pda(player), commitment: commit_pda(&hash), signer: *signer }.to_account_metas(None) }
    }
    fn ix_settle(&self, player: &Pubkey, open_hash: [u8; 32], server_seed: [u8; 32], hint: u64, signer: &Pubkey) -> Instruction {
        Instruction { program_id: openhouse::ID, data: openhouse::instruction::SettleBet { server_seed, payout_hint: hint }.data(),
            accounts: openhouse::accounts::SettleBet { config: config_pda(), vault: vault_pda(), player: player_pda(player), commitment: commit_pda(&open_hash), settler: *signer }.to_account_metas(None) }
    }
    fn ix_refund(caller: &Pubkey, player: &Pubkey) -> Instruction {
        Instruction { program_id: openhouse::ID, data: openhouse::instruction::RefundBet {}.data(),
            accounts: openhouse::accounts::RefundBet { config: config_pda(), vault: vault_pda(), player: player_pda(player), caller: *caller }.to_account_metas(None) }
    }
    fn ix_pause(&self, who: &Pubkey, p: bool) -> Instruction {
        Instruction { program_id: openhouse::ID, data: openhouse::instruction::Pause { paused: p }.data(),
            accounts: openhouse::accounts::Pause { config: config_pda(), authority: *who }.to_account_metas(None) }
    }
    fn ix_session(user: &Pubkey, key: &Pubkey, dur: i64, max: u64) -> Instruction {
        Instruction { program_id: openhouse::ID, data: openhouse::instruction::CreateSession { session_key: *key, duration_seconds: dur, max_spend: max }.data(),
            accounts: openhouse::accounts::CreateSession { player: player_pda(user), user: *user }.to_account_metas(None) }
    }
    fn ix_limits(user: &Pubkey, v: u64) -> Instruction {
        Instruction { program_id: openhouse::ID, data: openhouse::instruction::SetLimits { new_limit: v }.data(),
            accounts: openhouse::accounts::SetLimits { player: player_pda(user), user: *user }.to_account_metas(None) }
    }
    fn ix_exclude(user: &Pubkey, days: u16) -> Instruction {
        Instruction { program_id: openhouse::ID, data: openhouse::instruction::SelfExclude { days }.data(),
            accounts: openhouse::accounts::SelfExclude { player: player_pda(user), user: *user }.to_account_metas(None) }
    }
    fn ix_fees(&self, fee_wallet: &Pubkey, a: u64, who: &Pubkey) -> Instruction {
        Instruction { program_id: openhouse::ID, data: openhouse::instruction::WithdrawFees { amount: a }.data(),
            accounts: openhouse::accounts::WithdrawFees { config: config_pda(), vault: vault_pda(), fee_wallet: *fee_wallet, authority: *who }.to_account_metas(None) }
    }

    /// Commit a fresh seed, place the bet, return what the settler needs.
    async fn open_bet(&mut self, player: &Keypair, g: &str, amount: u64, params: [u8; 32]) -> Result<(u64, [u8; 32], [u8; 32], [u8; 32], u64), BanksClientError> {
        let i = self.next_seed; self.next_seed += 1;
        let s = seed(i); let h = seed_hash(&s); let c = cseed(i);
        let sc = self.ix_commit(h, i);
        let settler = Keypair::from_bytes(&self.settler.to_bytes()).unwrap();
        self.send(&[sc], &[&settler]).await?;
        let max = match games::verified_kind(&game(g)) { Some(k) => games::max_payout(k, &params, amount).unwrap(), None => amount * 2 };
        let ix = H::ix_bet(&player.pubkey(), &player.pubkey(), g, amount, max, params, c, h);
        self.send(&[ix], &[player]).await?;
        Ok((i, s, h, c, max))
    }
    async fn settle(&mut self, player: &Pubkey, h: [u8; 32], s: [u8; 32]) -> Result<(), BanksClientError> {
        let settler = Keypair::from_bytes(&self.settler.to_bytes()).unwrap();
        let ix = self.ix_settle(player, h, s, 0, &settler.pubkey());
        self.send(&[ix], &[&settler]).await
    }
}

fn expect(r: Result<(), BanksClientError>, e: OpenHouseError) {
    match r {
        Err(BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(c)))) => assert_eq!(c, code(e), "wrong error code, wanted {:?}", e as u32),
        Err(BanksClientError::SimulationError { err: TransactionError::InstructionError(_, InstructionError::Custom(c)), .. }) => assert_eq!(c, code(e)),
        other => panic!("expected {:?}, got {:?}", e as u32, other),
    }
}
fn expect_fail(r: Result<(), BanksClientError>) { assert!(r.is_err(), "expected the transaction to fail"); }

async fn setup(h: &mut H, vault_sol: u64) {
    let ix = h.ix_init(100, 2000, 1_000_000);
    let a = Keypair::from_bytes(&h.authority.to_bytes()).unwrap();
    h.send(&[ix], &[&a]).await.unwrap();
    let lp = Keypair::new();
    h.fund(&lp.pubkey(), vault_sol + SOL).await;
    h.send(&[H::ix_dep_vault(&lp.pubkey(), vault_sol)], &[&lp]).await.unwrap();
}
async fn new_player(h: &mut H, sol: u64) -> Keypair {
    let p = Keypair::new();
    h.fund(&p.pubkey(), sol + SOL / 2).await;
    h.send(&[H::ix_dep_player(&p.pubkey(), sol)], &[&p]).await.unwrap();
    p
}

/// vault lamports must always equal rent + depositor value + fees + every player balance + open stakes
async fn assert_conserved(h: &mut H, players: &[&Pubkey]) {
    let rent = h.ctx.banks_client.get_rent().await.unwrap().minimum_balance(8 + Vault::INIT_SPACE);
    let v = h.vault().await;
    let mut sum = rent + v.total_deposited + v.accrued_fees;
    for p in players { let pl = h.player(p).await; sum += pl.balance + if pl.open.active { pl.open.amount } else { 0 }; }
    let have = h.lamports(&vault_pda()).await;
    assert_eq!(have, sum, "vault lamports out of balance");
}

#[tokio::test]
async fn full_flow_conserves_money_and_matches_the_math() {
    let mut h = H::new().await;
    setup(&mut h, 100 * SOL).await;
    let p = new_player(&mut h, 20 * SOL).await;
    let w = p.pubkey();
    let initial = 100 * SOL;
    let settler_before = h.lamports(&h.settler.pubkey()).await;
    let mut wins = 0u32;
    for n in 0..240u64 {
        let amount = 10_000_000 + (n % 7) * 5_000_000;
        let (g, params) = match n % 3 { 0 => ("dice", dice(n % 2 == 0, 3000 + (n % 40) as u16 * 100)), 1 => ("flip", flip(n % 4 == 1)), _ => ("limbo", limbo(150 + (n % 9) as u32 * 50)) };
        let bal0 = h.player(&w).await.balance;
        let (_i, s, hash, c, max) = h.open_bet(&p, g, amount, params).await.unwrap();
        assert_conserved(&mut h, &[&w]).await;
        h.settle(&w, hash, s).await.unwrap();
        // predict the result with the same code the program uses
        let nonce = n; // player nonce counts bets
        let u = fair::first_u32(&s, &c, nonce, 0);
        let kind = games::verified_kind(&game(g)).unwrap();
        let win = games::is_win(kind, &params, u).unwrap();
        let expect_pay = if win { max } else { 0 };
        let bal1 = h.player(&w).await.balance;
        assert_eq!(bal1, bal0 - amount + expect_pay, "bet {n} {g}");
        if win { wins += 1 }
        let v = h.vault().await;
        assert_eq!(v.reserved_payouts, 0);
        assert_eq!(v.total_deposited as i128 + v.accrued_fees as i128, initial as i128 + v.net_pnl as i128, "LP value + fees = deposit + pnl");
        if v.fee_hwm > 0 { assert!((v.accrued_fees as i128) * 10_000 <= (v.fee_hwm as i128) * 2000, "fees only on net gains"); }
        assert!(h.data(&commit_pda(&hash)).await.is_none(), "commitment rent reclaimed");
        assert_conserved(&mut h, &[&w]).await;
    }
    assert!(wins > 40 && wins < 200, "wins {wins}");
    let settler_after = h.lamports(&h.settler.pubkey()).await;
    // the settler paid tx fees only, rent came back (fees are paid by ctx.payer here, so no loss at all)
    assert!(settler_after >= settler_before - 1_000_000, "commitment rent must come back to the settler");
}

#[tokio::test]
async fn nobody_but_the_settler_can_settle_and_seeds_cannot_be_chosen() {
    let mut h = H::new().await;
    setup(&mut h, 100 * SOL).await;
    let p = new_player(&mut h, 5 * SOL).await;
    let w = p.pubkey();
    let (_i, s, hash, _c, _m) = h.open_bet(&p, "dice", 50_000_000, dice(true, 5000)).await.unwrap();

    // an attacker who knows the revealed seed still cannot settle
    let attacker = Keypair::new();
    h.fund(&attacker.pubkey(), SOL).await;
    let ix = h.ix_settle(&w, hash, s, u64::MAX, &attacker.pubkey());
    expect(h.send(&[ix], &[&attacker]).await, OpenHouseError::Unauthorized);

    // wrong seed
    expect(h.settle(&w, hash, [9u8; 32]).await, OpenHouseError::InvalidServerSeedHash);

    // a second player cannot reuse the same commitment while it is open
    let p2 = new_player(&mut h, 5 * SOL).await;
    let ix = H::ix_bet(&p2.pubkey(), &p2.pubkey(), "flip", 10_000_000, 19_800_000, flip(false), [1u8; 32], hash);
    expect(h.send(&[ix], &[&p2]).await, OpenHouseError::SeedAlreadyUsed);

    // a hash the settler never committed to cannot be used at all
    let fake = seed_hash(&[7u8; 32]);
    let ix = H::ix_bet(&p2.pubkey(), &p2.pubkey(), "flip", 10_000_000, 19_800_000, flip(false), [1u8; 32], fake);
    expect_fail(h.send(&[ix], &[&p2]).await);

    // one open bet at a time
    let i = h.next_seed; h.next_seed += 1;
    let s2 = seed(i); let h2 = seed_hash(&s2);
    let sc = h.ix_commit(h2, i);
    let st = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();
    h.send(&[sc], &[&st]).await.unwrap();
    let ix = H::ix_bet(&w, &w, "flip", 10_000_000, 19_800_000, flip(false), [2u8; 32], h2);
    expect(h.send(&[ix], &[&p]).await, OpenHouseError::BetInProgress);

    h.settle(&w, hash, s).await.unwrap();
    assert!(!h.player(&w).await.open.active);
    // settling twice is impossible, the commitment is gone and nothing is open
    expect_fail(h.settle(&w, hash, s).await);
}

#[tokio::test]
async fn players_cannot_lie_about_payouts_and_the_vault_cap_holds() {
    let mut h = H::new().await;
    setup(&mut h, 100 * SOL).await;
    let p = new_player(&mut h, 50 * SOL).await;
    let w = p.pubkey();
    let st = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();
    let mut commit = |h: &mut H| { let i = h.next_seed; h.next_seed += 1; (i, seed(i), seed_hash(&seed(i))) };

    // max payout must be exactly what the game pays
    let (i, _s, hh) = commit(&mut h);
    let c = h.ix_commit(hh, i); h.send(&[c], &[&st]).await.unwrap();
    let ix = H::ix_bet(&w, &w, "dice", 100_000_000, 100_000_000, dice(true, 5000), [0u8; 32], hh);
    expect(h.send(&[ix], &[&p]).await, OpenHouseError::InvalidGameParams);
    // bad dice target
    let ix = H::ix_bet(&w, &w, "dice", 100_000_000, 200_000_000, dice(true, 9900), [0u8; 32], hh);
    expect(h.send(&[ix], &[&p]).await, OpenHouseError::InvalidGameParams);
    // unknown game
    let ix = H::ix_bet(&w, &w, "poker2", 100_000_000, 200_000_000, [0u8; 32], [0u8; 32], hh);
    expect(h.send(&[ix], &[&p]).await, OpenHouseError::InvalidGameParams);
    // below minimum
    let ix = H::ix_bet(&w, &w, "flip", 1_000, 1_980, flip(false), [0u8; 32], hh);
    expect(h.send(&[ix], &[&p]).await, OpenHouseError::AmountTooSmall);
    // 1% of the free vault is 1 SOL of risk. 1000x limbo on 1 SOL risks 999 SOL.
    let ix = H::ix_bet(&w, &w, "limbo", SOL, 1000 * SOL, limbo(100_000), [0u8; 32], hh);
    expect(h.send(&[ix], &[&p]).await, OpenHouseError::MaxBetExceeded);
    // a bet the vault can carry is fine: 0.1 SOL at 10x risks 0.9 SOL
    let ix = H::ix_bet(&w, &w, "limbo", SOL / 10, SOL, limbo(1000), [0u8; 32], hh);
    h.send(&[ix], &[&p]).await.unwrap();
    assert_eq!(h.vault().await.reserved_payouts, 9 * SOL / 10);
    // an LP cannot pull out the liquidity that backs the open bet
    let lp_free = h.vault().await.total_deposited - h.vault().await.reserved_payouts;
    assert!(lp_free < 100 * SOL);
}

#[tokio::test]
async fn refund_pause_withdraw_and_authority() {
    let mut h = H::new().await;
    setup(&mut h, 100 * SOL).await;
    let p = new_player(&mut h, 5 * SOL).await;
    let w = p.pubkey();
    let (_i, _s, _hash, _c, _m) = h.open_bet(&p, "flip", 100_000_000, flip(true)).await.unwrap();
    // too early
    expect(h.send(&[H::ix_refund(&w, &w)], &[&p]).await, OpenHouseError::RefundTimeoutNotReached);
    // a stranger can never trigger a refund
    let x = Keypair::new();
    h.warp(121).await;
    expect(h.send(&[H::ix_refund(&x.pubkey(), &w)], &[&x]).await, OpenHouseError::Unauthorized);
    let before = h.player(&w).await.balance;
    h.send(&[H::ix_refund(&w, &w)], &[&p]).await.unwrap();
    assert_eq!(h.player(&w).await.balance, before + 100_000_000);
    assert_eq!(h.vault().await.reserved_payouts, 0);
    assert!(!h.player(&w).await.open.active);
    assert_conserved(&mut h, &[&w]).await;

    // only the authority can pause, and a pause never blocks withdrawing
    let auth = Keypair::from_bytes(&h.authority.to_bytes()).unwrap();
    let evil = Keypair::new();
    h.fund(&evil.pubkey(), SOL).await;
    let ix = h.ix_pause(&evil.pubkey(), true);
    expect(h.send(&[ix], &[&evil]).await, OpenHouseError::Unauthorized);
    let ix = h.ix_pause(&auth.pubkey(), true);
    h.send(&[ix], &[&auth]).await.unwrap();
    let i = h.next_seed; h.next_seed += 1;
    let hh = seed_hash(&seed(i));
    let st = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();
    let c = h.ix_commit(hh, i); h.send(&[c], &[&st]).await.unwrap();
    let ix = H::ix_bet(&w, &w, "flip", 10_000_000, 19_800_000, flip(false), [0u8; 32], hh);
    expect(h.send(&[ix], &[&p]).await, OpenHouseError::BettingPaused);
    let wallet_before = h.lamports(&w).await;
    h.send(&[H::ix_wd_player(&w, 2 * SOL)], &[&p]).await.unwrap();
    assert_eq!(h.lamports(&w).await, wallet_before + 2 * SOL);
    // withdrawing more than your balance fails
    expect(h.send(&[H::ix_wd_player(&w, 100 * SOL)], &[&p]).await, OpenHouseError::InsufficientPlayerFunds);
    // another wallet cannot touch your balance
    let thief = Keypair::new();
    h.fund(&thief.pubkey(), SOL).await;
    expect_fail(h.send(&[H::ix_wd_player(&thief.pubkey(), 1)], &[&thief]).await);
    let ix = h.ix_pause(&auth.pubkey(), false);
    h.send(&[ix], &[&auth]).await.unwrap();
    assert_conserved(&mut h, &[&w]).await;

    // two step authority handover
    let nxt = Keypair::new();
    h.fund(&nxt.pubkey(), SOL).await;
    let prop = Instruction { program_id: openhouse::ID, data: openhouse::instruction::ProposeAuthority { new_authority: nxt.pubkey() }.data(),
        accounts: openhouse::accounts::ProposeAuthority { config: config_pda(), authority: auth.pubkey() }.to_account_metas(None) };
    h.send(&[prop], &[&auth]).await.unwrap();
    let acc = Instruction { program_id: openhouse::ID, data: openhouse::instruction::AcceptAuthority {}.data(),
        accounts: openhouse::accounts::AcceptAuthority { config: config_pda(), new_authority: evil.pubkey() }.to_account_metas(None) };
    expect(h.send(&[acc], &[&evil]).await, OpenHouseError::Unauthorized);
    let acc = Instruction { program_id: openhouse::ID, data: openhouse::instruction::AcceptAuthority {}.data(),
        accounts: openhouse::accounts::AcceptAuthority { config: config_pda(), new_authority: nxt.pubkey() }.to_account_metas(None) };
    h.send(&[acc], &[&nxt]).await.unwrap();
    assert_eq!(h.config().await.authority, nxt.pubkey());
    let ix = h.ix_pause(&auth.pubkey(), true);
    expect(h.send(&[ix], &[&auth]).await, OpenHouseError::Unauthorized);
}

#[tokio::test]
async fn session_keys_and_responsible_play_limits() {
    let mut h = H::new().await;
    setup(&mut h, 100 * SOL).await;
    let p = new_player(&mut h, 5 * SOL).await;
    let w = p.pubkey();
    let start = 1_700_000_000i64;
    h.set_time(start).await;
    let sk = Keypair::new();
    h.send(&[H::ix_session(&w, &sk.pubkey(), 3600, 100_000_000)], &[&p]).await.unwrap();
    let st = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();

    // session key bets inside its cap
    let mut place = |h: &mut H, signer: &Keypair, amount: u64| -> (Instruction, [u8; 32], [u8; 32], u64) {
        let i = h.next_seed; h.next_seed += 1;
        let s = seed(i); let hh = seed_hash(&s);
        (H::ix_bet(&w, &signer.pubkey(), "flip", amount, amount * 198 / 100, flip(false), cseed(i), hh), hh, s, i)
    };
    let (ix, hh, s, i) = place(&mut h, &sk, 60_000_000);
    let c = h.ix_commit(hh, i); h.send(&[c], &[&st]).await.unwrap();
    h.send(&[ix], &[&sk]).await.unwrap();
    h.settle(&w, hh, s).await.unwrap();
    // over the cap
    let (ix, hh, _s, i) = place(&mut h, &sk, 60_000_000);
    let c = h.ix_commit(hh, i); h.send(&[c], &[&st]).await.unwrap();
    expect(h.send(&[ix], &[&sk]).await, OpenHouseError::SessionKeyExpired);
    // a random key is not a session key
    let rnd = Keypair::new();
    let (ix, hh, _s, i) = place(&mut h, &rnd, 10_000_000);
    let c = h.ix_commit(hh, i); h.send(&[c], &[&st]).await.unwrap();
    expect(h.send(&[ix], &[&rnd]).await, OpenHouseError::Unauthorized);
    // the session key can never withdraw: the player PDA is derived from the signer
    expect_fail(h.send(&[H::ix_wd_player(&sk.pubkey(), 1)], &[&sk]).await);
    // expiry
    h.send(&[H::ix_session(&w, &sk.pubkey(), 3600, 500_000_000)], &[&p]).await.unwrap();
    h.warp(3601).await;
    let (ix, hh, _s, i) = place(&mut h, &sk, 10_000_000);
    let c = h.ix_commit(hh, i); h.send(&[c], &[&st]).await.unwrap();
    expect(h.send(&[ix], &[&sk]).await, OpenHouseError::SessionKeyExpired);

    // daily loss limit: tightening is instant, loosening waits a day
    let now = h.now().await;
    h.send(&[H::ix_limits(&w, 50_000_000)], &[&p]).await.unwrap();
    let (ix, hh2, _s2, i2) = place(&mut h, &p, 60_000_000);
    let c = h.ix_commit(hh2, i2); h.send(&[c], &[&st]).await.unwrap();
    expect(h.send(&[ix], &[&p]).await, OpenHouseError::DailyLossLimitExceeded);
    h.send(&[H::ix_limits(&w, 0)], &[&p]).await.unwrap(); // remove limit, queued
    let (ix, hh3, _s3, i3) = place(&mut h, &p, 60_000_000);
    let c = h.ix_commit(hh3, i3); h.send(&[c], &[&st]).await.unwrap();
    expect(h.send(&[ix], &[&p]).await, OpenHouseError::DailyLossLimitExceeded);
    assert!(h.player(&w).await.pending_limit_at >= now + 86_400);
    h.warp(86_401).await;
    let (ix, hh4, s4, i4) = place(&mut h, &p, 60_000_000);
    let c = h.ix_commit(hh4, i4); h.send(&[c], &[&st]).await.unwrap();
    h.send(&[ix], &[&p]).await.unwrap();
    h.settle(&w, hh4, s4).await.unwrap();

    // self exclusion blocks bets, never withdrawals, and can only be extended
    h.send(&[H::ix_exclude(&w, 30)], &[&p]).await.unwrap();
    let until = h.player(&w).await.excluded_until;
    h.send(&[H::ix_exclude(&w, 1)], &[&p]).await.unwrap();
    assert_eq!(h.player(&w).await.excluded_until, until);
    let (ix, hh5, _s5, i5) = place(&mut h, &p, 10_000_000);
    let c = h.ix_commit(hh5, i5); h.send(&[c], &[&st]).await.unwrap();
    expect(h.send(&[ix], &[&p]).await, OpenHouseError::SelfExcluded);
    h.send(&[H::ix_wd_player(&w, SOL)], &[&p]).await.unwrap();
}

#[tokio::test]
async fn fees_come_only_from_net_edge_and_lps_can_exit() {
    let mut h = H::new().await;
    setup(&mut h, 100 * SOL).await;
    let p = new_player(&mut h, 40 * SOL).await;
    let w = p.pubkey();
    for n in 0..150u64 { // coin flips at 1.98x, a 1% edge
        let (_i, s, hash, _c, _m) = h.open_bet(&p, "flip", 100_000_000, flip(n % 2 == 0)).await.unwrap();
        h.settle(&w, hash, s).await.unwrap();
    }
    let v = h.vault().await;
    assert_eq!(v.total_deposited as i128 + v.accrued_fees as i128, 100 * SOL as i128 + v.net_pnl as i128);
    // fees are charged only on new highs of cumulative net PnL, never more than fee_bps of the high-water mark
    if v.fee_hwm > 0 { assert!(v.accrued_fees as i128 * 10_000 <= v.fee_hwm as i128 * 2000); } else { assert_eq!(v.accrued_fees, 0); }
    assert!(v.fee_hwm >= v.net_pnl.max(0));
    // only the authority, only up to what accrued, only to the configured wallet
    let auth = Keypair::from_bytes(&h.authority.to_bytes()).unwrap();
    let cfg = h.config().await;
    let ix = h.ix_fees(&cfg.fee_wallet, v.accrued_fees + 1, &auth.pubkey());
    expect(h.send(&[ix], &[&auth]).await, OpenHouseError::VaultUnderfunded);
    let thief = Keypair::new();
    h.fund(&thief.pubkey(), SOL).await;
    let ix = h.ix_fees(&thief.pubkey(), 1, &thief.pubkey());
    expect_fail(h.send(&[ix], &[&thief]).await);
    if v.accrued_fees > 0 {
        let before = h.lamports(&cfg.fee_wallet).await;
        let ix = h.ix_fees(&cfg.fee_wallet, v.accrued_fees, &auth.pubkey());
        h.send(&[ix], &[&auth]).await.unwrap();
        assert!(h.lamports(&cfg.fee_wallet).await >= before + v.accrued_fees - 10_000_000);
    }
    assert_conserved(&mut h, &[&w]).await;
}

async fn run_200_bets_and_reconcile(
    h: &mut H,
    p: &Keypair,
    g: &str,
    kind: Kind,
    mut make_bet: impl FnMut(usize) -> (u64, [u8; 32]),
) {
    let w = p.pubkey();
    for step in 0..200 {
        let (amount, params) = make_bet(step);
        let before = h.player(&w).await.balance;
        let (_i, s, hash, c, _max) = h.open_bet(p, g, amount, params).await.unwrap();
        assert_conserved(h, &[&w]).await;
        let nonce = h.player(&w).await.open.nonce;
        let expected_payout = games::settle_payout(kind, &params, amount, &s, &c, nonce).unwrap();
        h.settle(&w, hash, s).await.unwrap();
        let after = h.player(&w).await.balance;
        assert_eq!(after, before - amount + expected_payout, "{g} step {step}");
        assert_conserved(h, &[&w]).await;
    }
}

#[tokio::test]
async fn wheel_200_bets_and_attacks() {
    let mut h = H::new().await;
    setup(&mut h, 1000 * SOL).await;
    let p = new_player(&mut h, 50 * SOL).await;
    let w = p.pubkey();
    let st = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();

    run_200_bets_and_reconcile(&mut h, &p, "wheel", Kind::Wheel, |step| {
        let mut pr = [0u8; 32];
        pr[0] = (step % 3) as u8;
        (50_000_000, pr)
    }).await;

    let i = h.next_seed; h.next_seed += 1;
    let hh = seed_hash(&seed(i));
    let sc = h.ix_commit(hh, i); h.send(&[sc], &[&st]).await.unwrap();
    let mut bad = [0u8; 32]; bad[0] = 3;
    expect(h.send(&[H::ix_bet(&w, &w, "wheel", 10_000_000, 15_200_000, bad, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
    let mut ok = [0u8; 32]; ok[0] = 0;
    expect(h.send(&[H::ix_bet(&w, &w, "wheel", 10_000_000, 99_999_999, ok, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
}

#[tokio::test]
async fn plinko_200_bets_and_attacks() {
    let mut h = H::new().await;
    setup(&mut h, 1000 * SOL).await;
    let p = new_player(&mut h, 50 * SOL).await;
    let w = p.pubkey();
    let st = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();
    let rows = [8u8, 10, 12, 14, 16];

    run_200_bets_and_reconcile(&mut h, &p, "plinko", Kind::Plinko, |step| {
        let mut pr = [0u8; 32];
        pr[0] = rows[step % rows.len()];
        pr[1] = ((step / rows.len()) % 3) as u8;
        (10_000_000, pr)
    }).await;

    let i = h.next_seed; h.next_seed += 1;
    let hh = seed_hash(&seed(i));
    let sc = h.ix_commit(hh, i); h.send(&[sc], &[&st]).await.unwrap();
    let mut bad_rows = [0u8; 32]; bad_rows[0] = 9; bad_rows[1] = 0;
    expect(h.send(&[H::ix_bet(&w, &w, "plinko", 10_000_000, 100_000_000, bad_rows, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
    let mut bad_risk = [0u8; 32]; bad_risk[0] = 8; bad_risk[1] = 3;
    expect(h.send(&[H::ix_bet(&w, &w, "plinko", 10_000_000, 100_000_000, bad_risk, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
    let mut ok = [0u8; 32]; ok[0] = 8; ok[1] = 0;
    expect(h.send(&[H::ix_bet(&w, &w, "plinko", 10_000_000, 10_000_000, ok, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
}

#[tokio::test]
async fn keno_200_bets_and_attacks() {
    let mut h = H::new().await;
    setup(&mut h, 1000 * SOL).await;
    let p = new_player(&mut h, 50 * SOL).await;
    let w = p.pubkey();
    let st = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();

    run_200_bets_and_reconcile(&mut h, &p, "keno", Kind::Keno, |step| {
        let n_picks = 1 + (step % 10);
        let mask: u64 = (1u64 << n_picks) - 1;
        let mut pr = [0u8; 32];
        pr[0..8].copy_from_slice(&mask.to_le_bytes());
        (10_000_000, pr)
    }).await;

    let i = h.next_seed; h.next_seed += 1;
    let hh = seed_hash(&seed(i));
    let sc = h.ix_commit(hh, i); h.send(&[sc], &[&st]).await.unwrap();
    let mut eleven = [0u8; 32]; eleven[0..8].copy_from_slice(&((1u64 << 11) - 1).to_le_bytes());
    expect(h.send(&[H::ix_bet(&w, &w, "keno", 10_000_000, 100_000_000, eleven, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
    let mut ok = [0u8; 32]; ok[0] = 1;
    expect(h.send(&[H::ix_bet(&w, &w, "keno", 10_000_000, 10_000_000, ok, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
}

#[tokio::test]
async fn roulette_200_bets_and_attacks() {
    let mut h = H::new().await;
    setup(&mut h, 1000 * SOL).await;
    let p = new_player(&mut h, 50 * SOL).await;
    let w = p.pubkey();
    let st = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();

    run_200_bets_and_reconcile(&mut h, &p, "roulette", Kind::Roulette, |step| {
        let mut pr = [0u8; 32];
        let s1 = (step % 37) as u8;
        let s2 = 37 + ((step / 37) % 12) as u8;
        pr[0] = s1; pr[1..3].copy_from_slice(&1u16.to_le_bytes());
        pr[3] = s2; pr[4..6].copy_from_slice(&2u16.to_le_bytes());
        (3 * games::ROULETTE_UNIT, pr)
    }).await;

    let i = h.next_seed; h.next_seed += 1;
    let hh = seed_hash(&seed(i));
    let sc = h.ix_commit(hh, i); h.send(&[sc], &[&st]).await.unwrap();
    let mut pr = [0u8; 32];
    pr[0] = 17; pr[1..3].copy_from_slice(&1u16.to_le_bytes());
    // stake != units * 0.001 SOL
    expect(h.send(&[H::ix_bet(&w, &w, "roulette", 2 * games::ROULETTE_UNIT, 36 * games::ROULETTE_UNIT, pr, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
    // wrong max_payout
    expect(h.send(&[H::ix_bet(&w, &w, "roulette", games::ROULETTE_UNIT, 10 * games::ROULETTE_UNIT, pr, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
    // duplicate slot
    let mut dup = pr; dup[3] = 17; dup[4..6].copy_from_slice(&1u16.to_le_bytes());
    expect(h.send(&[H::ix_bet(&w, &w, "roulette", 2 * games::ROULETTE_UNIT, 72 * games::ROULETTE_UNIT, dup, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
}

#[tokio::test]
async fn batch2_slots_and_scratch_200_bets() {
    for (name, kind) in [
        ("hotreels", Kind::HotReels),
        ("slots777", Kind::Slots777),
        ("vaultbreak", Kind::VaultBreak),
        ("scratch", Kind::Scratch),
    ] {
        let mut h = H::new().await;
        setup(&mut h, 1000 * SOL).await;
        let p = new_player(&mut h, 50 * SOL).await;
        run_200_bets_and_reconcile(&mut h, &p, name, kind, |_| (SOL / 100, [0u8; 32])).await;
    }
}

#[tokio::test]
async fn sicbo_dragon_baccarat_200_bets_and_attacks() {
    let mut h = H::new().await;
    setup(&mut h, 1000 * SOL).await;
    let p = new_player(&mut h, 50 * SOL).await;
    let w = p.pubkey();
    let st = Keypair::from_bytes(&h.settler.to_bytes()).unwrap();

    run_200_bets_and_reconcile(&mut h, &p, "sicbo", Kind::SicBo, |step| {
        let mut pr = [0u8; 32];
        pr[0] = (step % 8) as u8;
        (SOL / 100, pr)
    }).await;

    run_200_bets_and_reconcile(&mut h, &p, "dragon", Kind::Dragon, |step| {
        let mut pr = [0u8; 32];
        pr[0] = (step % 3) as u8;
        (SOL / 100, pr)
    }).await;

    // Baccarat 200 bets (includes ties/pushes where payout == stake, reconciled every step)
    run_200_bets_and_reconcile(&mut h, &p, "baccarat", Kind::Baccarat, |step| {
        let mut pr = [0u8; 32];
        pr[0] = (step % 3) as u8;
        (SOL / 100, pr)
    }).await;

    // Attack tests: bad params and wrong max_payout
    let i = h.next_seed; h.next_seed += 1;
    let hh = seed_hash(&seed(i));
    let sc = h.ix_commit(hh, i); h.send(&[sc], &[&st]).await.unwrap();
    let mut bad = [0u8; 32];
    bad[0] = 8;
    expect(h.send(&[H::ix_bet(&w, &w, "sicbo", SOL / 100, SOL / 100 * 11, bad, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
    bad[0] = 3;
    expect(h.send(&[H::ix_bet(&w, &w, "dragon", SOL / 100, SOL / 100 * 2, bad, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
    expect(h.send(&[H::ix_bet(&w, &w, "baccarat", SOL / 100, SOL / 100 * 2, bad, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
    // wrong max_payout
    let ok = [0u8; 32];
    expect(h.send(&[H::ix_bet(&w, &w, "baccarat", SOL / 100, SOL / 100 * 9, ok, [0u8; 32], hh)], &[&p]).await, OpenHouseError::InvalidGameParams);
}
