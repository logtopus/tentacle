use std::sync::Arc;

use crate::data::LogSource;
use grok::Grok;

pub struct ServerState {
    sources: Arc<Vec<LogSource>>,
    pub grok: Arc<Grok>,
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        ServerState {
            sources: self.sources.clone(),
            grok: self.grok.clone(),
        }
    }
}

impl ServerState {
    pub fn new(sources: Vec<LogSource>, grok: Grok) -> ServerState {
        ServerState {
            sources: Arc::new(sources),
            grok: Arc::new(grok),
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

    pub fn lookup_source(&self, key: &str) -> Option<LogSource> {
        let maybe_src = self
            .sources
            .iter()
            .find(|src| Self::extract_source_key(src) == key);
        maybe_src.map(|s| (*s).clone())
    }
}
