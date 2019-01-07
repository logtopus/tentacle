use actix;
use actix::Actor;
use actix_web;
use actix_web::AsyncResponder;
use actix_web::HttpResponse;
use bytes::Bytes;
use config;
use futures;
use futures::Future;
use futures::Stream;

use crate::logsource::LogSource;
use crate::logsource::LogSourceType;
use crate::logsource_svc::LogSourceService;
use crate::logsource_svc::LogSourceServiceMessage;
use crate::state::ServerState;

//#[derive(Serialize, Deserialize, Debug)]
//pub struct LogSourceSpec {
//    pub src_type: LogSourceType,
//    pub id: Option<String>,
//}

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
            LogSource::File { id, file_pattern, line_pattern } =>
                LogSourceRepr {
                    src_type: LogSourceType::File,
                    id: id.clone(),
                    line_pattern: Some(line_pattern.clone()),
                    file_pattern: Some(file_pattern.as_str().to_string()),
                    unit: None,
                },
            LogSource::Journal { id, unit, line_pattern } =>
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


pub fn get_source_content(id: actix_web::Path<String>, state: actix_web::State<ServerState>) -> actix_web::FutureResponse<HttpResponse> {
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
        .and_then(|result| {
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
                            .map(|s| Bytes::from(s)))
                })
        })
        .responder()
}

//#[cfg(test)]
//mod tests {
//}
