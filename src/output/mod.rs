//! One-shot output renderers and the metadata they share.
//!
//! [`json`] and [`text`] render the same snapshot for two different audiences —
//! other programs and a person at a terminal — and both take their surrounding
//! facts (which device, how long the fetch took) through [`OutputMetadata`] so
//! the two cannot drift apart.

pub mod json;
pub mod text;

use std::time::Duration;

#[derive(Debug, Clone, Copy, Default)]
pub struct OutputMetadata<'a> {
    pub device_url: Option<&'a str>,
    pub last_update: Option<&'a str>,
    pub fetch_duration: Option<Duration>,
}
