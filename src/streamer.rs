use actix;
use actix::AsyncContext;
use futures::Future;
use futures::Sink;
use futures::Stream;

use crate::state;
use std::io;

#[derive(Debug)]
pub enum Message {
    StreamFilePath(String),
    StreamFile(tokio::fs::File),
}

impl actix::Message for Message {
    type Result = ();
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
    type Result = ();

    fn handle(&mut self, msg: Message, ctx: &mut Self::Context) {
        let addr = ctx.address().clone();
        match msg {
            Message::StreamFilePath(filepath) => {
//                let path = std::path::Path::new(&(filepath.clone()));
                let open_fut = tokio::fs::File::open(filepath)
                    .map(move |file| {
                        addr.do_send(Message::StreamFile(file));
                        ()
                    }).map_err(|e| error!("File open error: {:?}", e));

                let r = self.state.spawn_blocking(open_fut);
                if let Err(e) = r {
                    error!("Spawn error: {:?}", e);
                }
                ()
            }
            Message::StreamFile(file) => {
                debug!("Starting stream for file handle: {:?}", file);
                let linereader = tokio::codec::FramedRead::new(file, tokio::codec::LinesCodec::new_with_max_length(2048));
                let mut tx = self.tx.clone();
// Not working, since file reading requires extra tokio runtime
// ctx.add_stream(linereader);
                let r = self.state.spawn_blocking(linereader
                    .for_each(move |s| {
                        match tx.start_send(s + "\n") {
                            Ok(_) => Ok(()),
                            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string()))
                        }
                    })
                    .map_err(|e| {
                        error!("Stream error: {:?}", e);
                    })
                );
                if let Err(e) = r {
                    error!("Spawn error: {:?}", e);
                }
                ()
            }
        }
    }
}
//
//impl actix::StreamHandler<String, std::io::Error> for Streamer {
//    fn handle(&mut self, line: String, _ctx: &mut Self::Context) {
//        println!("{:?}", line);
//        match self.tx.start_send(line) {
//            Ok(_) => (),
//            Err(e) => error!("Stream failure: {}", e)
//        }
//    }
//
//    fn finished(&mut self, ctx: &mut Self::Context) {
//        println!("Stream finished")
//    }
//}

impl actix::Actor for Streamer {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {}
}
