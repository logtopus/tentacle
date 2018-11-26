use actix;
use actix::Actor;
use actix_web;
use actix_web::Binary;
use actix_web::Body;
use actix_web::error::ResponseError;
use actix_web::http;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use app;
use app::ServerState;
use bytes::Bytes;
use config;
use futures;
use futures::Future;
use futures::Stream;
use std;
use streamer;

const WELCOME_MSG: &'static str = "This is a logtopus tentacle";

impl ResponseError for app::ApiError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            app::ApiError::SourceAlreadyAdded => {
                HttpResponse::BadRequest()
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .json(ErrorResponse { message: app::ApiError::SourceAlreadyAdded.to_string() })
            }
            app::ApiError::UnknownSourceType => {
                HttpResponse::BadRequest()
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .json(ErrorResponse { message: app::ApiError::UnknownSourceType.to_string() })
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SourceDTO {
    pub srctype: String,
    pub path: String,
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
//                    r.get().f(get_sources);
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

fn add_source((json, state): (actix_web::Json<SourceDTO>, actix_web::State<ServerState>)) -> HttpResponse {
    let response =
        match app::map_dto_to_source(json.into_inner()) {
            Ok(src) => {
                let mut state: ServerState = state.clone();

                match state.add_source(src) {
                    Ok(_id) =>
                        HttpResponse::Created()
                            .header(http::header::CONTENT_LOCATION, "/x")
                            .finish(),
                    Err(e) => e.error_response()
                }
            }
            Err(e) => e.error_response()
        };

    response
}


//fn get_sources(req: &HttpRequest<ServerState>) -> HttpResponse {
//    let state: Arc<ServerState> = Arc::clone(req.state());
//
//    HttpResponse::Ok()
////        .chunked()
//        .body(Body::Streaming(Box::new(futures::stream::once(Ok(Bytes::from_static(b"data"))))))
//}


fn get_source_content(state: actix_web::State<ServerState>) -> HttpResponse //actix_web::FutureResponse<HttpResponse>
{
    let (tx, rx_body) = futures::sync::mpsc::channel(1024 * 1024);
    let streamer = streamer::Streamer::new(state.clone(), tx).start();
    let request = streamer.send(streamer::Message::StreamFilePath("bla".to_string()));

    actix::spawn(request.map_err(|e| println!("Streaming Actor has probably died: {}", e)));

    HttpResponse::Ok().streaming(rx_body
        .map_err(|_| actix_web::error::PayloadError::Incomplete)
        .map(|s| Bytes::from(s)))
//            .map(|res| {
//                match res {
//                    Ok(result) => println!("Got result: {}", result),
//                    Err(err) => println!("Got error: {}", err),
//                }
//            })
//            .map_err(|e| {
//                println!("Actor is probably died: {}", e);
//            }));
    //     .map_err(error::Error::from)
    //     .and_then(|res| {
    //         match res {
    //             Ok(result) =>
    //                 HttpResponse::Ok().streaming(rx_body
    //                     .map_err(|_| actix_web::error::PayloadError::Incomplete)
    //                     .map(|s| Bytes::from(s))),
    //             Err(err) => Err(
    //                 error!("Failed to stream file content: BlockingError");
    //                 HttpResponse::BadRequest()
    //                     .header(http::header::CONTENT_TYPE, "application/json")
    //                     .json(ErrorResponse { message: e.to_string() })
    //                     error::ErrorInternalServerError("Something went wrong! :("))
    //         })


//        .responder()
}

#[cfg(test)]
mod tests {
    use actix_web::{http, test};
    use actix_web::Body;
    use actix_web::FromRequest;
    use actix_web::State;
    use std;

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

    #[test]
    fn test_get_source_content_handler() {
        let state = super::ServerState::new();
        let req = test::TestRequest::with_state(state).header("content-type", "text/plain").finish();
        let resp = super::get_source_content(State::extract(&req));
        assert_eq!(resp.status(), http::StatusCode::OK, "Response was {:?}", resp.body());

        match resp.body() {
            Body::Streaming(_) => (),
            t => assert!(false, format!("Wrong body type {:?}", t))
        }
    }
}
