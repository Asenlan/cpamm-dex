use anchor_lang::prelude::*;

#[error_code]
pub enum CpammError {
    #[msg("Amount must be greater than zero")]
    ZeroAmount,

    #[msg("Insufficient input amount for desired output")]
    InsufficientInput,

    #[msg("Output amount below minimum — slippage protection")]
    SlippageExceeded,

    #[msg("Insufficient reserves for swap")]
    InsufficientReserves,

    #[msg("Pool is not active")]
    PoolInactive,

    #[msg("LP token calculation overflow")]
    LpCalculationError,

    #[msg("Swap calculation overflow")]
    SwapCalculationError,

    #[msg("Insufficient LP tokens to burn")]
    InsufficientLpTokens,

    #[msg("Mint address must be sorted (lower pubkey first)")]
    UnsortedMints,

    #[msg("Arithmetic overflow")]
    Overflow,
}
