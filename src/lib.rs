extern crate actix;
extern crate actix_web;
extern crate bytes;
extern crate config;
extern crate failure;
extern crate futures;
extern crate http;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
extern crate tokio;
extern crate tokio_threadpool;

use crate::constants::AUTHORS;
use crate::constants::VERSION;

mod constants;
mod cfg;
mod server;
mod state;
mod streamer;
mod blocking;

pub fn version() -> &'static str {
    VERSION
}

pub fn authors() -> &'static str {
    AUTHORS
}

pub fn run(maybe_configfile: Option<&str>) {
    let settings = match cfg::read_config(&maybe_configfile) {
        Ok(config) => config,
        Err(msg) => {
            println!("Error: {}", msg);
            std::process::exit(1)
        }
    };

    let sys = actix::System::new("tentacle");

    server::start_server(&settings);

//    println!("\nConfiguration\n\n{:?} \n\n-----------",
//             settings.try_into::<HashMap<String, config::Value>>().unwrap());

    sys.run();
}