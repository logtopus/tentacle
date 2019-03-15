use failure::Fail;
use std::sync::Arc;

#[derive(Fail, Debug)]
pub enum ApplicationError {
    // indicates that a requested log source is not configured
    #[fail(display = "Source not found")]
    SourceNotFound,
    // indicates that a requested log source is configured but cannot be read
    #[fail(display = "Failed to read source")]
    FailedToReadSource,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ParsedLine {
    pub timestamp: u128,
    pub loglevel: Option<String>,
    pub message: String,
}

#[derive(Debug)]
pub struct StreamEntry {
    pub line: String,
    pub parsed_line: ParsedLine,
}

#[derive(Debug)]
pub struct LogFilter {
    pub from_ms: u128,
    pub loglevels: Option<Vec<String>>,
}

impl LogFilter {
    /// Check the filter against the log line.
    /// If the logline matches the filter and should be included within the output
    /// the this returns true.
    pub fn matches(&self, parsed_line: &ParsedLine) -> bool {
        if parsed_line.timestamp >= self.from_ms {
            match &self.loglevels {
                Some(filter) => match &parsed_line.loglevel {
                    Some(loglvl) => filter.iter().any(|f| f == loglvl),
                    None => false,
                },
                None => true, // no filter, everything matches
            }
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct LinePattern {
    pub raw: String,
    pub grok: Arc<grok::Pattern>,
    pub chrono: Arc<String>,
    pub timezone: chrono_tz::Tz,
    pub syslog_ts: bool, // indicates if the grok pattern is matching a syslog timestamp without year
}
