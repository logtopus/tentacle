use config::ConfigError;
use config::Value;

use regex::Regex;

#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
pub enum LogSourceType {
    File,
    Journal, // see https://docs.rs/systemd/0.0.8/systemd/journal/index.html
}

#[derive(Debug, Clone)]
pub enum LogSource {
    File { id: String, file_pattern: Regex, line_pattern: String },
    Journal { id: String, unit: String, line_pattern: String },
}

impl LogSource {
    pub fn try_from_config(value: &Value) -> Result<LogSource, ConfigError> {
        use regex::Regex;

        let file_map = value.clone().into_table()?;

        let id = file_map.get("id").ok_or(ConfigError::NotFound("id".to_string()))?;
        let id = id.clone().into_str()?;
        let line_pattern = file_map.get("line_pattern").ok_or(ConfigError::NotFound("line_pattern".to_string()))?.clone().into_str()?;

        let srctype = file_map.get("type").ok_or(ConfigError::NotFound("type".to_string()))?.clone().into_str()?;

        match srctype.as_ref() {
            "log" => {
                let file_pattern = file_map.get("file_pattern").ok_or(ConfigError::NotFound("file_pattern".to_string()))?.clone().into_str()?;
                let file_pattern = Regex::new(file_pattern.as_ref()).unwrap();

                Ok(LogSource::File { id: id, file_pattern: file_pattern, line_pattern: line_pattern })
            }
            "journal" => {
                let unit = file_map.get("unit").ok_or(ConfigError::NotFound("unit".to_string()))?.clone().into_str()?;

                Ok(LogSource::Journal { id: id, unit: unit, line_pattern: line_pattern })
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

    #[test]
    fn test_fileconfig_conversion() {
        let mut cfg = config::Config::default();
        let cfg = cfg.merge(config::File::with_name("tests/testconf.yml")).unwrap();
        let vec = cfg.get_array("sources").unwrap();
        let entry1 = vec.get(0).unwrap();
        let entry2 = vec.get(1).unwrap();
        let src1 = LogSource::try_from_config(entry1).unwrap();
        let src2 = LogSource::try_from_config(entry2).unwrap();

        match src1 {
            LogSource::File { id, file_pattern, line_pattern } => {
                assert_eq!(id, "test-log");
                assert_eq!(file_pattern.as_str(), r#"demo\.log(\.\d(\.gz)?)?"#);
                assert_eq!(line_pattern, "%{TIMESTAMP_ISO8601:timestamp} %{DATA:message}");
            }
            f => panic!("Conversion from file config failed: {:?}", f)
        }

        match src2 {
            LogSource::Journal { id, unit, line_pattern } => {
                assert_eq!(id, "test-journal");
                assert_eq!(unit.as_str(), "demo");
                assert_eq!(line_pattern, "%{TIMESTAMP_ISO8601:timestamp} %{DATA:message}");
            }
            f => panic!("Conversion from journal config failed: {:?}", f)
        }
    }
}