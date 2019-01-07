use std::sync::Arc;
use std::sync::RwLock;

use failure::Fail;
use futures::Future;

use crate::logsource::LogSource;
use std::sync::Mutex;
use tokio::runtime::Runtime;

type LogSources = Arc<RwLock<Vec<LogSource>>>;

#[derive(Fail, Debug)]
pub enum ApplicationError {
    #[fail(display = "Source not found")]
    SourceNotFound,
    #[fail(display = "Source already exists")]
    SourceAlreadyAdded,
    #[fail(display = "Failed to store source")]
    FailedToWriteSource,
    #[fail(display = "Failed to read source")]
    FailedToReadSource,
//    #[fail(display = "Missing attribute: {}", attr)]
//    MissingAttribute { attr: String },
}

pub struct ServerState {
    sources: LogSources,
    blk_rt: Arc<Mutex<Runtime>>,
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        ServerState {
            sources: self.sources.clone(),
            blk_rt: self.blk_rt.clone(),
        }
    }
}

impl ServerState {
    pub fn new() -> ServerState {
        ServerState {
            sources: Arc::new(RwLock::new(vec![])),
            blk_rt: Arc::new(Mutex::new(Runtime::new().unwrap())),
        }
    }

    pub fn run_blocking(&self, future: impl Future<Item=(), Error=()> + Send + 'static) {
        match self.blk_rt.lock().as_mut().map(|r| r.spawn(future)) {
            Ok(_) => (),
            Err(e) => error!("{}", e)
        }
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
            LogSource::File { id, file_pattern: _, line_pattern: _ } => id,
            LogSource::Journal { id, unit: _, line_pattern: _ } => id
        }
    }

    pub fn get_sources(&self) -> LogSources {
        self.sources.clone()
    }

    pub fn lookup_source(&self, key: &str) -> Result<Option<LogSource>, ApplicationError> {
        let locked_vec = self.sources.read().unwrap();
        let maybe_src = locked_vec.iter().find(|src| Self::extract_source_key(src) == key);
        Ok(maybe_src.map(|s| (*s).clone()))
    }
}
