//! CPAMM — Constant-Product Automated Market Maker
//!
//! Uniswap V2-style AMM on Solana. Implements the x*y=k invariant with
//! 0.3% swap fee, LP token minting/burning, and slippage protection.
//!
//! ## Instructions
//!
//! - `initialize_pool` — Create pool with initial liquidity
//! - `swap` — Swap token A for B (or B for A)
//! - `add_liquidity` — Deposit tokens, mint LP shares
//! - `remove_liquidity` — Burn LP shares, withdraw tokens

use anchor_lang::prelude::*;

pub mod state;
pub mod errors;
pub mod instructions;

use instructions::*;

declare_id!("CPAMM1111111111111111111111111111111111");

#[program]
pub mod cpamm {
    use super::*;

    /// Create a new CPAMM pool with initial liquidity.
    ///
    /// Tokens must be sorted by pubkey (lowest first). Deposits `amount_a`
    /// and `amount_b` of initial liquidity. Mints LP tokens to payer.
    /// The pool fee is `fee_numerator / fee_denominator` (e.g., 30/10000 = 0.3%).
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        amount_a: u64,
        amount_b: u64,
        fee_numerator: u64,
        fee_denominator: u64,
    ) -> Result<()> {
        instructions::initialize::handler(ctx, amount_a, amount_b, fee_numerator, fee_denominator)
    }

    /// Swap tokens using the constant-product formula.
    ///
    /// `amount_in` — exact tokens the user sends to the pool.
    /// `min_amount_out` — slippage protection; tx fails if output is less.
    /// `swap_a_to_b` — true = A → B, false = B → A.
    pub fn swap(
        ctx: Context<Swap>,
        amount_in: u64,
        min_amount_out: u64,
        swap_a_to_b: bool,
    ) -> Result<()> {
        instructions::swap::handler(ctx, amount_in, min_amount_out, swap_a_to_b)
    }

    /// Add liquidity to an existing pool.
    ///
    /// `desired_a` / `desired_b` — maximum amounts provider is willing to deposit.
    /// Program computes optimal amounts to maintain pool ratio.
    /// `min_lp_out` — slippage protection for LP tokens minted.
    pub fn add_liquidity(
        ctx: Context<AddLiquidity>,
        desired_a: u64,
        desired_b: u64,
        min_lp_out: u64,
    ) -> Result<()> {
        instructions::add_liquidity::handler(ctx, desired_a, desired_b, min_lp_out)
    }

    /// Remove liquidity from the pool.
    ///
    /// Burns `lp_amount` LP tokens and returns proportional share of
    /// both token reserves to the provider.
    pub fn remove_liquidity(
        ctx: Context<RemoveLiquidity>,
        lp_amount: u64,
    ) -> Result<()> {
        instructions::remove_liquidity::handler(ctx, lp_amount)
    }
}
