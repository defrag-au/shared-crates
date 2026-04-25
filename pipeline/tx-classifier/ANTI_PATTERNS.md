# Anti-Patterns and Code Quality Guidelines

## ⚠️ CRITICAL: Avoid Hardcoded Values in Business Logic

### Why Hardcoded Values Are Problematic

Hardcoded values in business logic create maintenance nightmares and system fragility:

1. **Break with real-world data**: Assumptions encoded as constants often don't match actual transaction patterns
2. **Maintenance burden**: Every new case requires code changes instead of configuration updates  
3. **Testing difficulties**: Hard to test edge cases when values are baked into code
4. **Scaling issues**: Different markets/collections have different behaviors
5. **Silent failures**: When assumptions are wrong, the system continues with incorrect data

### Examples of Problematic Hardcoded Values

```rust
// ❌ BAD: Hardcoded mint costs
let typical_mint_cost = 52_000_000; // What if mints cost 15 ADA? 100 ADA?

// ❌ BAD: Hardcoded policy IDs  
if mint_op.unit.starts_with("8972aab...") {
    // UG policy ID - breaks when new collections emerge
}

// ❌ BAD: Hardcoded thresholds
if transaction_value > 1_000_000_000 { // What defines "high value"?
    classify_as_high_value();
}
```

### Better Approaches

```rust
// ✅ GOOD: Parse actual data from transaction
let actual_cost = calculate_from_inputs_outputs(&tx_data);

// ✅ GOOD: Configuration-driven
let mint_cost_threshold = config.get_mint_cost_threshold();

// ✅ GOOD: Context-aware calculations
let cost = estimate_from_transaction_context(&tx_data, &market_data);

// ✅ GOOD: Explicit uncertainty
if cannot_calculate_precisely() {
    return EstimateQuality::Uncertain;
}
```

## Current Technical Debt

### CBOR Transaction Cost Calculation

**Problem**: `calculate_direct_mint_cost()` cannot calculate accurate costs for CBOR transactions because inputs/outputs aren't parsed yet.

**Current State**: Returns `None` to be honest about limitations rather than hardcoded estimates.

**Solution Needed**: Implement full CBOR input/output parsing in the decoder crate.

### Audit Required

The entire tx-classifier codebase needs auditing for:

- [ ] Hardcoded ADA amounts
- [ ] Hardcoded policy IDs
- [ ] Hardcoded asset names/patterns  
- [ ] Hardcoded addresses
- [ ] Hardcoded thresholds
- [ ] Magic numbers without explanation

## Guidelines Moving Forward

1. **Configuration over Code**: Move constants to configuration files
2. **Parse Don't Assume**: Extract values from actual transaction data
3. **Explicit Uncertainty**: Return `None`/`Uncertain` when data is incomplete
4. **Context-Aware**: Use transaction context to guide calculations
5. **Test Reality**: Use real transaction data, not synthetic examples
6. **Document Assumptions**: When assumptions are necessary, document them clearly

## Migration Strategy

1. **Identify**: Audit all hardcoded values
2. **Extract**: Move to configuration or calculation functions  
3. **Test**: Verify with real transaction data
4. **Monitor**: Add logging to track when assumptions fail
5. **Iterate**: Update based on real-world feedback