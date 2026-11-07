//! Monotonic wall-clock milliseconds, usable on native and wasm32 targets.
//!
//! `std::time::Instant::now()` panics on `wasm32-unknown-unknown`, so the
//! engine takes deadlines in "milliseconds from `clock::now_ms`" instead.
//! Native builds measure from process start; wasm builds use `Date.now()`.

#[cfg(not(target_arch = "wasm32"))]
pub fn now_ms() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static BASE: OnceLock<Instant> = OnceLock::new();
    let base = BASE.get_or_init(Instant::now);
    base.elapsed().as_secs_f64() * 1000.0
}

#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> f64 {
    js_sys::Date::now()
}
