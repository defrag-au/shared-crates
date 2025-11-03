//! Platform-agnostic timing utilities for performance instrumentation
//!
//! These macros provide consistent timing across WASM (Cloudflare Workers) and native targets.
//! On WASM, they use `js_sys::Date::now()` which returns milliseconds since epoch.
//! On native targets, they use `std::time::Instant` for accurate timing.
//!
//! # Examples
//!
//! ```
//! use worker_utils::timing::{timer_start, timer_elapsed_ms};
//!
//! let start = timer_start!();
//! // ... do some work ...
//! let elapsed = timer_elapsed_ms!(start);
//! println!("Operation took {:.2}ms", elapsed);
//! ```

/// Start a platform-agnostic timer
///
/// Returns `f64` (milliseconds since epoch) on WASM, `Instant` on native.
///
/// # Example
/// ```
/// use worker_utils::timing::timer_start;
///
/// let start = timer_start!();
/// // ... do work ...
/// ```
#[macro_export]
macro_rules! timer_start {
    () => {{
        #[cfg(target_arch = "wasm32")]
        {
            js_sys::Date::now()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::time::Instant::now()
        }
    }};
}

/// Calculate elapsed milliseconds from a timer started with `timer_start!()`
///
/// Returns `f64` milliseconds on all platforms.
///
/// # Example
/// ```
/// use worker_utils::timing::{timer_start, timer_elapsed_ms};
///
/// let start = timer_start!();
/// // ... do work ...
/// let elapsed_ms = timer_elapsed_ms!(start);
/// println!("Took {:.2}ms", elapsed_ms);
/// ```
#[macro_export]
macro_rules! timer_elapsed_ms {
    ($start:expr) => {{
        #[cfg(target_arch = "wasm32")]
        {
            js_sys::Date::now() - $start
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            $start.elapsed().as_secs_f64() * 1000.0
        }
    }};
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_timer_macros() {
        let start = timer_start!();

        // Small sleep to ensure time passes
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::sleep(std::time::Duration::from_millis(10));

        let elapsed = timer_elapsed_ms!(start);

        #[cfg(not(target_arch = "wasm32"))]
        assert!(
            elapsed >= 10.0,
            "Expected at least 10ms, got {:.2}ms",
            elapsed
        );

        #[cfg(target_arch = "wasm32")]
        assert!(elapsed >= 0.0, "Elapsed time should be non-negative");
    }
}
