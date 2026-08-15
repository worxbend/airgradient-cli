//! The crate's library surface, shared by the binary and the integration tests.
//!
//! Each module owns one boundary: [`device`] the network, [`config`] the
//! config file, [`sensors`] the payload-to-reading translation, [`output`] the
//! one-shot renderers, [`tui`] the interactive dashboard, and [`cli`] the
//! argument parsing that ties them together.

pub mod cli;
pub mod config;
pub mod device;
pub mod output;
pub mod sensors;
pub mod tui;
