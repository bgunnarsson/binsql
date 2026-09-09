//! binsql: a terminal front end and a command mode over the same core.
//!
//! Exposed as a library so the layout can be rendered against a test backend —
//! a TUI that is only reachable through `main` is a TUI nobody can assert on —
//! and so command mode can be driven the same way.

pub mod app;
pub mod cli;
pub mod theme;
pub mod ui;
