extern crate actix;
extern crate actix_web;
extern crate bytes;
extern crate futures;
extern crate http;

use actix_web::HttpMessage;
use futures::Future;

mod support;

#[test]
fn itest_health_api() {
    support::run_test(setup, || {
        fn request() -> impl futures::Future<Item=(), Error=support::TestError> {
            actix_web::client::ClientRequest::get("http://localhost:8080/health")
                .header("User-Agent", "Actix-web")
                .timeout(std::time::Duration::from_millis(1000))
                .finish()
                .unwrap()
                .send()
                .map_err(|_| support::TestError::Retry)
                .and_then(|response| {
                    assert!(response.status() == http::StatusCode::OK);
                    response.body()
                        .map(|bytes| {
                            actix::System::current().stop();
                            std::str::from_utf8(&bytes)
                                .map_err(|_| support::TestError::Fail)
                                .map(|s| match s {
                                    "OK" => Ok(()),
                                    _ => Err(support::TestError::Fail)
                                })
                        })
                        .map_err(|_| support::TestError::Fail)
                })
                .and_then(|r| r) // flatten result
                .and_then(|r| r) // flatten result
        };

        support::run_with_retries(&request, 10, "Failed to query health api")
    }, teardown)
}

fn setup() -> std::process::Child {
    let server = std::process::Command::new("target/debug/tentacle").arg("--config=tests/integrationtests.yml").spawn().expect("Failed to run server");
    server
}

fn teardown(server: &mut std::process::Child) {
    println!("Stopping tentacle server");
    server.kill().unwrap();
}
