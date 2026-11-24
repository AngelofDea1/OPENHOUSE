// Keep in the same order as programs/openhouse/src/errors.rs (test/errors.test.ts checks it).
export const ERROR_MESSAGES = [
  'That bet is too big for the house right now. Try a smaller bet',
  'Not enough SOL in your playing balance. Deposit more to play',
  'Your quick-play session expired. Reconnect your wallet to continue',
  'Something went wrong verifying the round. Please try again',
  'There is no open bet',
  'Finish or refund your open bet first',
  'Your open bet cannot be refunded yet, please wait a moment',
  'The house does not have enough funds for that payout right now. Try a smaller bet',
  'Betting is paused right now',
  'You are not allowed to do that',
  'Invalid bet settings. Please check your bet and try again',
  'That round already ran. Please try again',
  'That amount is too large',
  'Daily loss limit reached',
  'Cannot withdraw that much, a tiny amount must stay in the account',
  'Your account is currently on a self-exclusion break',
  'House deposits are temporarily closed',
  'Bet amount is too small',
  'Cannot reclaim that yet, please wait a moment',
] as const;

export const ERROR_NAMES = [
  'MaxBetExceeded', 'InsufficientPlayerFunds', 'SessionKeyExpired', 'InvalidServerSeedHash', 'NoOpenBet',
  'BetInProgress', 'RefundTimeoutNotReached', 'VaultUnderfunded', 'BettingPaused', 'Unauthorized',
  'InvalidGameParams', 'SeedAlreadyUsed', 'MathOverflow', 'DailyLossLimitExceeded', 'RentExemptViolation',
  'SelfExcluded', 'VaultDepleted', 'AmountTooSmall', 'CommitmentNotReclaimable',
] as const;

/** Turn anything a wallet or RPC throws into one sentence a player can act on. */
export function explainError(e: unknown): string {
  const text = String((e as any)?.message ?? e ?? '');
  const logs: string[] = (e as any)?.logs ?? (e as any)?.transactionLogs ?? [];
  const hay = text + ' ' + logs.join(' ');
  const m = hay.match(/custom program error: 0x([0-9a-f]+)/i) || hay.match(/"Custom":\s*(\d+)/);
  if (m) {
    const code = m[0].includes('0x') ? parseInt(m[1], 16) : Number(m[1]);
    const i = code - 6000;
    if (i >= 0 && i < ERROR_MESSAGES.length) return ERROR_MESSAGES[i];
  }
  const named = hay.match(/Error Message: ([^.\n]+)/);
  if (named) return named[1];
  if (/User rejected|rejected the request|declined/i.test(hay)) return 'You cancelled the request in your wallet';
  if (/insufficient lamports|insufficient funds|0x1\b/i.test(hay)) return 'Not enough SOL in your wallet for this and the network fee';
  if (/blockhash not found|expired/i.test(hay)) return 'The transaction expired, please try again';
  if (/429|too many requests|rate limit/i.test(hay)) return 'Network is busy right now, please try again in a moment';
  if (/[{}\[\]]|0x[0-9a-f]{6,}|Error:|Simulation failed/i.test(text)) return 'Something went wrong. Please try again';
  return text.length > 0 && text.length <= 100 ? text : 'Something went wrong. Please try again';
}
