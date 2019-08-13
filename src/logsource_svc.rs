use crate::data::ApplicationError;
use crate::data::LogFilter;
use crate::logsource::LogSource;
use crate::state;
use futures::Future;
use std::sync::Arc;

use crate::logsource_repo;

pub struct LogSourceService {
    state: Arc<state::ServerState>,
    repo: Arc<logsource_repo::LogSourceRepository>,
}

impl LogSourceService {
    pub fn new(
        state: state::ServerState,
        repo: logsource_repo::LogSourceRepository,
    ) -> LogSourceService {
        LogSourceService {
            state: Arc::new(state),
            repo: Arc::new(repo),
        }
    }

    pub fn stream_source_content(
        &self,
        id: String,
        logfilter: Arc<LogFilter>,
    ) -> impl Future<Item = (), Error = ApplicationError> {
        let state = self.state.clone();
        let repo = self.repo.clone();

        futures::lazy(move || {
            let lookup = state.lookup_source(id.as_ref());
            match lookup {
                None => Err(ApplicationError::SourceNotFound),

                Some(LogSource::File {
                    id: _,
                    file_pattern,
                    line_pattern,
                }) => repo.stream_files(&file_pattern, Arc::new(line_pattern), logfilter.clone()),

                Some(LogSource::Journal {
                    id: _,
                    unit: _,
                    line_pattern: _,
                }) => unimplemented!(),
            }
        })
    }
}
