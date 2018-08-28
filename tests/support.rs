// Test support methods
extern crate actix;
extern crate futures;
extern crate tokio;

use std::panic;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tokio::runtime::current_thread::Runtime;

const RETRY_SLEEPTIME: Duration = Duration::from_millis(1000);

#[derive(Debug)]
pub enum TestError {
    Retry,
    Fail,
}

pub struct TestActorSystem;

impl TestActorSystem {
    pub fn new() -> TestActorSystem {
        let (tx, rx) = mpsc::channel(); // need to transfer spawned system back to this thread
        thread::spawn(move || {
            let sys = actix::System::new("testsystem");
            tx.send(actix::System::current()).unwrap();
            sys.run();
        });
        let sys = rx.recv().unwrap();
        actix::System::set_current(sys);

        TestActorSystem {}
    }
}

impl Drop for TestActorSystem {
    fn drop(&mut self) {
        println!("Dropping integration test actor system");
        actix::System::current().stop();
    }
}

pub fn run_with_retries<R, F>(request: &R, retries: i32, failmsg: &'static str) -> ()
    where
        F: futures::Future<Item=(), Error=TestError>,
        R: Fn() -> F {
    let mut retries = retries;
    let mut runtime = Runtime::new().unwrap();
    while retries >= 0 { // exec at least once
        match runtime.block_on(request()) {
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

pub fn run_test<S, T, U, V>(setup: S, test: T, teardown: U) -> ()
    where
        S: FnOnce() -> V + panic::UnwindSafe,
        T: FnOnce() -> () + panic::UnwindSafe,
        U: FnOnce(&mut V) -> () + panic::UnwindSafe
{
    let mut state = setup();

    let result = panic::catch_unwind(|| {
        test()
    });

    teardown(&mut state);

    result.unwrap();
}
