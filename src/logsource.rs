use config::ConfigError;
use config::Value;

use grok::Grok;
use regex::Regex;
use std::sync::Arc;

use crate::data::LinePattern;

#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
pub enum LogSourceType {
    File,
    Journal, // see https://docs.rs/systemd/0.0.8/systemd/journal/index.html
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
    pub fn try_from_config(value: &Value, grok: &mut Grok) -> Result<LogSource, ConfigError> {
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

        let timezone = file_map
            .get("timezone")
            .ok_or(ConfigError::NotFound("timezone".to_string()))?
            .clone()
            .into_str()?;
        let timezone: chrono_tz::Tz = timezone.parse().unwrap();

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
                        timezone,
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
                        timezone,
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
    use crate::logsource::LogSource;
    use grok::Grok;

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
                    "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:loglevel} %{GREEDYDATA:msg}"
                );
                assert_eq!(line_pattern.chrono.as_ref(), "%b %d %H:%M:%S");

                let matches = line_pattern
                    .grok
                    .match_against("2007-08-31T16:47+00:00 DEBUG Some message")
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

}
