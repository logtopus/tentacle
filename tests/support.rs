// Test support methods
extern crate actix;
extern crate futures;

use actix_web::client::ClientRequest;
use actix_web::http::StatusCode;
use futures::future::Future;
use futures::FutureExt;
use futures::TryFutureExt;
use std::panic;
use std::thread;
use std::time::Duration;

const RETRY_SLEEPTIME: Duration = Duration::from_millis(1000);

#[derive(Debug)]
pub enum TestError {
    Retry,
    Fail,
}

pub async fn run_with_retries<R, I, F>(request: &R, retries: i32, failmsg: &'static str) -> ()
where
    F: Future<Output = Result<I, TestError>>,
    R: Fn() -> F,
{
    let mut retries = retries;
    while retries >= 0 {
        // exec at least once
        match request().await {
            Ok(_) => break,
            Err(TestError::Fail) => assert!(false, failmsg),
            Err(TestError::Retry) => {
                if retries <= 0 {
                    assert!(false, "Failed, all retries used")
                } else {
                    println!("Retrying, retries left {}", retries);
                    retries -= 1;
                    thread::sleep(RETRY_SLEEPTIME)
                }
            }
        }
    }
}

pub fn binary(name: &str) -> Result<std::path::PathBuf, std::io::Error> {
    let exe = std::env::current_exe();
    Ok(exe?.parent().unwrap().parent().unwrap().join(name))
}

pub fn run_test<S, T, U, V>(setup: S, test: T, teardown: U) -> ()
where
    S: FnOnce() -> V + panic::UnwindSafe,
    T: FnOnce() -> () + panic::UnwindSafe,
    U: FnOnce(&mut V) -> () + panic::UnwindSafe,
{
    let mut state = setup();

    let result = panic::catch_unwind(|| test());

    teardown(&mut state);

    result.unwrap();
}

pub async fn http_request(
    req: ClientRequest,
    expected_status: StatusCode,
) -> impl Future<Output = Result<String, TestError>> {
    req.send()
        .map_ok(move |mut r| {
            assert_eq!(
                r.status(),
                expected_status,
                "Query failed with error code {}",
                r.status()
            );
            r.body().map_err(|_| TestError::Retry)
        })
        .map_err(|_| TestError::Retry)
        .and_then(|body| {
            body.map_ok(move |bytes| {
                std::str::from_utf8(&bytes)
                    .map_err(|_| TestError::Retry)
                    .map(|s| s.to_string())
            })
        })
        .and_then(|s| futures::future::ready(s))
}
