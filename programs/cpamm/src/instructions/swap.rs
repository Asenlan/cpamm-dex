//! Swap tokens using constant-product AMM.
//!
//! User specifies `amount_in` of one token and `min_amount_out` for
//! slippage protection. The pool computes the actual output using
//! the x*y=k invariant with fee deduction.

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::errors::CpammError;
use crate::state::{Pool, POOL_SEED};

#[derive(Accounts)]
pub struct Swap<'info> {
    /// The user performing the swap
    #[account(mut)]
    pub user: Signer<'info>,

    /// The pool state
    #[account(
        seeds = [
            POOL_SEED,
            pool.token_a_mint.as_ref(),
            pool.token_b_mint.as_ref(),
        ],
        bump = pool.bump,
        constraint = pool.is_active @ CpammError::PoolInactive,
    )]
    pub pool: Account<'info, Pool>,

    /// User's token account for the input token
    #[account(
        mut,
        constraint = user_input_account.owner == user.key(),
        constraint = user_input_account.mint == input_mint.key(),
    )]
    pub user_input_account: Account<'info, TokenAccount>,

    /// User's token account for the output token
    #[account(
        mut,
        constraint = user_output_account.owner == user.key(),
        constraint = user_output_account.mint == output_mint.key(),
    )]
    pub user_output_account: Account<'info, TokenAccount>,

    /// Pool vault for the input token
    #[account(
        mut,
        constraint = input_vault.key() == if is_a { pool.vault_a } else { pool.vault_b },
        constraint = input_vault.mint == input_mint.key(),
    )]
    pub input_vault: Account<'info, TokenAccount>,

    /// Pool vault for the output token
    #[account(
        mut,
        constraint = output_vault.key() == if is_a { pool.vault_b } else { pool.vault_a },
        constraint = output_vault.mint == output_mint.key(),
    )]
    pub output_vault: Account<'info, TokenAccount>,

    /// The mint for the input token
    /// CHECK: used only for mint comparison via constraints above
    pub input_mint: UncheckedAccount<'info>,

    /// The mint for the output token
    /// CHECK: used only for mint comparison via constraints above
    pub output_mint: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<Swap>, amount_in: u64, min_amount_out: u64, swap_a_to_b: bool) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    require!(amount_in > 0, CpammError::ZeroAmount);

    let (reserve_in, reserve_out) = if swap_a_to_b {
        (pool.reserve_a, pool.reserve_b)
    } else {
        (pool.reserve_b, pool.reserve_a)
    };

    // Compute output amount
    let amount_out = Pool::compute_swap_out(
        amount_in,
        reserve_in,
        reserve_out,
        pool.fee_numerator,
        pool.fee_denominator,
    )
    .ok_or(CpammError::SwapCalculationError)?;

    // Slippage protection
    require!(amount_out >= min_amount_out, CpammError::SlippageExceeded);
    require!(amount_out < reserve_out, CpammError::InsufficientReserves);

    // Transfer input tokens from user to pool vault
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_input_account.to_account_info(),
                to: ctx.accounts.input_vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount_in,
    )?;

    // Transfer output tokens from pool vault to user — PDA signs
    let pool_key = pool.key();
    let bump = pool.bump;
    let pool_signer: &[&[u8]] = &[
        POOL_SEED,
        pool.token_a_mint.as_ref(),
        pool.token_b_mint.as_ref(),
        &[bump],
    ];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.output_vault.to_account_info(),
                to: ctx.accounts.user_output_account.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            pool_signer,
        ),
        amount_out,
    )?;

    // Update reserves after transfer (reserves reflect vault balances)
    if swap_a_to_b {
        pool.reserve_a = pool.reserve_a.checked_add(amount_in).ok_or(CpammError::Overflow)?;
        pool.reserve_b = pool.reserve_b.checked_sub(amount_out).ok_or(CpammError::Overflow)?;
    } else {
        pool.reserve_b = pool.reserve_b.checked_add(amount_in).ok_or(CpammError::Overflow)?;
        pool.reserve_a = pool.reserve_a.checked_sub(amount_out).ok_or(CpammError::Overflow)?;
    }

    Ok(())
}
