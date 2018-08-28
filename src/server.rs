use actix_web;
use actix_web::HttpRequest;
use config;
use std;

pub fn start_server(settings: &config::Config) {
    let port = settings.get_int("http.bind.port").unwrap();
    let ip = settings.get_str("http.bind.ip").unwrap();
    let addr: std::net::SocketAddr = format!("{}:{}", ip, port).parse().unwrap();

    actix_web::server::new(|| {
        actix_web::App::new()
            // enable logger
            .middleware(actix_web::middleware::Logger::default())
            .resource("/index.html", |r| r.f(index))
            .resource("/health", |r| r.get().f(health))
    })
        .bind(addr).expect(&format!("Failed to bind to {}:{}", ip, port))
        .start();

    println!("Started http server: {:?}", addr);
}

fn index(_req: &HttpRequest) -> &'static str {
    "Hello world!"
}

fn health(_req: &HttpRequest) -> &'static str {
    "OK"
}


#[cfg(test)]
mod tests {
    use actix_web::{http, test};
//    use std;

    #[test]
    fn test_health_api() {
        let resp = test::TestRequest::with_header("content-type", "text/plain")
            .run(&super::health)
            .unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);
//        assert_eq!(std::str::from_utf8(resp.body()), "OK") // TODO how to consume the body?
    }
}
