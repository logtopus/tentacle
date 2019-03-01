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
    support::run_test(
        setup,
        || {
            fn request() -> impl futures::Future<Item = (), Error = support::TestError> {
                let health_api_request =
                    actix_web::client::ClientRequest::get("http://localhost:18080/api/v1/health")
                        .header("User-Agent", "Actix-web")
                        .timeout(std::time::Duration::from_millis(1000))
                        .finish()
                        .unwrap();

                health_api_request
                    .send()
                    .map_err(|_| support::TestError::Retry)
                    .map(|r| {
                        assert!(
                            r.status() == http::StatusCode::OK,
                            format!("Query failed with error code {}", r.status())
                        );
                        r.body()
                    })
                    .map(
                        |body| {
                            body.map_err(|e| {
                                assert!(false, e);
                                support::TestError::Fail
                            })
                            .map(
                                |bytes| {
                                    std::str::from_utf8(&bytes)
                                        .map_err(|e| {
                                            assert!(false, e);
                                            support::TestError::Fail
                                        })
                                        .map(|s| {
                                            assert!(s == "");
                                            Ok(())
                                        })
                                        .and_then(|r| r)
                                }, // flatten result
                            )
                            .and_then(|r| r)
                        }, // flatten result
                    )
                    .and_then(|r| r) // flatten result
            }

            support::run_with_retries(&request, 10, "Failed to query health api")
        },
        teardown,
    )
}

fn setup() -> std::process::Child {
    let exe = support::binary("tentacle").unwrap();

    let server = std::process::Command::new(exe)
        .arg("--config=tests/integrationtests.yml")
        .spawn()
        .expect("Failed to run server");
    server
}

fn teardown(server: &mut std::process::Child) {
    println!("Stopping tentacle server");
    server.kill().unwrap();
}
