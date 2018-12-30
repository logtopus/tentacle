use std;

use actix;
use actix_web;
use actix_web::Binary;
use actix_web::Body;
use actix_web::error::ResponseError;
use actix_web::http;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use config;
use futures;

use crate::constants::*;
use crate::logsource::LogSource;
use crate::logsourceport;
use crate::state;
use crate::state::ServerState;

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
struct ErrorResponse {
    message: String
}

pub fn start_server(settings: &config::Config) {
    let port = settings.get_int("http.bind.port").unwrap();
    let ip = settings.get_str("http.bind.ip").unwrap();
    let addr: std::net::SocketAddr = format!("{}:{}", ip, port).parse().unwrap();
    let mut server_state = ServerState::new();

    if let Ok(array) = settings.get_array("files") {
        array.iter()
            .for_each(|v| {
                let src = LogSource::try_from_fileconfig(v.clone()).unwrap();
                server_state.add_source(src).unwrap();
            });
    };

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
                    r.get().with(logsourceport::get_sources);
                    r.head().f(|_| HttpResponse::MethodNotAllowed());
                })
                .resource("/sources/{id}/content", |r| {
                    r.get().with(logsourceport::get_source_content);
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

#[cfg(test)]
mod tests {
    use std;

    use actix_web::{http, test};
    use actix_web::Body;
    use actix_web::FromRequest;
    use actix_web::State;

//    use crate::state;

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
