use config::ConfigError;
use config::Value;

use crate::logsourceport::LogSourceSpec;
use crate::state::ApplicationError;

#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
pub enum LogSourceType {
    File,
    Journal, // see https://docs.rs/systemd/0.0.8/systemd/journal/index.html
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
pub enum LogSource {
    File { path: String },
    Journal,
}

impl LogSource {
    pub fn try_from_fileconfig(value: Value) -> Result<LogSource, ConfigError> {
        let file_map = value.into_table()?;
        let path = file_map.get("path").ok_or(ConfigError::NotFound("path".to_string()))?;
        let path = path.clone().into_str()?;
        Ok(LogSource::File { path })
    }

    pub fn try_from_spec(dto: LogSourceSpec) -> Result<LogSource, ApplicationError> {
        match dto.src_type {
            LogSourceType::File => {
                match dto.path {
                    Some(p) => Ok(LogSource::File {
                        path: p
                    }),
                    None => Err(ApplicationError::MissingAttribute { attr: "path".to_string() })
                }
            }
            LogSourceType::Journal =>
                unimplemented!()
        }
    }
}
