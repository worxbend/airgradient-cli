//! The crate's library surface, shared by the binary and the integration tests.
//!
//! Each module owns one boundary: [`device`] the network, [`config`] the
//! config file, [`sensors`] the payload-to-reading translation, [`output`] the
//! one-shot renderers, [`tui`] the interactive dashboard, and [`cli`] the
//! argument parsing that ties them together.
//!
//! [`device_url`] and [`refresh_interval`] sit below all of them: they are the
//! rules about what a device address and a refresh cadence are, depended on by
//! both the modules that use one and the module that stores one, so that
//! neither depends on the other.

mod atomic_file;
pub mod cli;
pub mod config;
pub mod device;
pub mod device_url;
pub mod history;
pub mod output;
pub mod refresh_interval;
pub mod sensors;
pub mod tui;
