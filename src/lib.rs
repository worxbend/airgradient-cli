//! The crate's library surface, shared by the binary and the integration tests.
//!
//! Each module owns one boundary: [`device`] the network, [`config`] the
//! config file, [`sensors`] the payload-to-reading translation, [`output`] the
//! one-shot renderers, [`tui`] the interactive dashboard, and [`cli`] the
//! argument parsing that ties them together.
//!
//! [`device_url`] sits below all of them: it is the rule about what a device
//! address is, depended on by both the module that fetches from one and the
//! module that stores one, so that neither depends on the other.

pub mod cli;
pub mod config;
pub mod device;
pub mod device_url;
pub mod output;
pub mod sensors;
pub mod tui;
