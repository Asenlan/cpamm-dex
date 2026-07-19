//! Pool account state and constant-product AMM math.
//!
//! Follows the Uniswap V2 invariant: x * y = k
//! Fee: 0.3% (30 basis points) — configurable per pool.

use anchor_lang::prelude::*;

/// Seeds for PDA derivation throughout the program.
pub const POOL_SEED: &[u8] = b"pool";
pub const VAULT_A_SEED: &[u8] = b"vault-a";
pub const VAULT_B_SEED: &[u8] = b"vault-b";
pub const LP_MINT_SEED: &[u8] = b"lp-mint";

/// A constant-product automated market maker pool.
///
/// PDAs derived from this pool:
/// - vault_a: [b"vault-a", pool.key()]
/// - vault_b: [b"vault-b", pool.key()]
/// - lp_mint: [b"lp-mint", pool.key()]
#[account]
#[derive(InitSpace)]
pub struct Pool {
    /// Token A mint (the "base" token, e.g., SOL)
    pub token_a_mint: Pubkey,
    /// Token B mint (the "quote" token, e.g., USDC)
    pub token_b_mint: Pubkey,
    /// PDA token account holding token A reserves
    pub vault_a: Pubkey,
    /// PDA token account holding token B reserves
    pub vault_b: Pubkey,
    /// PDA mint for LP tokens
    pub lp_mint: Pubkey,
    /// Current token A reserve (raw amount, no decimals)
    pub reserve_a: u64,
    /// Current token B reserve
    pub reserve_b: u64,
    /// Fee numerator (e.g., 30 for 0.3%)
    pub fee_numerator: u64,
    /// Fee denominator (e.g., 10000)
    pub fee_denominator: u64,
    /// Total LP tokens in circulation
    pub total_lp_supply: u64,
    /// Whether the pool is active (false after draining)
    pub is_active: bool,
    /// PDA bump for the pool account
    pub bump: u8,
}

/// Sorted mint addresses — used to deterministically derive pool address.
///
/// Token order in [b"pool", mint_a, mint_b] is sorted by pubkey bytes
/// so pool(tA, tB) == pool(tB, tA).
pub fn sort_mints(a: &Pubkey, b: &Pubkey) -> (Pubkey, Pubkey) {
    if a.to_bytes() <= b.to_bytes() {
        (*a, *b)
    } else {
        (*b, *a)
    }
}

impl Pool {
    /// Compute swap output using the constant-product formula with fee.
    ///
    /// Given `amount_in` of the input token, returns `amount_out` of the
    /// output token.
    ///
    /// # Math
    ///
    /// ```text
    ///   fee = amount_in * fee_numerator / fee_denominator
    ///   amount_in_after_fee = amount_in - fee
    ///   amount_out = (amount_in_after_fee * reserve_out)
    ///              / (reserve_in + amount_in_after_fee)
    /// ```
    ///
    /// Uses u128 for intermediate calculations to prevent overflow.
    pub fn compute_swap_out(
        amount_in: u64,
        reserve_in: u64,
        reserve_out: u64,
        fee_num: u64,
        fee_denom: u64,
    ) -> Option<u64> {
        if amount_in == 0 || reserve_in == 0 || reserve_out == 0 {
            return None;
        }

        // fee = amount_in * fee_num / fee_denom
        let fee = (amount_in as u128)
            .checked_mul(fee_num as u128)?
            .checked_div(fee_denom as u128)?;

        // amount_in_after_fee = amount_in - fee
        let amount_in_after_fee = (amount_in as u128).checked_sub(fee)?;

        // amount_out = (amount_in_after_fee * reserve_out) / (reserve_in + amount_in_after_fee)
        let numerator = amount_in_after_fee.checked_mul(reserve_out as u128)?;
        let denominator = (reserve_in as u128).checked_add(amount_in_after_fee)?;

        let amount_out = numerator.checked_div(denominator)?;

        // ponytail: clamp to u64 — realistically AMM operations fit in u64
        Some(amount_out as u64)
    }

