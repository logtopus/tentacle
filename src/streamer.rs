use std::fs;
use std::io;

use actix;
use actix::AsyncContext;
use futures::Future;
use futures::Sink;
use futures::Stream;

use crate::state;

#[derive(Debug)]
pub enum Message {
    StreamFilePath(String),
    StreamFile(tokio::fs::File),
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

    fn handle(&mut self, msg: Message, ctx: &mut Self::Context) -> Self::Result {
        let addr = ctx.address().clone();
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
                                addr.do_send(Message::StreamFile(tokio_file))
                            })
                    }
                    true => Err(io::Error::new(io::ErrorKind::NotFound, "Path is a directory"))
                }
            }

            Message::StreamFile(file) => {
                debug!("Starting stream for file handle: {:?}", file);
                let linereader = tokio::codec::FramedRead::new(file, tokio::codec::LinesCodec::new_with_max_length(2048));
                let mut tx = self.tx.clone();
                self.state.spawn_blocking(linereader
                    .for_each(move |s| {
                        match tx.start_send(s + "\n") {
                            Ok(_) => Ok(()),
                            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string()))
                        }
                    })
                    .map_err(|e| {
                        error!("Stream error: {:?}", e);
                    })
                )
            }
        };
        result
    }
}

impl actix::Actor for Streamer {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {}
}
