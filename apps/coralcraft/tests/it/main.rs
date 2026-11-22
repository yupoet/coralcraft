//! Integration tests: one test binary for the crate, one module per area.
// Unwrapping and panicking on failure is fine here, unlike in shipped code (AGENTS.md › Robustness).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod automation_args;
mod help;
// CoralCraft (P5 Task 4): the real crash/restart recovery acceptance.
mod recovery_restart;
