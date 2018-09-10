use actix_web;
use actix_web::AsyncResponder;
use actix_web::Binary;
use actix_web::Body;
use actix_web::http;
use actix_web::HttpMessage;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use bytes::Bytes;
use config;
use futures;
use futures::Future;
use std;
use std::cell::RefCell;
use std::sync::Arc;

const WELCOME_MSG: &'static str = "This is a logtopus tentacle";

enum ApiError {
    LogfileAlreadyAdded
}

#[derive(Serialize, Deserialize, Debug)]
struct LogFileRequest {
    path: String
}

#[derive(Serialize, Deserialize, Debug)]
struct ErrorResponse {
    message: &'static str
}

#[derive(Debug)]
struct ServerState {
    logs: RefCell<Vec<String>>
}

impl ServerState {
    fn new() -> ServerState {
        ServerState {
            logs: RefCell::new(vec![])
        }
    }

    fn add_log(&self, logfile: String) -> Result<(), ApiError> {
        if self.logs.borrow().contains(&logfile) {
            Err(ApiError::LogfileAlreadyAdded)
        } else {
            self.logs.borrow_mut().push(logfile);
            Ok(())
        }
    }
}

pub fn start_server(settings: &config::Config) {
    let port = settings.get_int("http.bind.port").unwrap();
    let ip = settings.get_str("http.bind.ip").unwrap();
    let addr: std::net::SocketAddr = format!("{}:{}", ip, port).parse().unwrap();

    actix_web::server::new(|| {
        let server_state = Arc::new(ServerState::new());

        vec![
            actix_web::App::with_state(server_state)
                .middleware(actix_web::middleware::Logger::default())
                .prefix("/api/v1")
                .resource("/health", |r| r.get().f(health))
                .resource("/logs", |r| r.post().f(add_log))
                .resource("/logs/{log}", |r| r.get().f(list_log))
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

fn health(_req: &HttpRequest<Arc<ServerState>>) -> HttpResponse {
    HttpResponse::Ok().finish()
}

fn add_log(req: &HttpRequest<Arc<ServerState>>) -> Box<Future<Item=HttpResponse, Error=actix_web::Error>> {
    let path = req.uri().path().to_owned();
    let state: Arc<ServerState> = req.state().clone();

    req.json()
        .from_err()
        .and_then(move |dto: LogFileRequest| {
            state.add_log(dto.path)
                .and_then(|_| {
                    Ok(HttpResponse::Ok()
                        .header(http::header::CONTENT_LOCATION, path + "/x")
                        .finish())
                })
                .or_else(|e| {
                    match e {
                        ApiError::LogfileAlreadyAdded => Ok(HttpResponse::BadRequest()
                            .header(http::header::CONTENT_TYPE, "application/json")
                            .json(ErrorResponse { message: "Log already added" })),
                    }
                })
        })
        .responder()
}

fn list_log(_req: &HttpRequest<Arc<ServerState>>) -> HttpResponse {
    HttpResponse::Ok()
//        .chunked()
        .body(Body::Streaming(Box::new(futures::stream::once(Ok(Bytes::from_static(b"data"))))))
}

#[cfg(test)]
mod tests {
    use actix_web::{http, test};
    use actix_web::Body;
    use std;
    use std::sync::Arc;

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
        let resp = test::TestRequest::with_state(Arc::new(super::ServerState::new())).header("content-type", "text/plain")
            .run(&super::health)
            .unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);
    }

    #[test]
    fn test_list_log_handler() {
        let resp = test::TestRequest::with_state(Arc::new(super::ServerState::new())).header("content-type", "text/plain")
            .run(&super::list_log)
            .unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);

        match resp.body() {
            Body::Streaming(_) => (),
            t => assert!(false, format!("Wrong body type {:?}", t))
        }
    }
}
