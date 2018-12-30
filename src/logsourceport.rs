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
use crate::logsourcesvc::LogSourceService;
use crate::logsourcesvc::LogSourceServiceMessage;
use crate::state::ServerState;

#[derive(Serialize, Deserialize, Debug)]
pub struct LogSourceSpec {
    pub src_type: LogSourceType,
    pub path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LogSourceRepr {
    pub src_type: LogSourceType,
    pub path: Option<String>,
}

pub fn get_sources(state: actix_web::State<ServerState>) -> HttpResponse {
    let sources = state.get_sources();
    let lock = sources.read();
    match lock {
        Ok(locked_vec) => {
            let dto: Vec<LogSourceRepr> = locked_vec.iter().map(|src| LogSource::into_repr(src)).collect();
            HttpResponse::Ok().json(dto)
        }
        Err(_) => HttpResponse::InternalServerError().finish()
    }
}


pub fn get_source_content(id: actix_web::Path<String>, state: actix_web::State<ServerState>) -> actix_web::FutureResponse<HttpResponse>
{
    debug!("Content for source {} requested", id);

    let src_lookup = state.get_source(id.as_ref());
    let result = src_lookup
        .from_err()
        .and_then(|maybe_src| {
            match maybe_src {
                Some(LogSource::File { path }) => Box::new(stream_file(state, &path)),
                Some(LogSource::Journal) => { unimplemented!() }
                None => futures::future::ok(HttpResponse::NotFound().finish()).responder()
            }
        });

    result.responder()
}

fn stream_file(state: actix_web::State<ServerState>, path: &String) -> impl Future<Item=HttpResponse, Error=actix_web::Error> {
    let (tx, rx_body) = futures::sync::mpsc::channel(1024 * 1024);
    let logsourcesvc = LogSourceService::new(state.clone(), tx).start();
    let request = logsourcesvc.send(LogSourceServiceMessage::StreamFilePath(path.to_string()));

    request
        .map_err(|e| { error!("{}",e); e })
        .from_err()
        .and_then(|result| {
            result
                .map_err(|e| { error!("{}",e); e.into() })
                .map(|_| {
                    HttpResponse::Ok()
                        .content_encoding(actix_web::http::ContentEncoding::Identity)
                        .content_type("text/plain")
                        .streaming(rx_body
                            .map_err(|_| actix_web::error::PayloadError::Incomplete)
                            .map(|s| Bytes::from(s)))
                })
        })
}

