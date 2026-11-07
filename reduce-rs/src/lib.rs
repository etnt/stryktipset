//! Fast Rust port of `tools/stryktipset/reduce_system.py`.
//!
//! Same CLI, same output format, same guarantees. The RNG stream differs
//! from NumPy's PCG64 seeding, so the same `--seed` gives a different (but
//! equally guaranteed) result; see README.md.
//!
//! Feature flags:
//! - `cli` (default): the command-line front end.
//! - `parallel` (default): rayon for the neighbour table, greedy restarts
//!   and the hit distribution. The serial build must give byte-identical
//!   output (enforced by `tests/golden.sh` and `tests/cli_goldens.rs`).
//! - `web`: JSON helpers for the wasm wrapper in `web/`.

pub mod anneal;
#[cfg(feature = "cli")]
pub mod cli;
pub mod clock;
pub mod engine;
pub mod eval;
pub mod gformat;
pub mod greedy;
pub mod neighbours;
pub mod output;
pub mod pad;
pub mod parallel;
pub mod system;
