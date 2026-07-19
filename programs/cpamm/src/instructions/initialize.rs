//! Initialize a new CPAMM pool with initial liquidity.

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, MintTo};

use crate::errors::CpammError;
use crate::state::{Pool, sort_mints, POOL_SEED, VAULT_A_SEED, VAULT_B_SEED, LP_MINT_SEED};

#[derive(Accounts)]
#[instruction(fee_numerator: u64, fee_denominator: u64)]
pub struct InitializePool<'info> {
    /// Who pays for account creation and provides initial liquidity
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Token A mint — must be sorted lower than token_b_mint
    pub token_a_mint: Account<'info, Mint>,

    /// Token B mint — must be sorted higher than token_a_mint
    pub token_b_mint: Account<'info, Mint>,

    /// Payer's token A account for initial deposit
    #[account(
        mut,
        constraint = payer_token_a.owner == payer.key(),
        constraint = payer_token_a.mint == token_a_mint.key(),
    )]
    pub payer_token_a: Account<'info, TokenAccount>,

    /// Payer's token B account for initial deposit
    #[account(
        mut,
        constraint = payer_token_b.owner == payer.key(),
        constraint = payer_token_b.mint == token_b_mint.key(),
    )]
    pub payer_token_b: Account<'info, TokenAccount>,

    /// Pool state account — PDA
    #[account(
        init,
        payer = payer,
        space = 8 + Pool::INIT_SPACE,
        seeds = [
            POOL_SEED,
            sort_mints(&token_a_mint.key(), &token_b_mint.key()).0.as_ref(),
            sort_mints(&token_a_mint.key(), &token_b_mint.key()).1.as_ref(),
        ],
        bump,
    )]
    pub pool: Account<'info, Pool>,

    /// Vault for token A — PDA token account
    #[account(
        init,
        payer = payer,
        token::mint = token_a_mint,
        token::authority = pool,
        seeds = [VAULT_A_SEED, pool.key().as_ref()],
        bump,
    )]
    pub vault_a: Account<'info, TokenAccount>,

    /// Vault for token B — PDA token account
    #[account(
        init,
        payer = payer,
        token::mint = token_b_mint,
        token::authority = pool,
        seeds = [VAULT_B_SEED, pool.key().as_ref()],
        bump,
    )]
    pub vault_b: Account<'info, TokenAccount>,

    /// LP token mint — PDA mint
    #[account(
        init,
        payer = payer,
        mint::decimals = 9,
        mint::authority = pool,
        seeds = [LP_MINT_SEED, pool.key().as_ref()],
        bump,
    )]
    pub lp_mint: Account<'info, Mint>,

    /// Payer's LP token account
    #[account(
        mut,
        constraint = payer_lp_account.owner == payer.key(),
        constraint = payer_lp_account.mint == lp_mint.key(),
    )]
    pub payer_lp_account: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<InitializePool>,
    amount_a: u64,
    amount_b: u64,
    fee_numerator: u64,
    fee_denominator: u64,
) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    let (token_a, token_b) = sort_mints(
        &ctx.accounts.token_a_mint.key(),
        &ctx.accounts.token_b_mint.key(),
    );

    // Validate sorted mints
    require!(
        token_a == ctx.accounts.token_a_mint.key() && token_b == ctx.accounts.token_b_mint.key(),
        CpammError::UnsortedMints
    );
    require!(amount_a > 0 && amount_b > 0, CpammError::ZeroAmount);
    require!(
        fee_numerator < fee_denominator && fee_denominator > 0,
        CpammError::Overflow
    );

    // Transfer token A from payer to vault
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            token::Transfer {
                from: ctx.accounts.payer_token_a.to_account_info(),
                to: ctx.accounts.vault_a.to_account_info(),
                authority: ctx.accounts.payer.to_account_info(),
            },
        ),
        amount_a,
    )?;

    // Transfer token B from payer to vault
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            token::Transfer {
                from: ctx.accounts.payer_token_b.to_account_info(),
                to: ctx.accounts.vault_b.to_account_info(),
                authority: ctx.accounts.payer.to_account_info(),
            },
        ),
        amount_b,
    )?;

    // Compute initial LP tokens: sqrt(amount_a * amount_b)
    let lp_amount = Pool::compute_lp_tokens_for_add(amount_a, amount_b, 0, 0, 0)
        .ok_or(CpammError::LpCalculationError)?;

    // Mint LP tokens to payer — pool PDA signs
    let pool_key = pool.key();
    let bump = ctx.bumps.pool;
    let pool_signer: &[&[u8]] = &[
        POOL_SEED,
        token_a.as_ref(),
        token_b.as_ref(),
        &[bump],
    ];

    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.lp_mint.to_account_info(),
                to: ctx.accounts.payer_lp_account.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            pool_signer,
        ),
        lp_amount,
    )?;

    // Populate pool state
    pool.token_a_mint = token_a;
    pool.token_b_mint = token_b;
    pool.vault_a = ctx.accounts.vault_a.key();
    pool.vault_b = ctx.accounts.vault_b.key();
    pool.lp_mint = ctx.accounts.lp_mint.key();
    pool.reserve_a = amount_a;
    pool.reserve_b = amount_b;
    pool.fee_numerator = fee_numerator;
    pool.fee_denominator = fee_denominator;
    pool.total_lp_supply = lp_amount;
    pool.is_active = true;
    pool.bump = bump;

    Ok(())
}
