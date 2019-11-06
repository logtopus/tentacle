use std;

use actix_web;
use actix_web::guard;
use actix_web::middleware;
use actix_web::web;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use config;

use crate::constants::*;
use crate::data::LogSource;
use crate::data::LogSourceBuilder;
use crate::logsource_port;
use crate::state::ServerState;

fn parse_source_config(settings: &config::Config, grok: &mut grok::Grok) -> Vec<LogSource> {
    if let Ok(array) = settings.get_array("sources") {
        array
            .iter()
            .map(|v| {
                // let grok = Arc::get_mut(&mut grok).unwrap();
                LogSourceBuilder::create(v, grok).unwrap()
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

    actix_web::HttpServer::new(move || {
        actix_web::App::new()
            .wrap(middleware::Logger::default())
            .data(server_state.clone())
            .service(
                web::scope("/api/v1")
                    .default_service(web::route().to(|| HttpResponse::MethodNotAllowed()))
                    .route("/health", web::get().to_async(health))
                    .route("/sources", web::get().to_async(logsource_port::get_sources))
                    .route(
                        "/sources/{id}/content",
                        web::get()
                            .guard(guard::Header("accept", "*/*"))
                            .to_async(logsource_port::get_source_content_text),
                    )
                    .route(
                        "/sources/{id}/content",
                        web::get()
                            .guard(guard::Header("accept", "text/plain"))
                            .to_async(logsource_port::get_source_content_text),
                    )
                    .route(
                        "/sources/{id}/content",
                        web::get()
                            .guard(guard::Header("accept", "application/json"))
                            .to_async(logsource_port::get_source_content_json),
                    )
                    .route(
                        "/sources/{id}/content",
                        web::get().to_async(|| HttpResponse::NotAcceptable()),
                    ),
            )
            .service(
                web::resource("/index.html")
                    .default_service(web::route().to(|| HttpResponse::MethodNotAllowed()))
                    .route(web::get().to(index)),
            )
    })
    .bind(addr)
    .expect(&format!("Failed to bind to {}:{}", ip, port))
    .start();

    println!("Started http server: {:?}", addr);
}

fn index(_req: HttpRequest) -> String {
    WELCOME_MSG.to_string()
}

fn health(_state: web::Data<ServerState>) -> HttpResponse {
    HttpResponse::Ok().finish()
}

#[cfg(test)]
mod tests {
    use actix_web::http;
    use actix_web::test;
    use actix_web::web;

    #[test]
    fn test_index_handler() {
        let req = test::TestRequest::with_header("content-type", "text/plain").to_http_request();
        let resp = super::index(req);
        assert_eq!(resp, super::WELCOME_MSG);
    }

    #[test]
    fn test_health_handler() {
        let state = super::ServerState::new(vec![], grok::Grok::default());
        let resp = test::block_on(super::health(web::Data::new(state))).unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);
    }
}
