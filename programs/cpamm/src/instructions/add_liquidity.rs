//! Add liquidity to an existing pool.
//!
//! LP must provide tokens in the same ratio as current pool reserves.
//! The optimal amounts are computed and LP tokens minted proportionally.

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, MintTo, Transfer};

use crate::errors::CpammError;
use crate::state::{Pool, POOL_SEED};

#[derive(Accounts)]
pub struct AddLiquidity<'info> {
    /// The liquidity provider
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

    /// Provider's token A account
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

    /// LP token mint
    #[account(
        mut,
        constraint = lp_mint.key() == pool.lp_mint,
    )]
    pub lp_mint: Account<'info, anchor_spl::token::Mint>,

    /// Provider's LP token account
    #[account(
        mut,
        constraint = provider_lp.owner == provider.key(),
        constraint = provider_lp.mint == pool.lp_mint,
    )]
    pub provider_lp: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(
    ctx: Context<AddLiquidity>,
    desired_a: u64,
    desired_b: u64,
    min_lp_out: u64,
) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    require!(desired_a > 0 && desired_b > 0, CpammError::ZeroAmount);

    // Compute optimal amounts based on current pool ratio
    let (amount_a, amount_b) = Pool::optimal_amounts(
        desired_a,
        desired_b,
        pool.reserve_a,
        pool.reserve_b,
    )
    .ok_or(CpammError::LpCalculationError)?;

    require!(amount_a > 0 && amount_b > 0, CpammError::ZeroAmount);

    // Compute LP tokens to mint
    let lp_amount = Pool::compute_lp_tokens_for_add(
        amount_a,
        amount_b,
        pool.reserve_a,
        pool.reserve_b,
        pool.total_lp_supply,
    )
    .ok_or(CpammError::LpCalculationError)?;

    require!(lp_amount >= min_lp_out, CpammError::SlippageExceeded);

    // Transfer token A
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.provider_token_a.to_account_info(),
                to: ctx.accounts.vault_a.to_account_info(),
                authority: ctx.accounts.provider.to_account_info(),
            },
        ),
        amount_a,
    )?;

    // Transfer token B
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.provider_token_b.to_account_info(),
                to: ctx.accounts.vault_b.to_account_info(),
                authority: ctx.accounts.provider.to_account_info(),
            },
        ),
        amount_b,
    )?;

    // Mint LP tokens — pool PDA signs
    let bump = pool.bump;
    let pool_signer: &[&[u8]] = &[
        POOL_SEED,
        pool.token_a_mint.as_ref(),
        pool.token_b_mint.as_ref(),
        &[bump],
    ];

    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.lp_mint.to_account_info(),
                to: ctx.accounts.provider_lp.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            pool_signer,
        ),
        lp_amount,
    )?;

    // Update pool reserves
    pool.reserve_a = pool.reserve_a.checked_add(amount_a).ok_or(CpammError::Overflow)?;
    pool.reserve_b = pool.reserve_b.checked_add(amount_b).ok_or(CpammError::Overflow)?;
    pool.total_lp_supply = pool.total_lp_supply.checked_add(lp_amount).ok_or(CpammError::Overflow)?;

    Ok(())
}
