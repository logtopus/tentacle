use std::sync::Arc;

use failure::Fail;
use futures::Future;

use crate::logsource::LogSource;
use grok::Grok;
use std::sync::Mutex;
use tokio::runtime::Runtime;

#[derive(Fail, Debug)]
pub enum ApplicationError {
    // indicates that a requested log source is not configured
    #[fail(display = "Source not found")]
    SourceNotFound,
    // indicates that a requested log source is configured but cannot be read
    #[fail(display = "Failed to read source")]
    FailedToReadSource,
    //    #[fail(display = "Missing attribute: {}", attr)]
    //    MissingAttribute { attr: String },
}

pub struct ServerState {
    sources: Arc<Vec<LogSource>>,
    blk_rt: Arc<Mutex<Runtime>>,
    pub grok: Arc<Grok>,
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        ServerState {
            sources: self.sources.clone(),
            blk_rt: self.blk_rt.clone(),
            grok: self.grok.clone(),
        }
    }
}

impl ServerState {
    pub fn new(sources: Vec<LogSource>, grok: Grok) -> ServerState {
        ServerState {
            sources: Arc::new(sources),
            blk_rt: Arc::new(Mutex::new(Runtime::new().unwrap())),
            grok: Arc::new(grok),
        }
    }

    pub fn run_blocking(&self, future: impl Future<Item = (), Error = ()> + Send + 'static) {
        match self.blk_rt.lock().as_mut().map(|r| r.spawn(future)) {
            Ok(_) => (),
            Err(e) => error!("{}", e),
        }
    }

    fn extract_source_key(source: &LogSource) -> &String {
        match source {
            LogSource::File {
                id,
                file_pattern: _,
                line_pattern: _,
            } => id,
            LogSource::Journal {
                id,
                unit: _,
                line_pattern: _,
            } => id,
        }
    }

    pub fn get_sources(&self) -> Arc<Vec<LogSource>> {
        self.sources.clone()
    }

    pub fn lookup_source(&self, key: &str) -> Result<Option<LogSource>, ApplicationError> {
        let maybe_src = self
            .sources
            .iter()
            .find(|src| Self::extract_source_key(src) == key);
        Ok(maybe_src.map(|s| (*s).clone()))
    }
}
