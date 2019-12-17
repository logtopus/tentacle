use config;
use derive_more::Display;
use futures::stream::LocalBoxStream;
use grok;
use regex::Regex;
use std::sync::Arc;

#[derive(Debug, Display)]
pub enum ApplicationError {
    // indicates that a requested log source is not configured
    #[display(fmt = "Source not found")]
    SourceNotFound,
    // indicates that a requested log source is configured but cannot be read
    #[display(fmt = "Failed to read source")]
    FailedToReadSource,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ParsedLine {
    pub timestamp: u128,
    pub loglevel: Option<String>,
    pub message: String,
}

#[derive(Debug)]
pub enum StreamEntry {
    LogLine {
        line: String,
        parsed_line: ParsedLine,
    },
}

pub type LogStream = LocalBoxStream<'static, Result<StreamEntry, ApplicationError>>;

#[derive(Debug)]
pub struct LogQueryContext {
    pub from_ms: u128,
    pub loglevels: Option<Vec<String>>,
    pub watch: Option<bool>,
}

impl LogQueryContext {
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

#[derive(Debug, Clone)]
pub enum LogSource {
    File {
        id: String,
        file_pattern: Regex,
        line_pattern: LinePattern,
    },
    Journal {
        id: String,
        unit: String,
        line_pattern: LinePattern,
    },
}

pub struct LogSourceBuilder;

impl LogSourceBuilder {
    pub fn create(
        value: &config::Value,
        grok: &mut grok::Grok,
    ) -> Result<LogSource, config::ConfigError> {
        let file_map = value.clone().into_table()?;

        let id = file_map
            .get("id")
            .ok_or(config::ConfigError::NotFound("id".to_string()))?;
        let id = id.clone().into_str()?;
        let line_pattern = file_map
            .get("line_pattern")
            .ok_or(config::ConfigError::NotFound("line_pattern".to_string()))?
            .clone()
            .into_str()?;
        let grok_pattern = grok.compile(&line_pattern, true).map_err(|e| {
            error!("{}", e);
            config::ConfigError::Message("Failed to parse line pattern".to_string())
        })?;
        // special case: add year from file time if syslog pattern is used
        let syslog_ts = if line_pattern.contains("%{SYSLOGTIMESTAMP:timestamp}") {
            true
        } else {
            false
        };

        let chrono_pattern = file_map
            .get("datetime_pattern")
            .ok_or(config::ConfigError::NotFound(
                "datetime_pattern".to_string(),
            ))?
            .clone()
            .into_str()?;
        let chrono_pattern = if syslog_ts {
            format!("%Y {}", chrono_pattern)
        } else {
            chrono_pattern
        };

        let timezone = file_map
            .get("timezone")
            .ok_or(config::ConfigError::NotFound("timezone".to_string()))?
            .clone()
            .into_str()?;
        let timezone: chrono_tz::Tz = timezone.parse().unwrap();

        let srctype = file_map
            .get("type")
            .ok_or(config::ConfigError::NotFound("type".to_string()))?
            .clone()
            .into_str()?;

        let logsource = match srctype.as_ref() {
            "journal" => {
                let unit = file_map
                    .get("unit")
                    .ok_or(config::ConfigError::NotFound("unit".to_string()))?
                    .clone()
                    .into_str()?;

                Ok(LogSource::Journal {
                    id,
                    unit,
                    line_pattern: LinePattern {
                        raw: line_pattern,
                        grok: Arc::new(grok_pattern),
                        chrono: Arc::new(chrono_pattern),
                        timezone,
                        syslog_ts,
                    },
                })
            }
            "file" => {
                let file_pattern = file_map
                    .get("file_pattern")
                    .ok_or(config::ConfigError::NotFound("file_pattern".to_string()))?
                    .clone()
                    .into_str()?;
                let file_pattern = Regex::new(file_pattern.as_ref()).unwrap();

                Ok(LogSource::File {
                    id,
                    file_pattern,
                    line_pattern: LinePattern {
                        raw: line_pattern,
                        grok: Arc::new(grok_pattern),
                        chrono: Arc::new(chrono_pattern),
                        timezone,
                        syslog_ts,
                    },
                })
            }
            e => Err(config::ConfigError::Message(format!("Unknown type: {}", e))),
        };

        logsource
    }
}
