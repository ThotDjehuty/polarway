"""
Polarway Monads Example - Rust-Powered Functional Programming

This example demonstrates how to use Polarway's Rust-powered Result and Option monads
exposed via PyO3 for safe, functional error handling in Python.

IMPORTANT: This requires Polarway to be compiled with PyO3 bindings enabled.
The `polars.monads` module is exposed from the Rust crate `polars-python/src/monads.rs`

To build with monad support:
    cd polarway
    cargo build --features python
    maturin develop --features python
"""

import polarway as pw

# These imports require Polarway compiled with PyO3 bindings
# See: polarway/crates/polars-python/src/monads.rs
from polars.monads import Result, Option, Thunk  # type: ignore

import polars as pl


def result_monad_examples():
    """Demonstrate Result<T, E> monad usage."""
    print("=" * 60)
    print("Result Monad Examples")
    print("=" * 60)
    
    # Create Result values
    success = Result.ok(42)
    failure = Result.err("Something went wrong")
    
    # Check status
    print(f"success.is_ok(): {success.is_ok()}")  # True
    print(f"failure.is_err(): {failure.is_err()}")  # True
    
    # Unwrap values safely
    print(f"success.unwrap(): {success.unwrap()}")  # 42
    print(f"failure.unwrap_or('default'): {failure.unwrap_or('default')}")  # 'default'
    
    # Map over Result (only applies to Ok values)
    doubled = success.map(lambda x: x * 2)
    print(f"doubled.unwrap(): {doubled.unwrap()}")  # 84
    
    # Chain operations with and_then (flat_map)
    result = (
        Result.ok(10)
        .and_then(lambda x: Result.ok(x + 5))
        .and_then(lambda x: Result.ok(x * 2))
        .map(lambda x: x - 10)
    )
    print(f"Chained result: {result.unwrap()}")  # 20
    
    # Pattern matching with match_result
    def process_result(r: Result):
        return r.match_result(
            on_ok=lambda val: f"Success: {val}",
            on_err=lambda err: f"Error: {err}"
        )
    
    print(process_result(success))  # "Success: 42"
    print(process_result(failure))  # "Error: Something went wrong"


def option_monad_examples():
    """Demonstrate Option<T> monad usage."""
    print("\n" + "=" * 60)
    print("Option Monad Examples")
    print("=" * 60)
    
    # Create Option values
    some_value = Option.some(100)
    nothing = Option.nothing()
    
    # Check status
    print(f"some_value.is_some(): {some_value.is_some()}")  # True
    print(f"nothing.is_none(): {nothing.is_none()}")  # True
    
    # Unwrap values safely
    print(f"some_value.unwrap(): {some_value.unwrap()}")  # 100
    print(f"nothing.unwrap_or(0): {nothing.unwrap_or(0)}")  # 0
    
    # Map over Option (only applies to Some values)
    incremented = some_value.map(lambda x: x + 1)
    print(f"incremented.unwrap(): {incremented.unwrap()}")  # 101
    
    # Filter Option with predicate
    even_only = some_value.filter(lambda x: x % 2 == 0)
    print(f"even filter on 100: {even_only.is_some()}")  # True
    
    odd_only = some_value.filter(lambda x: x % 2 != 0)
    print(f"odd filter on 100: {odd_only.is_none()}")  # True
    
    # Chain operations with flat_map
    result = (
        Option.some(5)
        .flat_map(lambda x: Option.some(x * 2))
        .flat_map(lambda x: Option.some(x + 10))
        .map(lambda x: x / 2)
    )
    print(f"Chained option: {result.unwrap()}")  # 10.0
    
    # Pattern matching with match_option
    def process_option(opt: Option):
        return opt.match_option(
            on_some=lambda val: f"Found: {val}",
            on_nothing=lambda: "Nothing here"
        )
    
    print(process_option(some_value))  # "Found: 100"
    print(process_option(nothing))  # "Nothing here"


