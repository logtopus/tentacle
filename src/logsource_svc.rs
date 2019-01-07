use std::fs;
use std::io;

use actix;
use futures::Future;
use futures::Sink;
use futures::Stream;

use crate::logcodec::LogCodec;
use crate::state;
use crate::logsource::LogSource;
use std::fs::read_dir;
use regex::Regex;
use crate::state::ApplicationError;
use std::path::Path;
use std::fs::DirEntry;
use std::time::SystemTime;

#[derive(Debug)]
pub enum LogSourceServiceMessage {
    StreamSourceContent(String),
}

impl actix::Message for LogSourceServiceMessage {
    type Result = core::result::Result<(), ApplicationError>;
}

pub struct LogSourceService {
    state: state::ServerState,
    tx: futures::sync::mpsc::Sender<String>,
}

impl LogSourceService {
    pub fn new(state: state::ServerState, tx: futures::sync::mpsc::Sender<String>) -> LogSourceService {
        LogSourceService {
            state,
            tx,
        }
    }

    fn stream_file(&mut self, path: &str) -> Result<(), ApplicationError> {
        let metadata = fs::metadata(&path).map_err(|_| ApplicationError::FailedToReadSource)?;

        match metadata.is_dir() {
            false => {
                // see https://github.com/actix/actix/issues/181
                let open_result = fs::File::open(path);
                open_result
                    .map_err(|_| ApplicationError::FailedToReadSource)
                    .map(|f| {
                        let tokio_file = tokio::fs::File::from_std(f);

                        let linereader = tokio::codec::FramedRead::new(
                            tokio_file,
                            LogCodec::new(2048),
                        );
                        let tx = self.tx.clone();
                        let future =
                            linereader
                                .forward(
                                    tx.sink_map_err(|e| {
                                        io::Error::new(io::ErrorKind::Other, e.to_string())
                                    }),
                                )
                                .map(|_| ())
                                .map_err(|e| {
                                    error!("Stream error: {:?}", e);
                                });
                        self.state.run_blocking(future);
                    })
            }
            true => Err(ApplicationError::FailedToReadSource)
        }
    }

    fn resolve_files(file_pattern: &Regex) -> Result<Vec<String>, ApplicationError> {
        let folder = Path::new(file_pattern.as_str()).parent().ok_or(ApplicationError::FailedToReadSource)?;
        debug!("Reading folder {:?}", folder);

        let files_iter = read_dir(folder)
            .map_err(|e| {
                error!("{}", e);
                ApplicationError::FailedToReadSource
            })?
            .filter_map(Result::ok)
            .map(|entry: DirEntry| {
                trace!("Found entry {:?}", entry);
                entry.path().to_str()
                    .filter(|path| {
                        file_pattern.is_match(path)
                    })
                    .filter(|path| {
                        debug!("matching file: {}", path);
                        if path.ends_with(".gz") {
                            warn!("Currently skipping gz files");
                            false
                        } else {
                            true
                        }
                    })
                    .map(|p| p.to_string())
            })
            .filter_map(|maybe_path| maybe_path)
            .map(|path: String| Ok(path));


        let result: Result<Vec<String>, ApplicationError> = files_iter.collect();

        if let Ok(mut vec) = result {
            let now = SystemTime::now();

            vec.sort_by(|entry_a, entry_b| {
                let modtime_a = fs::metadata(&entry_a).map(|meta| meta.modified())
                    .map(|maybe_time| maybe_time.unwrap_or_else(|_| now)).unwrap_or_else(|_| now);
                let modtime_b = fs::metadata(&entry_b).map(|meta| meta.modified())
                    .map(|maybe_time| maybe_time.unwrap_or_else(|_| now)).unwrap_or_else(|_| now);
                modtime_a.cmp(&modtime_b)
            });
            Ok(vec)
        } else {
            result
        }
    }
}

impl actix::Handler<LogSourceServiceMessage> for LogSourceService {
    type Result = core::result::Result<(), ApplicationError>;

    fn handle(&mut self, msg: LogSourceServiceMessage, _ctx: &mut Self::Context) -> Self::Result {
        match msg {
            LogSourceServiceMessage::StreamSourceContent(id) => {
                self.state.lookup_source(id.as_ref())
                    .and_then(|maybe_src| {
                        match maybe_src {
                            Some(LogSource::File { id: _, file_pattern, line_pattern: _ }) => {
                                let result = LogSourceService::resolve_files(&file_pattern);
                                match result {
                                    Ok(files) =>
                                        files.iter().map(|file| self.stream_file(file)).collect(),
                                    Err(_e) => Err(ApplicationError::FailedToReadSource)
                                }
                            }
                            Some(LogSource::Journal { id: _, unit: _, line_pattern: _ }) => {
                                unimplemented!()
                            }
                            None => Err(ApplicationError::SourceNotFound)
                        }
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
        let regex = Regex::new(r#"tests/demo\.log(\.\d(\.gz)?)?"#).unwrap();
        let result = LogSourceService::resolve_files(&regex).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(0), Some(&"tests/demo.log.1".to_string()));
        assert_eq!(result.get(1), Some(&"tests/demo.log".to_string()));
    }
}
