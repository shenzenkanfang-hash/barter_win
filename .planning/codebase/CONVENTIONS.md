RUST QUANTITATIVE TRADING SYSTEM - CODE CONVENTIONS

================================================================================
CORE SAFETY AND TYPE ENFORCEMENT
================================================================================

#![forbid(unsafe_code)] is enforced across all crates.

All crates must compile with this attribute. No exceptions. If you need unsafe
code for performance, you must first prove the safe alternative is insufficient.

================================================================================
FINANCIAL CALCULATIONS - DECIMAL MANDATORY
================================================================================

rust_decimal::Decimal is mandatory for all financial calculations.

This includes but is not limited to:
- Money amounts (balance, equity, PnL)
- Prices (entry, exit, mark price)
- Quantities (position size, order volume)
- Rates (interest, margin, fees)
- Any calculation involving currency or assets

NO f64 or f32 for any financial value. The precision requirements in trading
systems demand decimal arithmetic. Floating point errors are unacceptable.

================================================================================
TIMESTAMPS - UTC DATETIME REQUIRED
================================================================================

DateTime<Utc> is required for all timestamps across the system.

All market data timestamps, order timestamps, trade timestamps must use
DateTime<Utc> from the chrono crate with Utc timezone.

NO bare i64 timestamps. If you receive a timestamp from an exchange as i64,
convert to DateTime<Utc> immediately at the boundary layer.

================================================================================
LOCK STRATEGY AND CONCURRENCY MODEL
================================================================================

Lock Type: parking_lot::RwLock

This is the standard lock type across the codebase. Do not use std::sync::Mutex
or tokio::sync::Mutex unless you have a specific reason.

HOT PATH (Lock-Free Design):
- Tick processing
- Indicator calculation
- Strategy signal generation

These paths must be lock-free. Use atomic operations, ring buffers, or
message passing. If you need shared state in the hot path, consider:
- crossbeam-channel for mpsc between threads
- std::sync::atomic for simple flags/counters
- Arc<[T]> with interior mutability patterns

COLD PATH (RwLock Allowed):
- Account state
- Position management
- Portfolio-level calculations

These can use parking_lot::RwLock for shared read access.

LOCK ORDER TO PREVENT DEADLOCKS:
1. PositionManager (first acquired)
2. AccountPool (second acquired)

Always acquire locks in this order. Never reverse it. If you need both locks,
acquire PositionManager first, then AccountPool. Release in reverse order.

================================================================================
INCREMENTAL INDICATOR CALCULATIONS
================================================================================

O(1) incremental calculations are required for real-time indicators.

EMA (Exponential Moving Average) formula:
EMA_new = price * (2 / (period + 1)) + prev_EMA * (1 - 2 / (period + 1))

This allows constant-time update per new price tick.

For other indicators, maintain running state and update incrementally.
Avoid recalculating from scratch on each new data point.

================================================================================
ERROR HANDLING
================================================================================

Use thiserror for error types. Define errors as enums with thiserror::Error derive.

Explicit Result vs Option:
- Use Result<T, E> when errors are expected and should be handled
- Use Option<T> only when something is genuinely optional (not present)

NO unwrap() or expect() in production hot path code.
- These panic on None/Err and will crash the trading system
- If you must handle a None, use unwrap_or, unwrap_or_else, or match

Production hot path examples that are FORBIDDEN:
- data.unwrap()  // crashes if None
- result.expect("should have data")  // crashes if Err

Acceptable in tests or initialization code where you control the data.

================================================================================
NAMING CONVENTIONS
================================================================================

Modules and functions: snake_case
- get_current_price()
- calculate_ema()
- process_tick()

Types, enums, structs: CamelCase
- struct MarketData
- enum OrderStatus
- struct PriceLevel

Constants: SCREAMING_SNAKE_CASE
- const MAX_POSITION_SIZE: u64
- const DEFAULT_TIMEOUT_MS: u64

================================================================================
ASYNC RULES
================================================================================

All async fn must have Send + 'static bounds when they interact with shared state.

Example:
async fn process_order(order: Order) -> Result<(), Error>
where
    Self: Send + 'static,
{
    // implementation
}

ZERO tokio::spawn in hot path.
- tokio::spawn creates new tasks with overhead
- In hot path (tick processing), spawning tasks is too expensive
- Use synchronous channel send or atomic operations instead

For cold path (order execution, account updates), tokio::spawn is acceptable
if you need并发 concurrency.

================================================================================
DOCUMENTATION
================================================================================

All pub (public) items must have /// doc comments.

Required format:
/// Calculates the exponential moving average for a given period.
///
/// EMA is calculated as: EMA_new = price * (2 / (period + 1)) + prev * (1 - 2 / (period + 1))
///
/// # Arguments
/// * `price` - The current price value
/// * `period` - The EMA period (typically 12, 26, etc.)
///
/// # Example
/// let ema = calculate_ema(100.0, 26, Some(previous_ema));
///
/// # Panics
/// None - this function does not panic.
pub fn calculate_ema(price: Decimal, period: u32, prev: Option<Decimal>) -> Decimal

Private items (pub(crate), private) do not require doc comments but should
have regular comments for complex logic.

================================================================================
CARGO DEPENDENCY MANAGEMENT
================================================================================

Version constraints use caret (^) format.

Example:
rust_decimal = "^1.33"
chrono = "^0.4"
thiserror = "^1.0"

NO wildcard versions:
WRONG: rust_decimal = "*"
RIGHT: rust_decimal = "^1.33"

Wildcards make builds non-reproducible and can introduce breaking changes.

================================================================================
BREAKING CHANGES
================================================================================

When modifying core types or traits that are used across multiple crates:
1. Document the breaking change
2. Update the version number following SemVer
3. Add entry to CHANGELOG.md under Changed section

Core types include:
- MarketData
- Order
- Position
- Account
- Any shared trait definitions
