//! The real downloader: ureq over HTTPS, blocking (run it off the async runtime).

use super::{Download, Fetch, FetchError};
use std::time::Duration;

/// Downloads with ureq; HTTPS only.
#[derive(Clone, Debug)]
pub struct UreqFetch {
    agent: ureq::Agent,
}

impl UreqFetch {
    /// An agent with connect and overall time limits.
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_global(Some(Duration::from_secs(3600)))
            .user_agent("slipway")
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

impl Default for UreqFetch {
    fn default() -> Self {
        Self::new()
    }
}

impl Fetch for UreqFetch {
    fn open(&self, url: &str) -> Result<Download, FetchError> {
        let response = self.agent.get(url).call().map_err(|e| match e {
            ureq::Error::StatusCode(code) => FetchError::Refused(code),
            other => FetchError::Unreachable(other.to_string()),
        })?;
        let (_, body) = response.into_parts();
        let length = body.content_length();
        Ok(Download {
            length,
            body: Box::new(body.into_reader()),
        })
    }
}
