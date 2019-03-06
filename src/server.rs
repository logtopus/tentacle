use std;

use actix_web;
use actix_web::error::ResponseError;
use actix_web::http::header;
use actix_web::Binary;
use actix_web::Body;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use config;

use crate::constants::*;
use crate::data::ApplicationError;
use crate::logsource::LogSource;
use crate::logsource_port;
use crate::state::ServerState;

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
struct ErrorResponse {
    message: String,
}

fn parse_source_config(settings: &config::Config, grok: &mut grok::Grok) -> Vec<LogSource> {
    if let Ok(array) = settings.get_array("sources") {
        array
            .iter()
            .map(|v| {
                // let grok = Arc::get_mut(&mut grok).unwrap();
                LogSource::try_from_config(v, grok).unwrap()
            })
            .collect()
    } else {
        warn!("No log sources configured");
        vec![]
    }
}

pub fn start_server(settings: &config::Config) {
    let port = settings.get_int("http.bind.port").unwrap();
    let ip = settings.get_str("http.bind.ip").unwrap();
    let addr: std::net::SocketAddr = format!("{}:{}", ip, port).parse().unwrap();

    let mut grok = grok::Grok::default();

    let server_state = ServerState::new(parse_source_config(&settings, &mut grok), grok);

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
                    r.get().with(logsource_port::get_sources);
                    r.head().f(|_| HttpResponse::MethodNotAllowed());
                })
                .resource("/sources/{id}/content", |r| {
                    r.get()
                        .filter(actix_web::pred::Header("Accept", "*/*"))
                        .with(logsource_port::get_source_content_text);
                    r.get()
                        .filter(actix_web::pred::Header("Accept", "text/plain"))
                        .with(logsource_port::get_source_content_text);
                    r.get()
                        .filter(actix_web::pred::Header("Accept", "application/json"))
                        .with(logsource_port::get_source_content_json);
                    r.get().f(|_| HttpResponse::NotAcceptable());
                    r.head().f(|_| HttpResponse::MethodNotAllowed());
                })
                .boxed(),
            actix_web::App::new()
                .middleware(actix_web::middleware::Logger::default())
                .resource("/index.html", |r| r.f(index))
                .boxed(),
        ]
    })
    .bind(addr)
    .expect(&format!("Failed to bind to {}:{}", ip, port))
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

    use actix_web::http;
    use actix_web::test::TestRequest;
    use actix_web::Body;
    use actix_web::FromRequest;
    use actix_web::State;

    #[test]
    fn test_index_handler() {
        let resp = TestRequest::with_header("content-type", "text/plain")
            .execute(&super::index)
            .unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);

        match resp.body() {
            Body::Binary(binary) => assert_eq!(
                std::str::from_utf8(binary.as_ref()).unwrap(),
                super::WELCOME_MSG
            ),
            t => assert!(false, format!("Wrong body type {:?}", t)),
        }
    }

    #[test]
    fn test_health_handler() {
        actix::System::run(|| {
            let state = super::ServerState::new(vec![], grok::Grok::default());
            let req = TestRequest::with_state(state).finish();
            let resp = super::health(State::extract(&req));
            assert_eq!(resp.status(), http::StatusCode::OK);

            actix::System::current().stop();
        });
    }
}
