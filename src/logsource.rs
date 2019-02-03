use config::ConfigError;
use config::Value;

use regex::Regex;
use grok::Grok;

use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
pub enum LogSourceType {
    File,
    Journal, // see https://docs.rs/systemd/0.0.8/systemd/journal/index.html
}

#[derive(Debug, Clone)]
pub enum LogSource {
    File { id: String, file_pattern: Regex, line_pattern: String, grok_pattern: Arc<grok::Pattern> },
    Journal { id: String, unit: String, line_pattern: String, grok_pattern: Arc<grok::Pattern> },
}

impl LogSource {
    pub fn try_from_config(value: &Value, grok: &mut Grok) -> Result<LogSource, ConfigError> {
        use regex::Regex;

        let file_map = value.clone().into_table()?;

        let id = file_map.get("id").ok_or(ConfigError::NotFound("id".to_string()))?;
        let id = id.clone().into_str()?;
        let line_pattern = file_map.get("line_pattern").ok_or(ConfigError::NotFound("line_pattern".to_string()))?.clone().into_str()?;
        let grok_pattern = grok.compile(&line_pattern, true).map_err(|e| {
            error!("{}", e);
            ConfigError::Message("Failed to parse line pattern".to_string())
        })?;

        let srctype = file_map.get("type").ok_or(ConfigError::NotFound("type".to_string()))?.clone().into_str()?;

        match srctype.as_ref() {
            "log" => {
                let file_pattern = file_map.get("file_pattern").ok_or(ConfigError::NotFound("file_pattern".to_string()))?.clone().into_str()?;
                let file_pattern = Regex::new(file_pattern.as_ref()).unwrap();

                Ok(LogSource::File { id, file_pattern, line_pattern, grok_pattern: Arc::new(grok_pattern) })
            }
            "journal" => {
                let unit = file_map.get("unit").ok_or(ConfigError::NotFound("unit".to_string()))?.clone().into_str()?;

                Ok(LogSource::Journal { id, unit, line_pattern, grok_pattern: Arc::new(grok_pattern) })
            }
            e => Err(ConfigError::Message(format!("Unknown type: {}", e)))
        }
    }

//    pub fn try_from_spec(dto: LogSourceSpec) -> Result<LogSource, ApplicationError> {
//        match dto.src_type {
//            LogSourceType::File => {
//                match dto.path {
//                    Some(p) => Ok(LogSource::File {
//                        folder: p
//                    }),
//                    None => Err(ApplicationError::MissingAttribute { attr: "path".to_string() })
//                }
//            }
//            LogSourceType::Journal =>
//                unimplemented!()
//        }
//    }
}

#[cfg(test)]
mod tests {
    use crate::logsource::LogSource;
    use grok::Grok;

    #[test]
    fn test_fileconfig_conversion() {
        let mut cfg = config::Config::default();
        let cfg = cfg.merge(config::File::with_name("tests/testconf.yml")).unwrap();
        let vec = cfg.get_array("sources").unwrap();
        let entry1 = vec.get(0).unwrap();
        let entry2 = vec.get(1).unwrap();
        let mut grok = Grok::default();
        let src1 = LogSource::try_from_config(entry1, &mut grok).unwrap();
        let src2 = LogSource::try_from_config(entry2, &mut grok).unwrap();

        match src1 {
            LogSource::File { id, file_pattern, line_pattern, grok_pattern } => {
                assert_eq!(id, "test-log");
                assert_eq!(file_pattern.as_str(), r#"demo\.log(\.\d(\.gz)?)?"#);
                assert_eq!(line_pattern, "%{TIMESTAMP_ISO8601:timestamp} %{GREEDYDATA:msg}");

                let matches = grok_pattern.match_against("2007-08-31T16:47+00:00 Some message").unwrap();
                assert_eq!(matches.get("timestamp"), Some("2007-08-31T16:47+00:00"));
                assert_eq!(matches.get("msg"), Some("Some message"));
            }
            f => panic!("Conversion from file config failed: {:?}", f)
        }

        match src2 {
            LogSource::Journal { id, unit, line_pattern, grok_pattern } => {
                assert_eq!(id, "test-journal");
                assert_eq!(unit.as_str(), "demo");
                assert_eq!(line_pattern, "%{TIMESTAMP_ISO8601:timestamp} Message: %{GREEDYDATA:message}");
                
                let matches = grok_pattern.match_against("2007-08-31T16:47+00:00 Message: Some message")
                    .expect("No matches found");
                assert_eq!(matches.get("timestamp"), Some("2007-08-31T16:47+00:00"));
                assert_eq!(matches.get("message"), Some("Some message"));
            }
            f => panic!("Conversion from journal config failed: {:?}", f)
        }
    }
}