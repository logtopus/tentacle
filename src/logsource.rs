use config::ConfigError;
use config::Value;

use grok::Grok;
use regex::Regex;

use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
pub enum LogSourceType {
    File,
    Journal, // see https://docs.rs/systemd/0.0.8/systemd/journal/index.html
}

#[derive(Debug, Clone)]
pub struct LinePattern {
    pub raw: String,
    pub grok: Arc<grok::Pattern>,
    pub chrono: Arc<String>,
    pub syslog_ts: bool, // indicates if the grok pattern is matching a syslog timestamp without year
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ParsedLine {
    pub timestamp: i64,
    pub message: String,
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

impl LogSource {
    fn apply_pattern(line: &str, line_pattern: &LinePattern, year: i32) -> ParsedLine {
        let maybe_matches = line_pattern.grok.match_against(&line);
        if let Some(matches) = maybe_matches {
            let timestamp = matches
                .get("timestamp")
                .map(|ts| {
                    let parse_result = if line_pattern.syslog_ts {
                        let ts_w_year = format!("{} {}", year, ts);
                        chrono::NaiveDateTime::parse_from_str(&ts_w_year, &line_pattern.chrono)
                    } else {
                        chrono::NaiveDateTime::parse_from_str(&ts, &line_pattern.chrono)
                    };
                    parse_result
                        .map(|dt| dt.timestamp() * 1000 + (dt.timestamp_subsec_millis() as i64))
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            ParsedLine {
                timestamp,
                message: matches.get("message").unwrap_or("").to_string(),
            }
        } else {
            ParsedLine {
                timestamp: 0,
                message: format!("Failed to parse: {}", line),
            }
        }
    }

    pub fn parse_line(&self, line: &str, year: i32) -> ParsedLine {
        match *self {
            LogSource::File {
                id: _,
                file_pattern: _,
                ref line_pattern,
            } => LogSource::apply_pattern(line, &line_pattern, year),
            LogSource::Journal {
                id: _,
                unit: _,
                ref line_pattern,
            } => LogSource::apply_pattern(line, &line_pattern, year),
        }
    }

    pub fn try_from_config(value: &Value, grok: &mut Grok) -> Result<LogSource, ConfigError> {
        use regex::Regex;

        let file_map = value.clone().into_table()?;

        let id = file_map
            .get("id")
            .ok_or(ConfigError::NotFound("id".to_string()))?;
        let id = id.clone().into_str()?;
        let line_pattern = file_map
            .get("line_pattern")
            .ok_or(ConfigError::NotFound("line_pattern".to_string()))?
            .clone()
            .into_str()?;
        let grok_pattern = grok.compile(&line_pattern, true).map_err(|e| {
            error!("{}", e);
            ConfigError::Message("Failed to parse line pattern".to_string())
        })?;
        // special case: add year from file time if syslog pattern is used
        let syslog_ts = if line_pattern.contains("%{SYSLOGTIMESTAMP:timestamp}") {
            true
        } else {
            false
        };

        let chrono_pattern = file_map
            .get("datetime_pattern")
            .ok_or(ConfigError::NotFound("datetime_pattern".to_string()))?
            .clone()
            .into_str()?;
        let chrono_pattern = if syslog_ts {
            format!("%Y {}", chrono_pattern)
        } else {
            chrono_pattern
        };

        let srctype = file_map
            .get("type")
            .ok_or(ConfigError::NotFound("type".to_string()))?
            .clone()
            .into_str()?;

        match srctype.as_ref() {
            "log" => {
                let file_pattern = file_map
                    .get("file_pattern")
                    .ok_or(ConfigError::NotFound("file_pattern".to_string()))?
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
                        syslog_ts,
                    },
                })
            }
            "journal" => {
                let unit = file_map
                    .get("unit")
                    .ok_or(ConfigError::NotFound("unit".to_string()))?
                    .clone()
                    .into_str()?;

                Ok(LogSource::Journal {
                    id,
                    unit,
                    line_pattern: LinePattern {
                        raw: line_pattern,
                        grok: Arc::new(grok_pattern),
                        chrono: Arc::new(chrono_pattern),
                        syslog_ts,
                    },
                })
            }
            e => Err(ConfigError::Message(format!("Unknown type: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::logsource::LinePattern;
    use crate::logsource::LogSource;
    use grok::Grok;
    use std::sync::Arc;

    #[test]
    fn test_fileconfig_conversion() {
        let mut cfg = config::Config::default();
        let cfg = cfg
            .merge(config::File::with_name("tests/testconf.yml"))
            .unwrap();
        let vec = cfg.get_array("sources").unwrap();
        let entry1 = vec.get(0).unwrap();
        let entry2 = vec.get(1).unwrap();
        let mut grok = Grok::default();
        let src1 = LogSource::try_from_config(entry1, &mut grok).unwrap();
        let src2 = LogSource::try_from_config(entry2, &mut grok).unwrap();

        match src1 {
            LogSource::File {
                id,
                file_pattern,
                line_pattern,
            } => {
                assert_eq!(id, "test-log");
                assert_eq!(file_pattern.as_str(), r#"demo\.log(\.\d(\.gz)?)?"#);
                assert_eq!(
                    line_pattern.raw,
                    "%{TIMESTAMP_ISO8601:timestamp} %{GREEDYDATA:msg}"
                );
                assert_eq!(line_pattern.chrono.as_ref(), "%b %d %H:%M:%S");

                let matches = line_pattern
                    .grok
                    .match_against("2007-08-31T16:47+00:00 Some message")
                    .unwrap();
                assert_eq!(matches.get("timestamp"), Some("2007-08-31T16:47+00:00"));
                assert_eq!(matches.get("msg"), Some("Some message"));
            }
            f => panic!("Conversion from file config failed: {:?}", f),
        }

        match src2 {
            LogSource::Journal {
                id,
                unit,
                line_pattern,
            } => {
                assert_eq!(id, "test-journal");
                assert_eq!(unit.as_str(), "demo");
                assert_eq!(
                    line_pattern.raw,
                    "%{TIMESTAMP_ISO8601:timestamp} Message: %{GREEDYDATA:message}"
                );
                assert_eq!(line_pattern.chrono.as_ref(), "%b %d %H:%M:%S");

                let matches = line_pattern
                    .grok
                    .match_against("2007-08-31T16:47+00:00 Message: Some message")
                    .expect("No matches found");
                assert_eq!(matches.get("timestamp"), Some("2007-08-31T16:47+00:00"));
                assert_eq!(matches.get("message"), Some("Some message"));
            }
            f => panic!("Conversion from journal config failed: {:?}", f),
        }
    }

    #[test]
    fn test_apply_pattern() {
        let mut grok_default = grok::Grok::default();

        let raw = "%{TIMESTAMP_ISO8601:timestamp} %{GREEDYDATA:message}";
        let grok = Arc::new(grok_default.compile(raw, true).unwrap());
        let chrono = Arc::new("%Y-%m-%d %H:%M:%S".to_string());
        let line_pattern = LinePattern {
            raw: raw.to_string(),
            grok,
            chrono: chrono.clone(),
            syslog_ts: false,
        };

        let line = "2018-01-01 12:39:01 first message";
        let parsed_line = LogSource::apply_pattern(line, &line_pattern, 2018);
        assert_eq!(1514810341000, parsed_line.timestamp);
        assert_eq!("first message", parsed_line.message);

        let grok = Arc::new(
            grok_default
                .compile("%{SYSLOGTIMESTAMP:timestamp} %{GREEDYDATA:message}", true)
                .unwrap(),
        );
        let chrono = Arc::new("%Y %b %d %H:%M:%S".to_string());
        let line_pattern = LinePattern {
            raw: raw.to_string(),
            grok,
            chrono: chrono.clone(),
            syslog_ts: true,
        };

        let line = "Feb 28 13:29:46 second message";
        let parsed_line = LogSource::apply_pattern(line, &line_pattern, 2019);
        assert_eq!(1551360586000, parsed_line.timestamp);
        assert_eq!("second message", parsed_line.message);
    }
}
