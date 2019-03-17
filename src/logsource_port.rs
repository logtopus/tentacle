use actix;
use actix::Actor;
use actix_web;
use actix_web::AsyncResponder;
use actix_web::HttpResponse;
use bytes::BufMut;
use bytes::Bytes;
use futures;
use futures::Future;
use futures::Stream;
use std::sync::Arc;

use crate::data::LogFilter;
use crate::logsource::LogSource;
use crate::logsource::LogSourceType;
use crate::logsource_svc::LogSourceService;
use crate::logsource_svc::LogSourceServiceMessage;
use crate::state::ServerState;

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

pub fn get_sources(state: actix_web::State<ServerState>) -> HttpResponse {
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
    id: actix_web::Path<String>,
    filter: actix_web::Query<Filter>,
    state: actix_web::State<ServerState>,
    as_json: bool,
) -> actix_web::FutureResponse<HttpResponse> {
    debug!("Content for source {} requested", id);

    let (tx, rx_body) = futures::sync::mpsc::channel(1024 * 1024);
    let logsourcesvc = LogSourceService::new(state.clone(), tx).start();

    let request = logsourcesvc.send(LogSourceServiceMessage::StreamSourceContent(
        id.to_string(),
        Arc::new(logfilter_from_query(&filter)),
    ));

    request
        .map_err(|e| {
            error!("{}", e);
            e
        })
        .from_err()
        .and_then(move |result| {
            result
                .map_err(|e| {
                    error!("{}", e);
                    e.into()
                })
                .map(|_| {
                    let mut last_ts = 0;
                    HttpResponse::Ok()
                        .content_encoding(actix_web::http::ContentEncoding::Identity)
                        .content_type(if as_json {
                            "application/json"
                        } else {
                            "text/plain"
                        })
                        .streaming(
                            rx_body
                                .map_err(|_| actix_web::error::PayloadError::Incomplete)
                                .map(move |mut stream_entry| {
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
                                                error!(
                                                    "Failed to convert stream entry to json: {}",
                                                    e
                                                );
                                                Bytes::new()
                                            }
                                        }
                                    } else {
                                        let mut vec = stream_entry.line.into_bytes();
                                        vec.put_u8('\n' as u8);
                                        Bytes::from(vec)
                                    }
                                }),
                        )
                })
        })
        .responder()
}

pub fn get_source_content_text(
    id: actix_web::Path<String>,
    filter: actix_web::Query<Filter>,
    state: actix_web::State<ServerState>,
) -> actix_web::FutureResponse<HttpResponse> {
    get_source_content(id, filter, state, false)
}

pub fn get_source_content_json(
    id: actix_web::Path<String>,
    filter: actix_web::Query<Filter>,
    state: actix_web::State<ServerState>,
) -> actix_web::FutureResponse<HttpResponse> {
    get_source_content(id, filter, state, true)
}

// #[cfg(test)]
// mod tests {
// }
