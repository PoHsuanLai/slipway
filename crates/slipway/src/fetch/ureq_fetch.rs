//! The real downloader: ureq over HTTPS, blocking (run it off the async runtime).

use super::{Download, Fetch, FetchError, skip};
use std::time::Duration;

/// How long a connection may take to answer: the connection, then the response headers.
const ANSWER_WITHIN: Duration = Duration::from_secs(60);
/// The longest one connection is read for. A connection that brings nothing in this time is found
/// stalled; one that brought data is simply continued by a range request, so a slow link is never given
/// up on, and there is no cap on the whole download.
const READ_WINDOW: Duration = Duration::from_secs(120);

/// Downloads with ureq; HTTPS only.
#[derive(Clone, Debug)]
pub struct UreqFetch {
    agent: ureq::Agent,
}

impl UreqFetch {
    /// An agent with a connect time limit and a limit on how long one connection is read.
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(ANSWER_WITHIN))
            .timeout_recv_body(Some(READ_WINDOW))
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

fn failed(error: ureq::Error) -> FetchError {
    match error {
        ureq::Error::StatusCode(code) => FetchError::Refused(code),
        other => FetchError::Unreachable(other.to_string()),
    }
}

impl Fetch for UreqFetch {
    fn open(&self, url: &str) -> Result<Download, FetchError> {
        self.open_from(url, 0)
    }

    fn open_from(&self, url: &str, offset: u64) -> Result<Download, FetchError> {
        let mut request = self.agent.get(url);
        if offset > 0 {
            request = request.header("Range", format!("bytes={offset}-"));
        }
        let response = request.call().map_err(failed)?;
        let resumed = response.status().as_u16() == 206;
        let (_, body) = response.into_parts();
        let length = body.content_length();
        let mut reader = body.into_reader();
        if offset > 0 && !resumed {
            // The site ignored the range and sent the file from its start.
            skip(&mut reader, offset)?;
        }
        Ok(Download {
            length,
            body: Box::new(reader),
        })
    }
}
