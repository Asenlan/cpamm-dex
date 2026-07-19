# CPAMM DEX

Constant-Product Automated Market Maker on Solana. Uniswap V2-style x\*y=k with fee, LP tokens, and slippage protection. Built with Anchor.

## Features

- **Constant-product invariant** — `reserve_a * reserve_b = k`
- **0.3% swap fee** — stays in pool, accrues to LPs
- **LP tokens** — proportional share of pool reserves
- **Slippage protection** — `min_amount_out` for swaps, `min_lp_out` for liquidity
- **Sorted mints** — deterministic pool derivation regardless of token order
- **PDA vaults** — both token reserves held in program-derived accounts
- **12 unit tests** — full coverage of swap math, LP math, edge cases

## Instructions

| Instruction | Caller | Description |
|---|---|---|
| `initialize_pool` | Anyone | Create pool, deposit initial liquidity, mint LP |
| `swap` | Anyone | Swap A→B or B→A |
| `add_liquidity` | LP | Deposit tokens in ratio, mint LP shares |
| `remove_liquidity` | LP | Burn LP shares, withdraw proportion |

## Swap Math

```
fee = amount_in * fee_numerator / fee_denominator
amount_in_after_fee = amount_in - fee
amount_out = (amount_in_after_fee * reserve_out) / (reserve_in + amount_in_after_fee)
```

With 0.3% fee: `amount_out = (amount_in * 997 * reserve_out) / (reserve_in * 1000 + amount_in * 997)`

All computation uses u128 intermediates — no overflow risk.

## LP Math

**First provider:** `lp_tokens = sqrt(amount_a * amount_b)`

**Subsequent:** `lp_tokens = min(amount_a/reserve_a, amount_b/reserve_b) * total_supply`

**Withdrawal:** `amount = lp_amount * reserve / total_supply`

## Project Structure

```
programs/cpamm/src/
├── lib.rs                  # Entry + IDL dispatch
├── state.rs                # Pool account + AMM math (12 unit tests)
├── errors.rs               # Custom errors
└── instructions/
    ├── initialize.rs       # Create pool
    ├── swap.rs             # x*y=k swap
    ├── add_liquidity.rs    # Mint LP
    └── remove_liquidity.rs # Burn LP
tests/
└── cpamm.ts                # 5 integration tests
```

## PDA Derivation

| Account | Seeds |
|---------|-------|
| Pool | `[b"pool", mint_low, mint_high]` |
| Vault A | `[b"vault-a", pool]` |
| Vault B | `[b"vault-b", pool]` |
| LP Mint | `[b"lp-mint", pool]` |

## Getting Started

### Prerequisites

- Rust 1.70+, Solana CLI 1.18+, Anchor 0.30+, Node 18+

### Build & Test

```bash
anchor build
anchor test
```

## License

MIT
