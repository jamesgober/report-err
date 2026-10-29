//! Budgets on everything that can grow (PLAN 7.18). The defaults apply from
//! `0.3.0`; `configure` makes them settable once per process in `0.5.0`.

/// Context layers kept per error. Past it, the oldest layer above the
/// innermost eight is dropped and counted.
pub(crate) const MAX_FRAMES: usize = 64;

/// Links walked in one chain. Also stops a `source()` that never ends.
pub(crate) const MAX_CHAIN: usize = 128;