    /// Compute swap input needed for a desired output.
    ///
    /// ```text
    ///   amount_in = (desired_out * reserve_in) / ((reserve_out - desired_out) * (1 - fee))
    /// ```
    pub fn compute_swap_in(
        desired_out: u64,
        reserve_in: u64,
        reserve_out: u64,
        fee_num: u64,
        fee_denom: u64,
    ) -> Option<u64> {
        if desired_out == 0 || reserve_in == 0 || reserve_out <= desired_out {
            return None;
        }

        let fee_multiplier = (fee_denom as u128).checked_sub(fee_num as u128)?;
        let denominator = (reserve_out as u128)
            .checked_sub(desired_out as u128)?
            .checked_mul(fee_multiplier)?;

        let numerator = (desired_out as u128).checked_mul(reserve_in as u128)?.checked_mul(fee_denom as u128)?;

        // Add 1 for rounding up (user pays slightly more, never slightly less)
        let amount_in = numerator.checked_div(denominator)?;
        let has_remainder = numerator.checked_rem(denominator).unwrap_or(0) > 0;

        Some(amount_in as u64 + if has_remainder { 1 } else { 0 })
    }

    /// Compute LP tokens to mint when adding liquidity.
    ///
    /// If pool is empty (first provider), LP tokens = sqrt(amount_a * amount_b).
    /// Otherwise, LP tokens = min(amount_a * supply / reserve_a, amount_b * supply / reserve_b).
    pub fn compute_lp_tokens_for_add(
        amount_a: u64,
        amount_b: u64,
        reserve_a: u64,
        reserve_b: u64,
        total_supply: u64,
    ) -> Option<u64> {
        if amount_a == 0 || amount_b == 0 {
            return None;
        }

        if total_supply == 0 {
            // First liquidity provider: geometric mean
            let product = (amount_a as u128).checked_mul(amount_b as u128)?;
            let sqrt = integer_sqrt(product);
            Some(sqrt as u64)
        } else {
            // Proportional: min(amount_a / reserve_a, amount_b / reserve_b) * total_supply
            let share_a = (amount_a as u128)
                .checked_mul(total_supply as u128)?
                .checked_div(reserve_a as u128)?;
            let share_b = (amount_b as u128)
                .checked_mul(total_supply as u128)?
                .checked_div(reserve_b as u128)?;

            Some(share_a.min(share_b) as u64)
        }
    }

    /// Compute optimal token amounts for adding liquidity in correct ratio.
    ///
    /// Given `amount_a_desired` and current pool reserves, returns optimal
    /// `(amount_a, amount_b)` to maintain the pool ratio.
    pub fn optimal_amounts(
        desired_a: u64,
        desired_b: u64,
        reserve_a: u64,
        reserve_b: u64,
    ) -> Option<(u64, u64)> {
        if reserve_a == 0 || reserve_b == 0 {
            return Some((desired_a, desired_b));
        }

        // If providing amount_b based on amount_a:
        // optimal_b = desired_a * reserve_b / reserve_a
        let optimal_b = (desired_a as u128)
            .checked_mul(reserve_b as u128)?
            .checked_div(reserve_a as u128)? as u64;

        if optimal_b <= desired_b {
            Some((desired_a, optimal_b))
        } else {
            let optimal_a = (desired_b as u128)
                .checked_mul(reserve_a as u128)?
                .checked_div(reserve_b as u128)? as u64;
            Some((optimal_a, desired_b))
        }
    }

    /// Compute amounts returned for burning a given number of LP tokens.
    pub fn compute_remove_amounts(
        lp_amount: u64,
        reserve_a: u64,
        reserve_b: u64,
        total_supply: u64,
    ) -> Option<(u64, u64)> {
        if lp_amount == 0 || total_supply == 0 {
            return None;
        }

        let amount_a = (lp_amount as u128)
            .checked_mul(reserve_a as u128)?
            .checked_div(total_supply as u128)? as u64;
        let amount_b = (lp_amount as u128)
            .checked_mul(reserve_b as u128)?
            .checked_div(total_supply as u128)? as u64;

        Some((amount_a, amount_b))
    }
}

