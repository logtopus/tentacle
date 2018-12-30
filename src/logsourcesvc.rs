use std::fs;
use std::io;

use actix;

use crate::blocking;
use crate::state;

#[derive(Debug)]
pub enum LogSourceServiceMessage {
    StreamFilePath(String),
}

impl actix::Message for LogSourceServiceMessage {
    type Result = core::result::Result<(), io::Error>;
}

pub struct LogSourceService {
    state: state::ServerState,
    tx: futures::sync::mpsc::Sender<String>,
}

impl LogSourceService {
    pub fn new(state: state::ServerState, tx: futures::sync::mpsc::Sender<String>) -> LogSourceService {
        LogSourceService {
            state,
            tx,
        }
    }
}

impl actix::Handler<LogSourceServiceMessage> for LogSourceService {
    type Result = core::result::Result<(), io::Error>;

    fn handle(&mut self, msg: LogSourceServiceMessage, _ctx: &mut Self::Context) -> Self::Result {
        let result = match msg {
            LogSourceServiceMessage::StreamFilePath(filepath) => {
                let metadata = fs::metadata(&filepath)?;

                match metadata.is_dir() {
                    false => {
                        // see https://github.com/actix/actix/issues/181
                        let open_result = fs::File::open(filepath);
                        open_result
                            .map(|f| {
                                let tokio_file = tokio::fs::File::from_std(f);
                                self.state.run_blocking(blocking::Message::StreamFile(tokio_file, self.tx.clone()));
                            })
                    }
                    true => Err(io::Error::new(io::ErrorKind::NotFound, "Path is a directory"))
                }
            }
        };
        result
    }
}

impl actix::Actor for LogSourceService {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {}
}
