#[macro_use]
extern crate lazy_static;

use actix_web::HttpMessage;
use futures::Future;
use std::sync::RwLock;

mod support;

lazy_static! {
    static ref SERVER_RUNNING: RwLock<bool> = RwLock::new(false);
}

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
                            r.status() == 200,
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

#[test]
fn itest_get_source_content_api() {
    support::run_test(
        setup,
        || {
            fn request() -> impl futures::Future<Item = String, Error = support::TestError> {
                let content_api_request = actix_web::client::ClientRequest::get(
                    "http://localhost:18080/api/v1/sources/itest/content",
                )
                .header("User-Agent", "Actix-web")
                .header("Accept", "text/plain")
                .timeout(std::time::Duration::from_millis(1000))
                .finish()
                .unwrap();

                let result = content_api_request
                    .send()
                    .map_err(|_| support::TestError::Retry)
                    .map(|r| {
                        assert!(
                            r.status() == 200,
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
                                        .map(|s| s.to_string())
                                    // .map(|s| {
                                    //     assert_eq!(s, "2019-01-01 09:00:01 demo1line12019-01-01 10:00:01 demo0line12019-01-01 10:00:02 demo0line22019-01-01 09:00:02 demo1line2");
                                    //     Ok(())
                                    // })
                                }, // flatten result
                            )
                            .and_then(|r| r)
                        }, // flatten result
                    )
                    .and_then(|r| r); // flatten result
                result.map(|s| {
                    assert_eq!(
                        s,
                        r#"2019-01-01 08:00:01 demo2line1
2019-01-01 08:00:02 demo2line2
2019-01-01 08:00:03 demo2line3
2019-01-01 09:00:01 demo1line1
2019-01-01 09:00:02 demo1line2
2019-01-01 10:00:01 demo0line1
2019-01-01 10:00:02 demo0line2
2019-01-01 10:00:03 demo0line3
2019-01-01 10:00:04 demo0line4
"#
                    );
                    s
                })
            }

            support::run_with_retries(&request, 10, "Failed to query source content api")
        },
        teardown,
    )
}

fn setup() -> Option<std::process::Child> {
    let mut wlock = SERVER_RUNNING.write().unwrap();
    if *wlock {
        return None;
    }

    let exe = support::binary("tentacle").unwrap();
    let process = std::process::Command::new(exe)
        .arg("--config=tests/integrationtests.yml")
        .spawn()
        .expect("Failed to run server");

    (*wlock) = true;
    Some(process)
}

fn teardown(maybe_server: &mut Option<std::process::Child>) {
    println!("Stopping tentacle server");

    if let Some(server) = maybe_server {
        let mut wlock = SERVER_RUNNING.write().unwrap();
        server.kill().unwrap();
        (*wlock) = false;
    }
}
