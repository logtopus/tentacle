use actix_web::error::ResponseError;
use actix_web::http::header;
use actix_web::web;
use actix_web::HttpResponse;
use bytes::BufMut;
use bytes::Bytes;
use futures;
use futures::Future;
use futures::Stream;
use std::sync::Arc;

use crate::data::ApplicationError;
use crate::data::LogFilter;
use crate::logsource::LogSource;
use crate::logsource::LogSourceType;
use crate::logsource_repo::LogSourceRepository;
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

impl From<&LogSource> for LogSourceRepr {
    fn from(src: &LogSource) -> Self {
        match src {
            LogSource::File {
                id,
                file_pattern,
                line_pattern,
            } => LogSourceRepr {
                src_type: LogSourceType::File,
                id: id.clone(),
                line_pattern: Some(line_pattern.raw.clone()),
                file_pattern: Some(file_pattern.as_str().to_string()),
                unit: None,
            },
            LogSource::Journal {
                id,
                unit,
                line_pattern,
            } => LogSourceRepr {
                src_type: LogSourceType::Journal,
                id: id.clone(),
                line_pattern: Some(line_pattern.raw.clone()),
                file_pattern: None,
                unit: Some(unit.clone()),
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
) -> impl Future<Item = HttpResponse, Error = actix_web::Error> {
    debug!("Content for source {} requested", id);

    let (tx, rx_body) = futures::sync::mpsc::channel(1024 * 1024);
    let reposvc = LogSourceRepository::new(tx);
    let domainsvc = LogSourceService::new(state.get_ref().clone(), reposvc);

    domainsvc
        .stream_source_content(id.to_string(), Arc::new(logfilter_from_query(&filter)))
        .map_err(|e| e.into())
        .and_then(move |_| {
            let mut last_ts = 0;
            futures::future::ok(
                HttpResponse::Ok()
                    .content_type(if as_json {
                        "application/json"
                    } else {
                        "text/plain"
                    })
                    .streaming(rx_body.map(move |mut stream_entry| {
                        if stream_entry.parsed_line.timestamp == 0 {
                            stream_entry.parsed_line.timestamp = last_ts;
                        } else {
                            last_ts = stream_entry.parsed_line.timestamp;
                        }

                        if as_json {
                            match serde_json::to_vec(&stream_entry.parsed_line) {
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
                            let mut vec = stream_entry.line.into_bytes();
                            vec.put_u8('\n' as u8);
                            Bytes::from(vec)
                        }
                    })),
            )
        })
}

pub fn get_source_content_text(
    id: web::Path<String>,
    filter: web::Query<Filter>,
    state: web::Data<ServerState>,
) -> impl Future<Item = HttpResponse, Error = actix_web::Error> {
    get_source_content(id, filter, state, false)
}

pub fn get_source_content_json(
    id: web::Path<String>,
    filter: web::Query<Filter>,
    state: web::Data<ServerState>,
) -> impl Future<Item = HttpResponse, Error = actix_web::Error> {
    get_source_content(id, filter, state, true)
}

// #[cfg(test)]
// mod tests {
// }
