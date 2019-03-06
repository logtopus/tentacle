use std::fs;

use crate::data::ApplicationError;
use crate::data::LinePattern;
use crate::data::LogFilter;
use crate::data::StreamEntry;
use crate::log_streamer::StreamLogFile;
use crate::logsource::LogSource;
use crate::state;
use actix;
use futures::future::Future;
use regex::Regex;
use std::cmp::Ordering;
use std::fs::read_dir;
use std::fs::DirEntry;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Debug)]
pub enum LogSourceServiceMessage {
    StreamSourceContent(String, Arc<LogFilter>),
}

impl actix::Message for LogSourceServiceMessage {
    type Result = core::result::Result<(), ApplicationError>;
}

pub struct LogSourceService {
    state: state::ServerState,
    tx: futures::sync::mpsc::Sender<StreamEntry>,
}

impl LogSourceService {
    pub fn new(
        state: state::ServerState,
        tx: futures::sync::mpsc::Sender<StreamEntry>,
    ) -> LogSourceService {
        LogSourceService { state, tx }
    }

    fn stream_file(
        &mut self,
        path: &str,
        line_pattern: LinePattern,
        logfilter: Arc<LogFilter>,
    ) -> Result<(), ApplicationError> {
        let metadata = fs::metadata(&path).map_err(|_| ApplicationError::FailedToReadSource)?;

        match metadata.is_dir() {
            false => {
                let result = self
                    .state
                    .streamer()
                    .send(StreamLogFile {
                        path: path.to_string(),
                        line_pattern,
                        filter: logfilter,
                        tx: self.tx.clone(),
                    })
                    .wait();
                if let Err(t) = result {
                    error!("Failed to stream file: {}", t);
                };
                Ok(())
            }
            true => Err(ApplicationError::FailedToReadSource),
        }
    }

    fn resolve_files(file_pattern: &Regex) -> Result<Vec<String>, ApplicationError> {
        let folder = Path::new(file_pattern.as_str())
            .parent()
            .ok_or(ApplicationError::FailedToReadSource)?;
        debug!("Reading folder {:?}", folder);

        let files_iter = read_dir(folder)
            .map_err(|e| {
                error!("{}", e);
                ApplicationError::FailedToReadSource
            })?
            .filter_map(Result::ok)
            .flat_map(|entry: DirEntry| {
                trace!("Found entry {:?}", entry);
                let t = entry
                    .path()
                    .to_str()
                    .map(|path| {
                        let maybe_matches = file_pattern.captures(path);
                        if let Some(captures) = maybe_matches {
                            debug!("matching file: {}", path);
                            let rotation_idx = captures
                                .name("rotation")
                                .map(|e| e.as_str().parse::<i32>())
                                .and_then(|r| r.ok());
                            Some((path.to_string(), rotation_idx.unwrap_or(0)))
                        } else {
                            None
                        }
                    })
                    .and_then(|t| t);
                t
            });

        let mut vec: Vec<(String, i32)> = files_iter.collect();

        let now = SystemTime::now();

        vec.sort_by(|(path_a, idx_a), (path_b, idx_b)| match idx_b.cmp(idx_a) {
            Ordering::Equal => {
                let modtime_a = fs::metadata(&path_a)
                    .map(|meta| meta.modified())
                    .map(|maybe_time| maybe_time.unwrap_or_else(|_| now))
                    .unwrap_or_else(|_| now);
                let modtime_b = fs::metadata(&path_b)
                    .map(|meta| meta.modified())
                    .map(|maybe_time| maybe_time.unwrap_or_else(|_| now))
                    .unwrap_or_else(|_| now);
                modtime_a.cmp(&modtime_b)
            }
            ord => ord,
        });
        Ok(vec.into_iter().map(|(p, _)| p).collect())
    }
}

impl actix::Handler<LogSourceServiceMessage> for LogSourceService {
    type Result = core::result::Result<(), ApplicationError>;

    fn handle(&mut self, msg: LogSourceServiceMessage, _ctx: &mut Self::Context) -> Self::Result {
        match msg {
            LogSourceServiceMessage::StreamSourceContent(id, logfilter) => {
                let lookup = self.state.lookup_source(id.as_ref());
                lookup.and_then(|maybe_src| match maybe_src {
                    Some(LogSource::File {
                        id: _,
                        file_pattern,
                        line_pattern,
                    }) => {
                        let result = LogSourceService::resolve_files(&file_pattern);
                        match result {
                            Ok(files) => files
                                .iter()
                                .map(|file| {
                                    self.stream_file(file, line_pattern.clone(), logfilter.clone())
                                })
                                .collect(),
                            Err(_e) => Err(ApplicationError::FailedToReadSource),
                        }
                    }
                    Some(LogSource::Journal {
                        id: _,
                        unit: _,
                        line_pattern: _,
                    }) => unimplemented!(),
                    None => Err(ApplicationError::SourceNotFound),
                })
            }
        }
    }
}

impl actix::Actor for LogSourceService {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {}
}

#[cfg(test)]
mod tests {
    use crate::logsource_svc::LogSourceService;
    use regex::Regex;

    #[test]
    fn test_resolve_files() {
        let regex = Regex::new(r#"tests/demo\.log(\.(?P<rotation>\d)(\.gz)?)?"#).unwrap();
        let result = LogSourceService::resolve_files(&regex).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.get(0), Some(&"tests/demo.log.2.gz".to_string()));
        assert_eq!(result.get(1), Some(&"tests/demo.log.1".to_string()));
        assert_eq!(result.get(2), Some(&"tests/demo.log".to_string()));
    }
}
