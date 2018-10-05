use actix;
use actix::Actor;
use actix::AsyncContext;
use actix_web;
use actix_web::AsyncResponder;
use actix_web::Error;
use actix_web::error::ErrorInternalServerError;
use actix_web::Binary;
use actix_web::Body;
use actix_web::http;
use actix_web::HttpMessage;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use app;
use app::ServerState;
use bytes::Bytes;
use config;
use futures;
use futures::Future;
use std;
use std::sync::Arc;
use tokio;
use tokio::fs::File;
use futures::Stream;
use futures::Sink;

const WELCOME_MSG: &'static str = "This is a logtopus tentacle";

#[derive(Serialize, Deserialize, Debug)]
pub struct SourceDTO {
    pub srctype: String,
    pub path: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct ErrorResponse {
    message: &'static str
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
                .resource("/health", |r| {
                    r.get().f(health);
                    r.head().f(|_| HttpResponse::MethodNotAllowed());
                })
                .resource("/sources", |r| {
                    r.post().f(add_source);
                    r.get().f(get_sources);
                    r.head().f(|_| HttpResponse::MethodNotAllowed());
                })
                .resource("/sources/{id}/content", |r| {
                    r.get().f(get_source_content);
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

fn health(_req: &HttpRequest<Arc<ServerState>>) -> HttpResponse {
    HttpResponse::Ok().finish()
}

fn add_source(req: &HttpRequest<Arc<ServerState>>) -> Box<Future<Item=HttpResponse, Error=actix_web::Error>> {
    let path = req.uri().path().to_owned();
    let state: Arc<ServerState> = Arc::clone(req.state());

    let response = req.json().from_err()
        .and_then(move |dto: SourceDTO|
            match app::map_dto_to_source(dto) {
                Ok(src) =>
                    match state.add_source(src) {
                        Ok(_) => Ok(HttpResponse::Created()
                            .header(http::header::CONTENT_LOCATION, path + "/x")
                            .finish()),
                        Err(app::ApiError::SourceAlreadyAdded) => Ok(HttpResponse::BadRequest()
                            .header(http::header::CONTENT_TYPE, "application/json")
                            .json(ErrorResponse { message: "Source already exists" })),
                        Err(_) => Ok(HttpResponse::InternalServerError()
                            .header(http::header::CONTENT_TYPE, "application/json")
                            .json(ErrorResponse { message: "Unexpected Error" })),
                    },
                Err(app::ApiError::UnknownSourceType) => Ok(HttpResponse::BadRequest()
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .json(ErrorResponse { message: "Unknown source type" })),
                Err(_) => Ok(HttpResponse::InternalServerError()
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .json(ErrorResponse { message: "Unexpected Error" })),
            }
        );

    response.responder()
}


fn get_sources(_req: &HttpRequest<Arc<ServerState>>) -> HttpResponse {
    HttpResponse::Ok()
//        .chunked()
        .body(Body::Streaming(Box::new(futures::stream::once(Ok(Bytes::from_static(b"data"))))))
}

struct Streamer {
    tx: futures::sync::mpsc::Sender<String>,
    file: tokio::fs::File
}

impl Streamer {
    pub fn new(tx: futures::sync::mpsc::Sender<String>, file: tokio::fs::File) -> Self {
        Streamer { 
            tx: tx, 
            file: file
        }
    }
}
impl actix::Actor for Streamer {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // let linereader = tokio::codec::FramedRead::new(self.file, tokio::codec::LinesCodec::new());
        // ctx.add_stream(linereader);
    }
}

impl actix::StreamHandler<String, std::io::Error> for Streamer {
    fn handle(&mut self, line: String, _ctx: &mut Self::Context) {
        match self.tx.start_send(line)  {
            Ok(_) => (),
            Err(e) => error!("Stream failure: {}", e)
        }
    }
}

fn get_source_content(_req: &HttpRequest<Arc<ServerState>>) -> Box<Future<Item = HttpResponse, Error = Error>> {
    let response =  File::open("/var/log/alternatives.log")
        .and_then(|f| {
            let (tx, rx_body) = futures::sync::mpsc::channel(1024*1024);
            Streamer::new(tx, f).start();
            Ok(HttpResponse::Ok().streaming(rx_body.map_err(|_| ErrorInternalServerError("Failed stream")).map(|s| Bytes::from(s))))
    });

    response.from_err().responder()
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
            .run(&super::get_source_content)
            .unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);

        match resp.body() {
            Body::Streaming(_) => (),
            t => assert!(false, format!("Wrong body type {:?}", t))
        }
    }
}
