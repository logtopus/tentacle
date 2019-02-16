use actix;
use actix::Actor;
use actix_web;
use actix_web::AsyncResponder;
use actix_web::HttpResponse;
use bytes::Bytes;
use futures;
use futures::Future;
use futures::Stream;

use crate::logsource::LogSource;
use crate::logsource::LogSourceType;
use crate::logsource_svc::LogSourceService;
use crate::logsource_svc::LogSourceServiceMessage;
use crate::logsource_svc::StreamEntry;
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

impl From<&LogSource> for LogSourceRepr {
    fn from(src: &LogSource) -> Self {
        match src {
            LogSource::File { id, file_pattern, line_pattern, grok_pattern: _ } =>
                LogSourceRepr {
                    src_type: LogSourceType::File,
                    id: id.clone(),
                    line_pattern: Some(line_pattern.clone()),
                    file_pattern: Some(file_pattern.as_str().to_string()),
                    unit: None,
                },
            LogSource::Journal { id, unit, line_pattern, grok_pattern: _ } =>
                LogSourceRepr {
                    src_type: LogSourceType::Journal,
                    id: id.clone(),
                    line_pattern: Some(line_pattern.clone()),
                    file_pattern: None,
                    unit: Some(unit.clone()),
                },
        }
    }
}

pub fn get_sources(state: actix_web::State<ServerState>) -> HttpResponse {
    let sources = state.get_sources();
    let lock = sources.read();
    match lock {
        Ok(locked_vec) => {
            let dto: Vec<LogSourceRepr> = locked_vec.iter().map(|src| LogSourceRepr::from(src)).collect();
            HttpResponse::Ok().json(dto)
        }
        Err(_) => HttpResponse::InternalServerError().finish()
    }
}

fn get_source_content(id: actix_web::Path<String>, state: actix_web::State<ServerState>, as_json: bool) -> actix_web::FutureResponse<HttpResponse> {
    debug!("Content for source {} requested", id);

    let (tx, rx_body) = futures::sync::mpsc::channel(1024 * 1024);
    let logsourcesvc = LogSourceService::new(state.clone(), tx).start();
    let request = logsourcesvc.send(LogSourceServiceMessage::StreamSourceContent(id.to_string()));

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
                    HttpResponse::Ok()
                        .content_encoding(actix_web::http::ContentEncoding::Identity)
                        .content_type("text/plain")
                        .streaming(rx_body
                            .map_err(|_| actix_web::error::PayloadError::Incomplete)
                            .map(move |stream_entry| {
                                if as_json {
                                    match serde_json::to_vec(&parse_line(&stream_entry)) {
                                        Ok(vec) => Bytes::from(vec),
                                        Err(e) => {
                                            error!("Failed to convert stream entry to json: {}", e);
                                            Bytes::new()
                                        }
                                    }
                                } else {
                                    Bytes::from(stream_entry.line)
                                }
                            }))
                })
        })
        .responder()
}

#[derive(Serialize, Deserialize, Debug)]
struct ParsedLine {
    timestamp: String,
    message: String
}

fn parse_line(stream_entry: &StreamEntry) -> ParsedLine {
    let maybe_matches = stream_entry.pattern.match_against(&stream_entry.line);

    if let Some(matches) = maybe_matches {
        ParsedLine {
            timestamp: matches.get("timestamp").unwrap_or("1970-01-01T00:00+00:00").to_string(),
            message: matches.get("message").unwrap_or("").to_string(),
        }
    } else {
        ParsedLine {
            timestamp: "1970-01-01T00:00+00:00".to_string(),
            message: "<failed to parse entry>".to_string(),
        }
    }
}

pub fn get_source_content_text(id: actix_web::Path<String>, state: actix_web::State<ServerState>) -> actix_web::FutureResponse<HttpResponse> {
    get_source_content(id, state, false)
}


pub fn get_source_content_json(id: actix_web::Path<String>, state: actix_web::State<ServerState>) -> actix_web::FutureResponse<HttpResponse> {
    get_source_content(id, state, true)
}

#[cfg(test)]
mod tests {
    use crate::logsource_svc::StreamEntry;
    use grok;
    use std::sync::Arc;

    #[test]
    fn test_parse_line_with_timestamp() {
        let mut grok = grok::Grok::default();

        let pattern = Arc::new(grok.compile("%{TIMESTAMP_ISO8601:timestamp} %{GREEDYDATA:message}", true).unwrap());
        let line = "2018-01-01 12:39:01 first message".to_string();
        let parsed_line = super::parse_line(&StreamEntry { line, pattern });
        assert_eq!("2018-01-01 12:39:01", parsed_line.timestamp);
        assert_eq!("first message", parsed_line.message);

        let pattern = Arc::new(grok.compile("%{SYSLOGTIMESTAMP:timestamp} %{GREEDYDATA:message}", true).unwrap());
        let line = "Feb 10 13:17:01 second message".to_string();
        let parsed_line = super::parse_line(&StreamEntry { line, pattern });
        assert_eq!("Feb 10 13:17:01", parsed_line.timestamp);
        assert_eq!("second message", parsed_line.message);
    }
}
