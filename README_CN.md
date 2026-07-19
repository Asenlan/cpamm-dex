# CPAMM DEX

Solana 链上恒定乘积自动做市商（AMM）。Uniswap V2 风格的 x\*y=k 不变式，含 0.3% 手续费、LP 代币和滑点保护。Anchor 实现。

## 功能

- **恒定乘积不变式** — `reserve_a * reserve_b = k`
- **0.3% 交易手续费** — 留在池子中，按份额归 LP 持有者
- **LP 代币** — 流动性提供者获得池子份额的凭证
- **滑点保护** — 交易 `min_amount_out`，添加流动性 `min_lp_out`
- **排序代币地址** — 无论传入顺序，池子 PDA 地址确定唯一
- **PDA 金库** — 两种代币储备由程序派生地址托管

## 交易数学

```
手续费 = 输入量 × 费率分子 / 费率分母
有效输入 = 输入量 - 手续费
输出量 = (有效输入 × 输出储备) / (输入储备 + 有效输入)
```

0.3% 费率下：`输出量 = (输入量 × 997 × 输出储备) / (输入储备 × 1000 + 输入量 × 997)`

所有计算使用 u128 中间类型，无溢出风险。

## LP 数学

**首个提供者：** `lp数量 = sqrt(代币A × 代币B)`

**后续提供：** `lp数量 = min(添加A/储备A, 添加B/储备B) × 总供应量`

**提取：** `提取量 = lp销毁量 × 储备量 / 总供应量`

## 指令

| 指令 | 调用者 | 说明 |
|------|--------|------|
| `initialize_pool` | 任何人 | 创建池子，存入初始流动性，铸造 LP |
| `swap` | 任何人 | A→B 或 B→A 兑换 |
| `add_liquidity` | LP | 按比例存入代币，铸造 LP 份额 |
| `remove_liquidity` | LP | 销毁 LP 份额，提取代币 |

## PDA 派生

| 账户 | 种子 |
|------|------|
| Pool | `[b"pool", mint_low, mint_high]` |
| Vault A | `[b"vault-a", pool]` |
| Vault B | `[b"vault-b", pool]` |
| LP Mint | `[b"lp-mint", pool]` |

## 快速开始

```bash
anchor build
anchor test
```

## 许可

MIT
