/// Pure integer dice payout calculation (no f64 on-chain).
/// `target_bp` and `roll_bp` are in basis points [0..10000).
pub fn compute_dice_payout(roll_under: bool, target_bp: u16, bet_lamports: u64, roll_bp: u16) -> (bool, u64) {
    if target_bp == 0 || target_bp >= 10000 || roll_bp >= 10000 {
        return (false, 0);
    }
    let win = if roll_under {
        roll_bp < target_bp
    } else {
        roll_bp > (10000 - target_bp)
    };
    if win {
        let payout = ((bet_lamports as u128) * 9900u128 / (target_bp as u128)) as u64;
        (true, payout)
    } else {
        (false, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dice_integer_math() {
        let (win, payout) = compute_dice_payout(true, 4950, 1_000_000_000, 2500);
        assert!(win);
        assert_eq!(payout, 2_000_000_000);

        let (lose, zero) = compute_dice_payout(true, 4950, 1_000_000_000, 4950);
        assert!(!lose);
        assert_eq!(zero, 0);

        let (win_over, p_over) = compute_dice_payout(false, 4950, 1_000_000_000, 7500);
        assert!(win_over);
        assert_eq!(p_over, 2_000_000_000);
    }
}
