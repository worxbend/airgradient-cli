//! The interactive dashboard.
//!
//! Split by concern: [`app`] is the state machine (no I/O), [`ui`] draws a
//! frame from that state (no state of its own), [`runtime`] owns the terminal,
//! the clock, and the network, [`theme`] the colours, and [`command`] the
//! parsing of what is typed into the `:` palette.

pub mod app;
pub mod command;
pub mod runtime;
pub mod theme;
pub mod ui;
