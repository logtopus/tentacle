#[macro_use]
extern crate log;

use clap::{App, Arg, ArgMatches};

fn main() {
    let cli_matches = parse_cli();

    init_log(&cli_matches);

    tentacle::run(cli_matches.value_of("config"));
}

fn init_log(matches: &ArgMatches) {
    let loglevel = match matches.occurrences_of("v") {
        0 => "error",
        1 => "warn",
        2 => "info",
        3 => "debug",
        _ => "trace"
    };

    let loglevel = match matches.value_of("module") {
        Some(module) => {
            let mut module_loglevel = String::from(module);
            module_loglevel.push_str("=");
            module_loglevel.push_str(loglevel);
            module_loglevel
        }
        _ => String::from(loglevel)
    };

    std::env::set_var("RUST_LOG", &loglevel);
    env_logger::init();
    debug!("Setting log level to {}", &loglevel);
}

fn parse_cli() -> clap::ArgMatches<'static> {
    App::new("Logtopus tentacle")
        .version(tentacle::version())
        .author(tentacle::authors())
        .about("Provide logfile access for logtopus server")
        .arg(Arg::with_name("config")
            // .required(true)
            .short("c")
            .long("config")
            .value_name("FILE")
            .help("Sets the configuration file name")
            .takes_value(true))
        .arg(Arg::with_name("module")
            .short("m")
            .long("module")
            .takes_value(true)
            .help("Sets the optional name of the module for which to set the verbosity level"))
        .arg(Arg::with_name("v")
            .short("v")
            .multiple(true)
            .help("Level of verbosity (error is default) if used multiple times: warn(v), info(vv), debug(vvv) and trace(vvvv)"))
        .get_matches()
}
