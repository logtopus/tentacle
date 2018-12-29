use std::io;

use actix;
use futures::Future;
use futures::Sink;
use futures::Stream;
use tokio::runtime::Runtime;

#[derive(Debug)]
pub enum Message {
    StreamFile(tokio::fs::File, futures::sync::mpsc::Sender<String>),
}

impl actix::Message for Message {
    type Result = core::result::Result<(), io::Error>;
}

/// Actor which spawns its own tokio runtime, should normally be used as a singleton.
/// Handles jobs of blocking nature, like streaming from a file.
pub struct BlockingSpawner {
    blk_rt: Runtime,
}

impl BlockingSpawner {
    /// Creates a new actor and a new tokio runtime.
    pub fn new() -> Self {
        BlockingSpawner {
            blk_rt: Runtime::new().unwrap(),
        }
    }

    fn spawn(
        &mut self,
        f: impl Future<Item = (), Error = ()> + Send + 'static,
    ) -> Result<(), io::Error> {
        self.blk_rt.spawn(f);
        Ok(())
    }
}

impl actix::Actor for BlockingSpawner {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {}
}

impl actix::Handler<Message> for BlockingSpawner {
    type Result = core::result::Result<(), io::Error>;

    fn handle(&mut self, msg: Message, _ctx: &mut Self::Context) -> Self::Result {
        match msg {
            Message::StreamFile(file, tx) => {
                debug!("Starting stream for file handle: {:?}", file);
                let linereader = tokio::codec::FramedRead::new(
                    file,
                    tokio::codec::LinesCodec::new_with_max_length(2048),
                );
                self.spawn(
                    linereader
                        .forward(
                            tx.sink_map_err(|e| {
                                io::Error::new(io::ErrorKind::Other, e.to_string())
                            }),
                        )
                        .map(|_| ())
                        .map_err(|e| {
                            error!("Stream error: {:?}", e);
                        }),
                )
            }
        }
    }
}