/// Integer square root using Newton's method.
///
/// ```text
///   x_{n+1} = (x_n + n / x_n) / 2
/// ```
fn integer_sqrt(n: u128) -> u128 {
    if n <= 1 {
        return n;
    }

    let mut x0 = n / 2;
    if x0 == 0 {
        x0 = 1;
    }

    loop {
        let x1 = (x0 + n / x0) / 2;
        if x1 >= x0 {
            return x0;
        }
        x0 = x1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swap_exact_output() {
        // Pool: 1000 USDC, 10 SOL
        // Swap 100 USDC → should get ~0.906 SOL (before fee)
        let reserve_in = 1_000_000; // USDC (6 decimals)
        let reserve_out = 10_000_000_000; // SOL (9 decimals)
        let amount_in = 100_000; // 0.1 USDC

        let out = Pool::compute_swap_out(amount_in, reserve_in, reserve_out, 30, 10000).unwrap();
        // Expected: amount_in_after_fee = 100_000 * 0.997 = 99_700
        // out = 99_700 * 10_000_000_000 / (1_000_000 + 99_700) ≈ 906,...
        assert!(out > 0);
    }

    #[test]
    fn test_swap_zero_amount_returns_none() {
        assert_eq!(Pool::compute_swap_out(0, 100, 100, 30, 10000), None);
    }

    #[test]
    fn test_lp_first_provider() {
        // First deposit: 1000 USDC (6 dec) and 10 SOL (9 dec)
        let lp = Pool::compute_lp_tokens_for_add(
            1_000_000,      // 1000 USDC
            10_000_000_000,  // 10 SOL
            0,              // no reserves yet
            0,
            0,
        )
        .unwrap();
        // sqrt(1e6 * 1e10) = sqrt(1e16) = 1e8
        assert_eq!(lp, 100_000_000);
    }

    #[test]
    fn test_lp_proportional() {
        // Pool has 1000 LP, reserves 1000 USDC : 10 SOL
        // Add 100 USDC : 1 SOL → should get ~100 LP
        let lp = Pool::compute_lp_tokens_for_add(
            100_000,         // 100 USDC
            1_000_000_000,   // 1 SOL
            1_000_000,       // reserve USDC
            10_000_000_000,  // reserve SOL
            1000,            // total supply
        )
        .unwrap();
        assert_eq!(lp, 100);
    }

    #[test]
    fn test_remove_amounts() {
        let (a, b) = Pool::compute_remove_amounts(
            500,            // burn half the LP
            1_000_000,      // reserve USDC
            10_000_000_000, // reserve SOL
            1000,           // total supply
        )
        .unwrap();
        assert_eq!(a, 500_000);
        assert_eq!(b, 5_000_000_000);
    }

    #[test]
    fn test_optimal_amounts() {
        let (a, b) = Pool::optimal_amounts(
            200_000,        // want to add 200 USDC
            5_000_000_000,  // and 5 SOL
            1_000_000,      // current reserve USDC
            10_000_000_000, // current reserve SOL
        )
        .unwrap();
        // Ratio is 1:10000, so 200 USDC → 2 SOL optimal
        assert_eq!(a, 200_000);
        assert_eq!(b, 2_000_000_000); // 2 SOL
    }

    #[test]
    fn test_integer_sqrt() {
        assert_eq!(integer_sqrt(0), 0);
        assert_eq!(integer_sqrt(1), 1);
        assert_eq!(integer_sqrt(4), 2);
        assert_eq!(integer_sqrt(100), 10);
        assert_eq!(integer_sqrt(10_000_000_000_000_000), 100_000_000);
    }

    #[test]
    fn test_sort_mints() {
        let a = Pubkey::new_from_array([1u8; 32]);
        let b = Pubkey::new_from_array([2u8; 32]);
        let (low, high) = sort_mints(&a, &b);
        assert_eq!(low, a);
        assert_eq!(high, b);

        // Reverse order
        let (low, high) = sort_mints(&b, &a);
        assert_eq!(low, a);
        assert_eq!(high, b);
    }

    #[test]
    fn test_compute_swap_in() {
        // Pool: 1000 USDC, 10 SOL. I want exactly 0.5 SOL out.
        let amount_in = Pool::compute_swap_in(
            500_000_000,    // desired 0.5 SOL (9 dec)
            1_000_000,      // reserve USDC
            10_000_000_000, // reserve SOL
            30,             // 0.3% fee
            10000,
        )
        .unwrap();
        // Should require roughly ~52 USDC in
        assert!(amount_in > 0);
        assert!(amount_in < 100_000); // less than 100 USDC
    }
}
