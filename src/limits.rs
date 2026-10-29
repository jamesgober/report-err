//! Budgets on everything that can grow (PLAN 7.18). The defaults apply from
//! `0.3.0`; `configure` makes them settable once per process in `0.5.0`.

/// Context layers kept per error. Past it, the oldest layer above the
/// innermost eight is dropped and counted.
pub(crate) const MAX_FRAMES: usize = 64;

/// Links walked in one chain. Also stops a `source()` that never ends.
pub(crate) const MAX_CHAIN: usize = 128;

/// Attachments kept per error; past it a new one is dropped and counted.
pub(crate) const MAX_ATTACHMENTS: usize = 32;

/// Sections kept per error; past it a new one is dropped and counted.
pub(crate) const MAX_SECTIONS: usize = 32;

/// Text and value bytes one error's extension block holds: formatted
/// sections, display attachments, and the shallow size of attached values.
pub(crate) const MAX_BYTES: usize = 64 * 1024;

/// Bytes one guarded rendering writes (`ExitReport`); past it the report is
/// cut with `(report truncated)`.
#[cfg_attr(not(feature = "std"), allow(dead_code))]
pub(crate) const MAX_RENDER: usize = 1024 * 1024;
