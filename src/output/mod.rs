//! One-shot output renderers and the metadata they share.
//!
//! [`json`], [`text`], and [`waybar`] render the same snapshot for different
//! audiences — another program, a person at a terminal, and a status bar — and
//! take their surrounding facts (which device, how long the fetch took)
//! through [`OutputMetadata`] so those facts cannot drift apart.

pub mod json;
pub mod text;
pub mod waybar;

use std::time::Duration;

#[derive(Debug, Clone, Copy, Default)]
pub struct OutputMetadata<'a> {
    pub device_url: Option<&'a str>,
    pub last_update: Option<&'a str>,
    pub fetch_duration: Option<Duration>,
}
