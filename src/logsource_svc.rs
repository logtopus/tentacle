use crate::data::ApplicationError;
use crate::data::LogFilter;
use crate::data::LogSource;
use crate::data::StreamSink;
use crate::repository::FileRepository;
use crate::state;
use futures::Future;
use std::sync::Arc;

pub struct LogSourceService;

impl LogSourceService {
    pub fn stream_source_content(
        tx: StreamSink,
        id: String,
        state: state::ServerState,
        logfilter: Arc<LogFilter>,
    ) -> impl Future<Item = (), Error = ApplicationError> {
        let state = state.clone();

        futures::lazy(move || {
            let lookup = state.lookup_source(id.as_ref());
            match lookup {
                Some(logsource) => match logsource {
                    LogSource::File {
                        id: _,
                        file_pattern,
                        line_pattern,
                    } => {
                        FileRepository::stream(&file_pattern, Arc::new(line_pattern), tx, logfilter)
                    }
                    LogSource::Journal { .. } => unimplemented!(),
                },
                None => Err(ApplicationError::SourceNotFound),
            }
        })
    }
}
