pub mod json;
pub mod text;

use std::time::Duration;

#[derive(Debug, Clone, Copy, Default)]
pub struct OutputMetadata<'a> {
    pub device_url: Option<&'a str>,
    pub last_update: Option<&'a str>,
    pub fetch_duration: Option<Duration>,
}
