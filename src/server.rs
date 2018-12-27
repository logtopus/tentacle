use std;

use actix;
use actix::Actor;
use actix_web;
use actix_web::AsyncResponder;
use actix_web::Binary;
use actix_web::Body;
use actix_web::error::ResponseError;
use actix_web::http;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use bytes::Bytes;
use config;
use futures;
use futures::Future;
use futures::Stream;

use crate::constants::*;
use crate::state;
use crate::state::ServerState;
use crate::streamer;

impl ResponseError for state::ApplicationError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            state::ApplicationError::SourceAlreadyAdded => {
                HttpResponse::BadRequest()
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .json(ErrorResponse { message: self.to_string() })
            }
            state::ApplicationError::FailedToWriteSource => {
                HttpResponse::InternalServerError()
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .json(ErrorResponse { message: self.to_string() })
            }
            state::ApplicationError::FailedToReadSource => {
                HttpResponse::InternalServerError()
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .json(ErrorResponse { message: self.to_string() })
            }
            state::ApplicationError::MissingAttribute { attr: _ } => {
                HttpResponse::BadRequest()
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .json(ErrorResponse { message: self.to_string() })
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SourceSpec {
    pub key: Option<String>,
    pub src_type: state::SourceType,
    pub path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SourceRepr {
    pub key: String,
    pub src_type: state::SourceType,
    pub path: Option<String>,
}


#[derive(Serialize, Deserialize, Debug)]
struct ErrorResponse {
    message: String
}

pub fn start_server(settings: &config::Config) {
    let port = settings.get_int("http.bind.port").unwrap();
    let ip = settings.get_str("http.bind.ip").unwrap();
    let addr: std::net::SocketAddr = format!("{}:{}", ip, port).parse().unwrap();
    let server_state = ServerState::new();

    actix_web::server::new(move || {
        vec![
            actix_web::App::with_state(server_state.clone())
                .middleware(actix_web::middleware::Logger::default())
                .prefix("/api/v1")
                .resource("/health", |r| {
                    r.get().with(health);
                    r.head().f(|_| HttpResponse::MethodNotAllowed());
                })
                .resource("/sources", |r| {
                    r.post().with(add_source);
                    r.get().with(get_sources);
                    r.head().f(|_| HttpResponse::MethodNotAllowed());
                })
                .resource("/sources/{id}/content", |r| {
                    r.get().with(get_source_content);
                    r.head().f(|_| HttpResponse::MethodNotAllowed());
                })
                .boxed(),
            actix_web::App::new()
                .middleware(actix_web::middleware::Logger::default())
                .resource("/index.html", |r| r.f(index))
                .boxed()
        ]
    })
        .bind(addr).expect(&format!("Failed to bind to {}:{}", ip, port))
        .start();

    println!("Started http server: {:?}", addr);
}

fn index(_req: &HttpRequest) -> HttpResponse {
    HttpResponse::Ok().body(Body::Binary(Binary::Slice(WELCOME_MSG.as_ref())))
}

fn health(_state: actix_web::State<ServerState>) -> HttpResponse {
    HttpResponse::Ok().finish()
}

fn add_source((json, state): (actix_web::Json<SourceSpec>, actix_web::State<ServerState>)) -> HttpResponse {
    debug!("add_source");
    let response = match state::LogSource::try_from_spec(json.into_inner()) {
        Ok(src) => {
            match state.clone().add_source(src) {
                Ok(key) =>
                    HttpResponse::Created()
                        .header(http::header::CONTENT_LOCATION, format!("/api/v1/sources/{}", key))
                        .finish(),
                Err(e) => e.error_response()
            }
        }
        Err(e) => e.error_response()
    };

    response
}


fn get_sources(state: actix_web::State<ServerState>) -> HttpResponse {
    let sources = state.get_sources();
    let lock = sources.read();
    match lock {
        Ok(locked_vec) => {
            let dto: Vec<SourceRepr> = locked_vec.iter().map(|src| state::LogSource::into_repr(src)).collect();
            HttpResponse::Ok().json(dto)
        }
        Err(_) => HttpResponse::InternalServerError().finish()
    }
}


fn get_source_content(id: actix_web::Path<String>, state: actix_web::State<ServerState>) -> actix_web::FutureResponse<HttpResponse>
{
    debug!("Content for source {} requested", id);

    let src_lookup = state.get_source(id.as_ref());
    let result = src_lookup
        .from_err()
        .and_then(|maybe_src| {
            match maybe_src {
                Some(state::LogSource::File { key: _, path }) => Box::new(stream_file(state, &path)),
                Some(state::LogSource::Url) => { unimplemented!() }
                Some(state::LogSource::Journal) => { unimplemented!() }
                None => futures::future::ok(HttpResponse::NotFound().finish()).responder()
            }
        });

    result.responder()
}

fn stream_file(state: actix_web::State<ServerState>, path: &String) -> impl Future<Item=HttpResponse, Error=actix_web::Error> {
    let (tx, rx_body) = futures::sync::mpsc::channel(1024 * 1024);
    let streamer = streamer::Streamer::new(state.clone(), tx).start();
    let request = streamer.send(streamer::Message::StreamFilePath(path.to_string()));

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

#[cfg(test)]
mod tests {
    use std;

    use actix_web::{http, test};
    use actix_web::Body;
    use actix_web::FromRequest;
    use actix_web::State;
    use crate::state;

    #[test]
    fn test_index_handler() {
        let resp = test::TestRequest::with_header("content-type", "text/plain")
            .run(&super::index)
            .unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);

        match resp.body() {
            Body::Binary(binary) => {
                assert_eq!(std::str::from_utf8(binary.as_ref()).unwrap(), super::WELCOME_MSG)
            }
            t => assert!(false, format!("Wrong body type {:?}", t))
        }
    }

    #[test]
    fn test_health_handler() {
        let state = super::ServerState::new();
        let req = test::TestRequest::with_state(state).finish();
        let resp = super::health(State::extract(&req));
        assert_eq!(resp.status(), http::StatusCode::OK);
    }

//    #[test]
//    fn test_get_source_content_handler() {
//        let mut state = super::ServerState::new();
//        state.add_source(state::LogSource::File { key: "id".to_string(), path: "dummypath".to_string() }).unwrap();
//
//        let req = test::TestRequest::with_state(state).header("content-type", "text/plain").finish();
//        let resp = super::get_source_content(actix_web::Path::from("id".to_string()), State::extract(&req));
//
//        println!("before spawn");
//        let result = futures::executor::spawn(resp).wait_future().unwrap();
//        println!("after spawn");
//        assert_eq!(result.status(), http::StatusCode::OK, "Response was {:?}", result.body());
//
//        match result.body() {
//            Body::Streaming(_) => (),
//            t => assert!(false, format!("Wrong body type {:?}", t))
//        }
//    }
}
