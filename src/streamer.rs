use actix;
use actix::AsyncContext;
use futures::Future;
use futures::Sink;

use app;

#[derive(Debug)]
pub enum Message {
    StreamFilePath(String),
    StreamReader(tokio::codec::FramedRead<tokio::fs::File, tokio::codec::LinesCodec>),
    StreamFile(tokio::fs::File),
}

impl actix::Message for Message {
    type Result = ();
}

pub struct Streamer {
    state: app::ServerState,
    tx: futures::sync::mpsc::Sender<String>,
}

impl Streamer {
    pub fn new(state: app::ServerState, tx: futures::sync::mpsc::Sender<String>) -> Streamer {
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
                let path = std::path::Path::new("/var/log/alternatives.log");
                let open_fut = tokio::fs::File::open(path)
                    .map(move |f| {
                        addr.do_send(Message::StreamFile(f));
                        ()
                    }).map_err(|e| println!("{:?}", e));

                self.state.spawn_blocking(open_fut);
            }
            Message::StreamFile(file) => {
                let linereader = tokio::codec::FramedRead::new(file, tokio::codec::LinesCodec::new_with_max_length((2048)));
                ctx.add_stream(linereader);
            }
        }
    }
}

impl actix::StreamHandler<String, std::io::Error> for Streamer {
    fn handle(&mut self, line: String, _ctx: &mut Self::Context) {
        println!("{:?}", line);
        match self.tx.start_send(line) {
            Ok(_) => (),
            Err(e) => error!("Stream failure: {}", e)
        }
    }

    fn finished(&mut self, ctx: &mut Self::Context) {
        println!("Stream finished")
    }
}

impl actix::Actor for Streamer {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {}
}
