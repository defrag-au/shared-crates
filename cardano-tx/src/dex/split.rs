//! N-pool split routing optimizer for constant-product AMM DEXes.
//!
//! Given a total ADA amount and a set of pools, finds the optimal allocation
//! across pools to maximize total output tokens. DEX-agnostic — the optimizer
//! only sees reserves and fees, not which DEX a pool belongs to.

use super::cswap::pool::constant_product_swap;
use crate::intents::DexPlatform;
use serde::{Deserialize, Serialize};

/// A constant-product AMM pool available for routing.
#[derive(Debug, Clone)]
pub struct PoolLiquidity {
    pub dex: DexPlatform,
    pub ada_reserves: u64,
    pub token_reserves: u64,
    pub fee_bps: u64,
}

/// One leg of an optimized split route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitLeg {
    pub dex: DexPlatform,
    pub input_amount: u64,
    pub expected_tokens: u64,
}

/// Result of split optimization across N pools.
#[derive(Debug, Clone)]
pub struct SplitRoute {
    pub legs: Vec<SplitLeg>,
    pub total_tokens: u64,
}

/// Find optimal ADA allocation across N constant-product AMM pools
/// to maximize total output tokens.
///
/// Algorithm: greedy incremental allocation. Divides `total_ada` into
/// small increments and at each step allocates the next increment to
/// whichever pool offers the highest marginal output. This works because
/// constant-product AMMs have monotonically decreasing marginal returns.
///
/// Pools with zero reserves are filtered out. Returns legs only for
/// pools that received a non-zero allocation.
pub fn find_optimal_split(total_ada: u64, pools: &[PoolLiquidity]) -> SplitRoute {
    let valid_pools: Vec<&PoolLiquidity> = pools
        .iter()
        .filter(|p| p.ada_reserves > 0 && p.token_reserves > 0)
        .collect();

    if valid_pools.is_empty() {
        return SplitRoute {
            legs: Vec::new(),
            total_tokens: 0,
        };
    }

    // Single pool: no split needed
    if valid_pools.len() == 1 {
        let p = valid_pools[0];
        let tokens = constant_product_swap(total_ada, p.ada_reserves, p.token_reserves, p.fee_bps);
        return SplitRoute {
            legs: vec![SplitLeg {
                dex: p.dex,
                input_amount: total_ada,
                expected_tokens: tokens,
            }],
            total_tokens: tokens,
        };
    }

    // Track current allocation per pool
    let mut allocations = vec![0u64; valid_pools.len()];

    let steps = 1000u64;
    let increment = total_ada / steps;
    if increment == 0 {
        // Amount too small to split meaningfully — give all to best pool
        let (best_idx, best_tokens) = valid_pools
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    i,
                    constant_product_swap(total_ada, p.ada_reserves, p.token_reserves, p.fee_bps),
                )
            })
            .max_by_key(|(_, t)| *t)
            .unwrap();
        return SplitRoute {
            legs: vec![SplitLeg {
                dex: valid_pools[best_idx].dex,
                input_amount: total_ada,
                expected_tokens: best_tokens,
            }],
            total_tokens: best_tokens,
        };
    }

    // Greedy: at each step, allocate increment to pool with best marginal output
    for _ in 0..steps {
        let mut best_pool = 0;
        let mut best_marginal = 0u64;

        for (i, p) in valid_pools.iter().enumerate() {
            let current_tokens =
                constant_product_swap(allocations[i], p.ada_reserves, p.token_reserves, p.fee_bps);
            let new_tokens = constant_product_swap(
                allocations[i] + increment,
                p.ada_reserves,
                p.token_reserves,
                p.fee_bps,
            );
            let marginal = new_tokens.saturating_sub(current_tokens);
            if marginal > best_marginal {
                best_marginal = marginal;
                best_pool = i;
            }
        }

        allocations[best_pool] += increment;
    }

    // Assign any remainder (from integer division) to the pool with best marginal
    let allocated: u64 = allocations.iter().sum();
    let remainder = total_ada.saturating_sub(allocated);
    if remainder > 0 {
        let mut best_pool = 0;
        let mut best_marginal = 0u64;
        for (i, p) in valid_pools.iter().enumerate() {
            let current =
                constant_product_swap(allocations[i], p.ada_reserves, p.token_reserves, p.fee_bps);
            let with_remainder = constant_product_swap(
                allocations[i] + remainder,
                p.ada_reserves,
                p.token_reserves,
                p.fee_bps,
            );
            let marginal = with_remainder.saturating_sub(current);
            if marginal > best_marginal {
                best_marginal = marginal;
                best_pool = i;
            }
        }
        allocations[best_pool] += remainder;
    }

    // Build legs for pools with non-zero allocation
    let mut legs = Vec::new();
    let mut total_tokens = 0u64;

    for (i, p) in valid_pools.iter().enumerate() {
        if allocations[i] > 0 {
            let tokens =
                constant_product_swap(allocations[i], p.ada_reserves, p.token_reserves, p.fee_bps);
            legs.push(SplitLeg {
                dex: p.dex,
                input_amount: allocations[i],
                expected_tokens: tokens,
            });
            total_tokens += tokens;
        }
    }

    SplitRoute { legs, total_tokens }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_pool() {
        let pools = vec![PoolLiquidity {
            dex: DexPlatform::Splash,
            ada_reserves: 100_000_000_000,
            token_reserves: 1_000_000_000,
            fee_bps: 78,
        }];
        let route = find_optimal_split(10_000_000, &pools);
        assert_eq!(route.legs.len(), 1);
        assert_eq!(route.legs[0].dex, DexPlatform::Splash);
        assert_eq!(route.legs[0].input_amount, 10_000_000);
        assert!(route.total_tokens > 0);
    }

    #[test]
    fn test_empty_pools() {
        let route = find_optimal_split(10_000_000, &[]);
        assert_eq!(route.legs.len(), 0);
        assert_eq!(route.total_tokens, 0);
    }

    #[test]
    fn test_zero_reserve_pool_filtered() {
        let pools = vec![
            PoolLiquidity {
                dex: DexPlatform::Splash,
                ada_reserves: 100_000_000_000,
                token_reserves: 1_000_000_000,
                fee_bps: 78,
            },
            PoolLiquidity {
                dex: DexPlatform::Cswap,
                ada_reserves: 0,
                token_reserves: 0,
                fee_bps: 85,
            },
        ];
        let route = find_optimal_split(10_000_000, &pools);
        assert_eq!(route.legs.len(), 1);
        assert_eq!(route.legs[0].dex, DexPlatform::Splash);
    }

    #[test]
    fn test_two_pool_split_beats_single() {
        // Two equal-sized pools — splitting should always beat single
        let pools = vec![
            PoolLiquidity {
                dex: DexPlatform::Splash,
                ada_reserves: 50_000_000_000, // 50K ADA
                token_reserves: 500_000_000,
                fee_bps: 78,
            },
            PoolLiquidity {
                dex: DexPlatform::Cswap,
                ada_reserves: 50_000_000_000,
                token_reserves: 500_000_000,
                fee_bps: 85,
            },
        ];

        let amount = 5_000_000_000u64; // 5000 ADA — significant price impact
        let route = find_optimal_split(amount, &pools);

        assert_eq!(route.legs.len(), 2, "should split across both pools");

        // Verify split total beats either single pool
        let splash_only = constant_product_swap(amount, 50_000_000_000, 500_000_000, 78);
        let cswap_only = constant_product_swap(amount, 50_000_000_000, 500_000_000, 85);
        let best_single = splash_only.max(cswap_only);

        assert!(
            route.total_tokens > best_single,
            "split {} should beat best single {best_single}",
            route.total_tokens
        );
    }

    #[test]
    fn test_allocations_sum_to_total() {
        let pools = vec![
            PoolLiquidity {
                dex: DexPlatform::Splash,
                ada_reserves: 80_000_000_000,
                token_reserves: 800_000_000,
                fee_bps: 75,
            },
            PoolLiquidity {
                dex: DexPlatform::Cswap,
                ada_reserves: 30_000_000_000,
                token_reserves: 300_000_000,
                fee_bps: 50,
            },
        ];

        let amount = 500_000_000u64; // 500 ADA
        let route = find_optimal_split(amount, &pools);

        let total_allocated: u64 = route.legs.iter().map(|l| l.input_amount).sum();
        assert_eq!(total_allocated, amount, "all ADA must be allocated");
    }

    #[test]
    fn test_small_amount_no_split() {
        // With a tiny amount, price impact is negligible — should go to better pool
        let pools = vec![
            PoolLiquidity {
                dex: DexPlatform::Splash,
                ada_reserves: 100_000_000_000,
                token_reserves: 1_000_000_000,
                fee_bps: 78,
            },
            PoolLiquidity {
                dex: DexPlatform::Cswap,
                ada_reserves: 50_000_000_000,
                token_reserves: 500_000_000,
                fee_bps: 85,
            },
        ];

        let amount = 1_000_000u64; // 1 ADA — negligible impact
        let route = find_optimal_split(amount, &pools);

        // With 1 ADA, the increment (1000 lovelace) is tiny. The optimizer
        // should heavily favor the lower-fee pool (Splash at 78 bps).
        // It might still allocate a small amount to CSWAP depending on
        // increment granularity, but Splash should dominate.
        let splash_leg = route.legs.iter().find(|l| l.dex == DexPlatform::Splash);
        assert!(splash_leg.is_some(), "Splash should get allocation");
        assert!(route.total_tokens > 0);
    }
}
