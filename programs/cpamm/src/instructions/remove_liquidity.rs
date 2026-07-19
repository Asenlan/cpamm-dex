//! Remove liquidity from the pool.
//!
//! LP burns their LP tokens and receives a proportional share of
//! both token reserves.

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Burn, Transfer};

use crate::errors::CpammError;
use crate::state::{Pool, POOL_SEED};

#[derive(Accounts)]
pub struct RemoveLiquidity<'info> {
    /// The LP removing liquidity
    #[account(mut)]
    pub provider: Signer<'info>,

    /// Pool state
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

    /// LP token mint
    #[account(
        mut,
        constraint = lp_mint.key() == pool.lp_mint,
    )]
    pub lp_mint: Account<'info, anchor_spl::token::Mint>,

    /// Provider's LP token account (to burn from)
    #[account(
        mut,
        constraint = provider_lp.owner == provider.key(),
        constraint = provider_lp.mint == pool.lp_mint,
    )]
    pub provider_lp: Account<'info, TokenAccount>,

    /// Vault A
    #[account(
        mut,
        constraint = vault_a.key() == pool.vault_a,
    )]
    pub vault_a: Account<'info, TokenAccount>,

    /// Vault B
    #[account(
        mut,
        constraint = vault_b.key() == pool.vault_b,
    )]
    pub vault_b: Account<'info, TokenAccount>,

    /// Provider's token A account (to receive funds)
    #[account(
        mut,
        constraint = provider_token_a.owner == provider.key(),
        constraint = provider_token_a.mint == pool.token_a_mint,
    )]
    pub provider_token_a: Account<'info, TokenAccount>,

    /// Provider's token B account
    #[account(
        mut,
        constraint = provider_token_b.owner == provider.key(),
        constraint = provider_token_b.mint == pool.token_b_mint,
    )]
    pub provider_token_b: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<RemoveLiquidity>, lp_amount: u64) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    require!(lp_amount > 0, CpammError::ZeroAmount);
    require!(
        ctx.accounts.provider_lp.amount >= lp_amount,
        CpammError::InsufficientLpTokens
    );

    // Compute amounts to withdraw
    let (amount_a, amount_b) = Pool::compute_remove_amounts(
        lp_amount,
        pool.reserve_a,
        pool.reserve_b,
        pool.total_lp_supply,
    )
    .ok_or(CpammError::LpCalculationError)?;

    require!(amount_a > 0 && amount_b > 0, CpammError::ZeroAmount);
    require!(
        amount_a <= pool.reserve_a && amount_b <= pool.reserve_b,
        CpammError::InsufficientReserves
    );

    // Burn LP tokens — pool PDA signs
    let bump = pool.bump;
    let pool_signer: &[&[u8]] = &[
        POOL_SEED,
        pool.token_a_mint.as_ref(),
        pool.token_b_mint.as_ref(),
        &[bump],
    ];

    token::burn(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint: ctx.accounts.lp_mint.to_account_info(),
                from: ctx.accounts.provider_lp.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            pool_signer,
        ),
        lp_amount,
    )?;

    // Transfer token A back to provider
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault_a.to_account_info(),
                to: ctx.accounts.provider_token_a.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            pool_signer,
        ),
        amount_a,
    )?;

    // Transfer token B back to provider
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault_b.to_account_info(),
                to: ctx.accounts.provider_token_b.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            pool_signer,
        ),
        amount_b,
    )?;

    // Update pool state
    pool.reserve_a = pool.reserve_a.checked_sub(amount_a).ok_or(CpammError::Overflow)?;
    pool.reserve_b = pool.reserve_b.checked_sub(amount_b).ok_or(CpammError::Overflow)?;
    pool.total_lp_supply = pool.total_lp_supply.checked_sub(lp_amount).ok_or(CpammError::Overflow)?;

    // If all liquidity removed, mark pool inactive
    if pool.total_lp_supply == 0 {
        pool.is_active = false;
    }

    Ok(())
}
