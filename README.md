# CPAMM DEX

Constant-Product Automated Market Maker on Solana. Uniswap V2-style x\*y=k invariant with 0.3% fee, LP tokens, and slippage protection. Built with Anchor.

## Features

- **Constant-product invariant** — `reserve_a * reserve_b = k`
- **0.3% swap fee** — Stays in pool, accrues to liquidity providers
- **LP tokens** — Proportional share of pool reserves, minted on deposit
- **Slippage protection** — `min_amount_out` for swaps, `min_lp_out` for liquidity
- **Sorted mint derivation** — Deterministic pool PDA regardless of token order
- **PDA vaults** — Both token reserves held in program-derived accounts

## Swap Math

```
fee = amount_in * fee_num / fee_denom
effective_in = amount_in - fee
amount_out = (effective_in * reserve_out) / (reserve_in + effective_in)
```

With 0.3% fee: `amount_out = (amount_in * 997 * reserve_out) / (reserve_in * 1000 + amount_in * 997)`

All computation uses u128 intermediates — no overflow risk.

## LP Math

**First provider:** `lp_tokens = sqrt(amount_a * amount_b)`

**Subsequent:** `lp_tokens = min(amount_a/reserve_a, amount_b/reserve_b) * total_supply`

**Withdrawal:** `amount = lp_amount * reserve / total_supply`

## Instructions

| Instruction | Caller | Description |
|---|---|---|
| `initialize_pool` | Anyone | Create pool, deposit initial liquidity, mint LP |
| `swap` | Anyone | Swap A→B or B→A |
| `add_liquidity` | LP | Deposit tokens in ratio, mint LP shares |
| `remove_liquidity` | LP | Burn LP shares, withdraw tokens |

## PDA Derivation

| Account | Seeds |
|---------|-------|
| Pool | `[b"pool", mint_low, mint_high]` |
| Vault A | `[b"vault-a", pool]` |
| Vault B | `[b"vault-b", pool]` |
| LP Mint | `[b"lp-mint", pool]` |

## Quick Start

```bash
anchor build
anchor test
```

## License

MIT
