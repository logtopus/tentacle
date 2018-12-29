use crate::blocking;
use crate::server;
use failure::Fail;
use futures::Future;
use std::sync::Arc;
use std::sync::RwLock;

type LogSources = Arc<RwLock<Vec<LogSource>>>;

#[derive(Fail, Debug)]
pub enum ApplicationError {
    #[fail(display = "LogSource already exists")]
    SourceAlreadyAdded,
    #[fail(display = "Failed to store source")]
    FailedToWriteSource,
    #[fail(display = "Failed to read source")]
    FailedToReadSource,
    #[fail(display = "Missing attribute: {}", attr)]
    MissingAttribute { attr: String },
}

#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
pub enum LogSourceType {
    File,
    Journal, // see https://docs.rs/systemd/0.0.8/systemd/journal/index.html
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
pub enum LogSource {
    File { path: String },
    Journal
}

impl LogSource {
    pub fn try_from_spec(dto: server::LogSourceSpec) -> Result<LogSource, ApplicationError> {
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

    pub fn into_repr(src: &LogSource) -> server::LogSourceRepr {
        match src {
            LogSource::File { path } =>
                server::LogSourceRepr {
                    src_type: LogSourceType::File,
                    path: Some(path.clone()),
                },
            LogSource::Journal =>
                unimplemented!()
        }
    }
}

pub struct ServerState {
    sources: LogSources,
    pub addr_blocking: actix::Addr<blocking::BlockingSpawner>
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        ServerState {
            sources: self.sources.clone(),
            addr_blocking: self.addr_blocking.clone()
        }
    }
}

impl ServerState {
    pub fn new() -> ServerState {
        ServerState {
            sources: Arc::new(RwLock::new(vec![])),
            addr_blocking: actix::Arbiter::start(|_| blocking::BlockingSpawner::new())
        }
    }

    pub fn add_source(&mut self, source: LogSource) -> Result<String, ApplicationError> {
        let key = Self::extract_source_key(&source);
        let mut locked_vec = self.sources.write().map_err(|_| ApplicationError::FailedToWriteSource)?;

        if locked_vec.iter().find(|src| Self::extract_source_key(src) == key).is_some() {
            Err(ApplicationError::SourceAlreadyAdded)
        } else {
            let key = ServerState::extract_source_key(&source).to_string();
            locked_vec.push(source);
            Ok(key)
        }
    }

    fn extract_source_key(source: &LogSource) -> &String {
        match source {
            LogSource::File { path } => path,
            LogSource::Journal =>
                unimplemented!(),
        }
    }

    pub fn get_sources(&self) -> LogSources {
        self.sources.clone()
    }

    pub fn get_source<'a>(&'a self, key: &'a str) -> impl Future<Item=Option<LogSource>, Error=ApplicationError> {
        futures::future::result(self.lookup_source(key))
    }

    fn lookup_source(&self, key: &str) -> Result<Option<LogSource>, ApplicationError> {
        let locked_vec = self.sources.read().map_err(|_| ApplicationError::FailedToReadSource)?;
        let maybe_src = locked_vec.iter().find(|src| Self::extract_source_key(src) == key);
        Ok(maybe_src.map(|s| (*s).clone()))
    }
}
