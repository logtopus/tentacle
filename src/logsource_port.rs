use crate::data;
use crate::data::LogSource;
use crate::data::StreamEntry;
use actix_web::error::ResponseError;
use actix_web::http::header;
use actix_web::web;
use actix_web::HttpResponse;
use bytes::BufMut;
use bytes::Bytes;
use futures_01::stream::Stream;
use std::sync::Arc;

use crate::data::ApplicationError;
use crate::data::LogFilter;
use crate::logsource_svc::LogSourceService;
use crate::state::ServerState;

#[derive(Serialize, Deserialize, Debug)]
struct ErrorResponse {
    message: String,
}

impl ResponseError for ApplicationError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            ApplicationError::SourceNotFound => HttpResponse::NotFound()
                .header(header::CONTENT_TYPE, "application/json")
                .json(ErrorResponse {
                    message: self.to_string(),
                }),
            ApplicationError::FailedToReadSource => HttpResponse::InternalServerError()
                .header(header::CONTENT_TYPE, "application/json")
                .json(ErrorResponse {
                    message: self.to_string(),
                }),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
pub enum LogSourceType {
    File,
    Journal, // see https://docs.rs/systemd/0.0.8/systemd/journal/index.html
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LogSourceRepr {
    pub src_type: LogSourceType,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Deserialize)]
pub struct Filter {
    from_ms: Option<i64>,
    loglevels: Option<String>,
}

impl From<&data::LogSource> for LogSourceRepr {
    fn from(src: &data::LogSource) -> Self {
        match src {
            LogSource::File {
                id,
                file_pattern,
                line_pattern,
            } => LogSourceRepr {
                src_type: LogSourceType::File,
                id: id.to_string(),
                line_pattern: Some(line_pattern.raw.clone()),
                file_pattern: Some(file_pattern.to_string()),
                unit: None,
            },
            LogSource::Journal {
                id,
                unit,
                line_pattern,
            } => LogSourceRepr {
                src_type: LogSourceType::Journal,
                id: id.to_string(),
                line_pattern: Some(line_pattern.raw.clone()),
                file_pattern: None,
                unit: Some(unit.to_string()),
            },
        }
    }
}

pub fn get_sources(state: web::Data<ServerState>) -> HttpResponse {
    let dto: Vec<LogSourceRepr> = state
        .get_sources()
        .iter()
        .map(|src| LogSourceRepr::from(src))
        .collect();
    HttpResponse::Ok().json(dto)
}

fn logfilter_from_query(filter: &Filter) -> LogFilter {
    LogFilter {
        from_ms: filter.from_ms.map(|dt| dt as u128).unwrap_or(0),
        loglevels: filter
            .loglevels
            .clone()
            .map(|s| s.split(",").map(|s| s.to_string()).collect()),
    }
}

fn get_source_content(
    id: web::Path<String>,
    filter: web::Query<Filter>,
    state: web::Data<ServerState>,
    as_json: bool,
) -> HttpResponse {
    debug!("Content for source {} requested", id);

    let stream_result = LogSourceService::create_content_stream(
        id.to_string(),
        state.get_ref().clone(),
        &Arc::new(logfilter_from_query(&filter)),
    );

    match stream_result {
        Ok(stream) => {
            let mut last_ts = 0;

            let compat = futures::compat::Compat::new(stream);
            let mapped_stream = compat.map(move |stream_entry| match stream_entry {
                StreamEntry::LogLine {
                    line,
                    mut parsed_line,
                } => {
                    if as_json {
                        if parsed_line.timestamp == 0 {
                            parsed_line.timestamp = last_ts;
                        } else {
                            last_ts = parsed_line.timestamp;
                        }

                        match serde_json::to_vec(&parsed_line) {
                            Ok(mut vec) => {
                                vec.put_u8('\n' as u8);
                                Bytes::from(vec)
                            }
                            Err(e) => {
                                error!("Failed to convert stream entry to json: {}", e);
                                Bytes::new()
                            }
                        }
                    } else {
                        let mut vec = line.into_bytes();
                        vec.put_u8('\n' as u8);
                        Bytes::from(vec)
                    }
                }
            });

            HttpResponse::Ok()
                .content_type(if as_json {
                    "application/json"
                } else {
                    "text/plain"
                })
                .streaming(mapped_stream)
        }
        Err(e) => e.error_response(),
    }
}

pub fn get_source_content_text(
    id: web::Path<String>,
    filter: web::Query<Filter>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    get_source_content(id, filter, state, false)
}

pub fn get_source_content_json(
    id: web::Path<String>,
    filter: web::Query<Filter>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    get_source_content(id, filter, state, true)
}

// #[cfg(test)]
// mod tests {
// }
