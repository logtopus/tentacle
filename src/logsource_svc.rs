use crate::data::ApplicationError;
use crate::data::LogQueryContext;
use crate::data::LogSource;
use crate::data::LogStream;
use crate::repository::FileRepository;
use crate::state;
use std::sync::Arc;

pub struct LogSourceService;

impl LogSourceService {
    pub fn create_content_stream(
        id: String,
        state: state::ServerState,
        logfilter: &Arc<LogQueryContext>,
    ) -> Result<LogStream, ApplicationError> {
        let state = state.clone();

        let lookup = state.lookup_source(id.as_ref());
        match lookup {
            Some(logsource) => match logsource {
                LogSource::File {
                    id: _,
                    file_pattern,
                    line_pattern,
                } => FileRepository::create_stream(
                    &file_pattern,
                    &Arc::new(line_pattern),
                    &logfilter,
                ),
                LogSource::Journal { .. } => unimplemented!(),
            },
            None => Err(ApplicationError::SourceNotFound),
        }
    }
}
