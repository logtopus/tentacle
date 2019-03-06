use std::sync::Arc;

use crate::data::ApplicationError;
use crate::log_streamer::LogFileStreamer;
use crate::logsource::LogSource;
use grok::Grok;

pub struct ServerState {
    sources: Arc<Vec<LogSource>>,
    streamer: Arc<actix::Addr<LogFileStreamer>>,
    pub grok: Arc<Grok>,
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        ServerState {
            sources: self.sources.clone(),
            streamer: self.streamer.clone(),
            grok: self.grok.clone(),
        }
    }
}

impl ServerState {
    pub fn new(sources: Vec<LogSource>, grok: Grok) -> ServerState {
        ServerState {
            sources: Arc::new(sources),
            streamer: Arc::new(actix::sync::SyncArbiter::start(16, || LogFileStreamer {})),
            grok: Arc::new(grok),
        }
    }

    pub fn streamer(&self) -> Arc<actix::Addr<LogFileStreamer>> {
        self.streamer.clone()
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
