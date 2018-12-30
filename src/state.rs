use std::sync::Arc;
use std::sync::RwLock;

use failure::Fail;
use futures::Future;

use crate::blocking;
use crate::logsource::LogSource;

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

pub struct ServerState {
    sources: LogSources,
    addr_blocking: actix::Addr<blocking::BlockingSpawner>,
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        ServerState {
            sources: self.sources.clone(),
            addr_blocking: self.addr_blocking.clone(),
        }
    }
}

impl ServerState {
    pub fn new() -> ServerState {
        ServerState {
            sources: Arc::new(RwLock::new(vec![])),
            addr_blocking: actix::Arbiter::start(|_| blocking::BlockingSpawner::new()),
        }
    }

    pub fn run_blocking(&self, message: blocking::Message) {
        self.addr_blocking.do_send(message)
    }

    pub fn add_source(&mut self, source: LogSource) -> Result<(), ApplicationError> {
        let key = Self::extract_source_key(&source);
        let mut locked_vec = self.sources.write().map_err(|_| ApplicationError::FailedToWriteSource)?;

        if locked_vec.iter().find(|src| Self::extract_source_key(src) == key).is_some() {
            Err(ApplicationError::SourceAlreadyAdded)
        } else {
            locked_vec.push(source);
            Ok(())
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