def thunk_monad_examples():
    """Demonstrate Thunk (lazy evaluation) monad usage."""
    print("\n" + "=" * 60)
    print("Thunk Monad Examples - Lazy Evaluation")
    print("=" * 60)
    
    # Create expensive computation (not evaluated yet)
    counter = {"count": 0}
    
    def expensive_computation():
        counter["count"] += 1
        print(f"  Computing... (call #{counter['count']})")
        result = sum(range(1000000))
        return result
    
    lazy = Thunk(expensive_computation)
    
    print(f"Thunk created, evaluated? {lazy.is_evaluated()}")  # False
    
    # Force evaluation (memoized)
    print("Forcing evaluation...")
    result1 = lazy.force()
    print(f"Result: {result1}")
    print(f"Is evaluated now? {lazy.is_evaluated()}")  # True
    
    # Second force() returns cached value (no recomputation)
    print("Forcing again...")
    result2 = lazy.force()
    print(f"Same result: {result2}")
    print(f"Computation only ran {counter['count']} time(s)")  # 1
    
    # Map over thunk (creates new lazy computation)
    lazy_doubled = lazy.map(lambda x: x * 2)
    print(f"Mapped thunk evaluated? {lazy_doubled.is_evaluated()}")  # False
    print(f"Forcing mapped thunk: {lazy_doubled.force()}")


def safe_dataframe_operations():
    """Demonstrate monads with DataFrame operations."""
    print("\n" + "=" * 60)
    print("Safe DataFrame Operations with Monads")
    print("=" * 60)
    
    # Simulated DataFrame operations returning Result
    # In real Polarway, these would come from the server
    
    # Safe division avoiding division by zero
    def safe_divide(a: float, b: float) -> Result:
        if b == 0:
            return Result.err("Division by zero")
        return Result.ok(a / b)
    
    # Railway-oriented programming
    pipeline = (
        safe_divide(100, 5)
        .and_then(lambda x: safe_divide(x, 4))
        .and_then(lambda x: safe_divide(x, 2))
        .map(lambda x: f"Final result: {x}")
    )
    
    print(pipeline.unwrap())  # "Final result: 2.5"
    
    # Error short-circuits the pipeline
    error_pipeline = (
        safe_divide(100, 5)
        .and_then(lambda x: safe_divide(x, 0))  # Error here
        .and_then(lambda x: safe_divide(x, 2))  # Never executed
        .map(lambda x: f"Final result: {x}")    # Never executed
    )
    
    print(f"Error: {error_pipeline.err_value()}")  # "Division by zero"
    
    # Safe optional value extraction
    def safe_get_first(values: list) -> Option:
        if len(values) == 0:
            return Option.nothing()
        return Option.some(values[0])
    
    prices = [100.5, 101.2, 99.8]
    first_price = safe_get_first(prices)
    
    markup = first_price.match_option(
        on_some=lambda price: price * 1.1,
        on_nothing=lambda: 0.0
    )
    
    print(f"Price with 10% markup: {markup}")  # 110.55


def main():
    """Run all examples."""
    result_monad_examples()
    option_monad_examples()
    thunk_monad_examples()
    safe_dataframe_operations()
    
    print("\n" + "=" * 60)
    print("Summary")
    print("=" * 60)
    print("""
Polarway's monads are Rust types exposed via PyO3:
- Result<T, E>: Safe error handling (no exceptions)
- Option<T>: Safe null handling (no None checks)
- Thunk<T>: Lazy evaluation with memoization

Benefits:
✅ Zero-cost abstractions (Rust performance)
✅ Type-safe operations
✅ Composable with .map(), .and_then(), .flat_map()
✅ Pattern matching with match_result/match_option
✅ Railway-oriented programming (errors short-circuit)
✅ No silent failures or None/NaN corruption
    """)


if __name__ == "__main__":
    main()
