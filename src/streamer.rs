use std::fs;
use std::io;

use actix;

use crate::state;
use crate::blocking;

#[derive(Debug)]
pub enum Message {
    StreamFilePath(String),
}

impl actix::Message for Message {
    type Result = core::result::Result<(), io::Error>;
}

pub struct Streamer {
    state: state::ServerState,
    tx: futures::sync::mpsc::Sender<String>,
}

impl Streamer {
    pub fn new(state: state::ServerState, tx: futures::sync::mpsc::Sender<String>) -> Streamer {
        Streamer {
            state,
            tx,
        }
    }
}

impl actix::Handler<Message> for Streamer {
    type Result = core::result::Result<(), io::Error>;

    fn handle(&mut self, msg: Message, _ctx: &mut Self::Context) -> Self::Result {
        let result = match msg {
            Message::StreamFilePath(filepath) => {
                let metadata = fs::metadata(&filepath)?;

                match metadata.is_dir() {
                    false => {
                        // see https://github.com/actix/actix/issues/181
                        let open_result = fs::File::open(filepath);
                        open_result
                            .map(|f| {
                                let tokio_file = tokio::fs::File::from_std(f);
                                self.state.addr_blocking.do_send(blocking::Message::StreamFile(tokio_file, self.tx.clone()));
                            })
                    }
                    true => Err(io::Error::new(io::ErrorKind::NotFound, "Path is a directory"))
                }
            }
        };
        result
    }
}

impl actix::Actor for Streamer {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {}
}
