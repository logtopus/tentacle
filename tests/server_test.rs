#[macro_use]
extern crate lazy_static;

use actix_web::http::StatusCode;
use futures::Future;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::Weak;
mod support;

struct ProcessHolder {
    process: std::process::Child,
}

impl Drop for ProcessHolder {
    fn drop(&mut self) {
        println!("Stopping tentacle server"); // only shown with --nocapture flag
        self.process.kill().unwrap();
    }
}

lazy_static! {
    static ref SERVER_RUNNING: RwLock<Option<Weak<ProcessHolder>>> = RwLock::new(None);
}

#[test]
fn itest_health_api() {
    support::run_test(
        setup,
        || {
            support::run_with_retries(
                &|| {
                    let req = actix_web::client::ClientRequest::get(
                        "http://localhost:18080/api/v1/health",
                    )
                    .header("User-Agent", "Actix-web")
                    .header("Accept", "*/*")
                    .timeout(std::time::Duration::from_millis(1000))
                    .finish()
                    .unwrap();

                    support::http_request(req, StatusCode::OK).map(|s| assert_eq!(s, ""))
                },
                10,
                "Failed to query health api",
            )
        },
        teardown,
    )
}

#[test]
fn itest_get_source_content_api() {
    support::run_test(
        setup,
        || {
            support::run_with_retries(
                &|| {
                    let req = actix_web::client::ClientRequest::get(
                        "http://localhost:18080/api/v1/sources/itest/content",
                    )
                    .header("User-Agent", "Actix-web")
                    .header("Accept", "*/*")
                    .timeout(std::time::Duration::from_millis(1000))
                    .finish()
                    .unwrap();

                    let result = support::http_request(req, StatusCode::OK);
                    result.map(|s| {
                        assert_eq!(
                            s,
                            r#"2019-01-01 08:00:01 ERROR demo2line1
2019-01-01 08:00:02 DEBUG demo2line2
2019-01-01 08:00:03 INFO demo2line3
2019-01-01 09:00:01 WARNING demo1line1
2019-01-01 09:00:02 DEBUG demo1line2
2019-01-01 10:00:01 DEBUG demo0line1
2019-01-01 10:00:02 DEBUG demo0line2
2019-01-01 10:00:03 ERROR demo0line3
2019-01-01 10:00:04 INFO demo0line4
"#
                        )
                    })
                },
                10,
                "Failed to query source content api",
            )
        },
        teardown,
    )
}

#[test]
fn itest_get_source_content_api_with_logfilter() {
    support::run_test(
        setup,
        || {
            support::run_with_retries(
                &|| {
                    let req = actix_web::client::ClientRequest::get(
                        "http://localhost:18080/api/v1/sources/itest/content?loglevels=DEBUG",
                    )
                    .header("User-Agent", "Actix-web")
                    .header("Accept", "*/*")
                    .timeout(std::time::Duration::from_millis(1000))
                    .finish()
                    .unwrap();

                    let result = support::http_request(req, StatusCode::OK);
                    result.map(|s| {
                        assert_eq!(
                            s,
                            r#"2019-01-01 08:00:02 DEBUG demo2line2
2019-01-01 09:00:02 DEBUG demo1line2
2019-01-01 10:00:01 DEBUG demo0line1
2019-01-01 10:00:02 DEBUG demo0line2
"#
                        )
                    })
                },
                10,
                "Failed to query source content api",
            )
        },
        teardown,
    )
}

fn setup() -> Arc<ProcessHolder> {
    let mut wlock = SERVER_RUNNING.write().unwrap();
    match &mut *wlock {
        Some(s) => s.upgrade().unwrap().clone(),
        None => {
            let exe = support::binary("tentacle").unwrap();
            let process = std::process::Command::new(exe)
                .arg("--config=tests/integrationtests.yml")
                .spawn()
                .expect("Failed to run server");

            let arc = Arc::new(ProcessHolder { process });
            (*wlock) = Some(Arc::downgrade(&arc));
            std::thread::sleep(std::time::Duration::from_millis(1000));
            arc
        }
    }
}

fn teardown(_: &mut Arc<ProcessHolder>) {}
