use anchor_lang::prelude::*;

#[error_code]
pub enum OpenHouseError {
    #[msg("Bet risk exceeds k percent of the free vault balance")]
    MaxBetExceeded,
    #[msg("Not enough balance in your player account")]
    InsufficientPlayerFunds,
    #[msg("Session key expired or spend cap reached")]
    SessionKeyExpired,
    #[msg("Server seed does not match the committed hash")]
    InvalidServerSeedHash,
    #[msg("There is no open bet")]
    NoOpenBet,
    #[msg("Finish or refund your open bet first")]
    BetInProgress,
    #[msg("Bet cannot be refunded yet")]
    RefundTimeoutNotReached,
    #[msg("Vault does not have enough free liquidity")]
    VaultUnderfunded,
    #[msg("Betting is paused")]
    BettingPaused,
    #[msg("Signer is not allowed to do this")]
    Unauthorized,
    #[msg("Invalid game or game parameters")]
    InvalidGameParams,
    #[msg("This seed commitment was already used")]
    SeedAlreadyUsed,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Daily loss limit reached")]
    DailyLossLimitExceeded,
    #[msg("Withdrawal would take the vault below rent exemption")]
    RentExemptViolation,
    #[msg("Account is self-excluded")]
    SelfExcluded,
    #[msg("Vault has no value left, deposits are closed until the authority resets it")]
    VaultDepleted,
    #[msg("Amount is too small")]
    AmountTooSmall,
    #[msg("Commitment cannot be reclaimed yet")]
    CommitmentNotReclaimable,
    // --- batch3 additions (keep existing codes unchanged above) ---
    #[msg("Round is not finished yet")]
    RoundNotFinished,
    #[msg("Invalid action for this game")]
    InvalidAction,
    #[msg("Force settle timeout not reached yet")]
    ForceSettleTimeoutNotReached,
    #[msg("Action log is full (max 32 actions)")]
    ActionLogFull,
}
