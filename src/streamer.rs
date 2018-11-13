use actix;

pub enum Message {
    StreamFile
}

pub struct Streamer{
    tx: futures::sync::mpsc::Sender<String>
}

impl Streamer {
    pub fn new(tx: futures::sync::mpsc::Sender<String>) -> Streamer {
        Streamer {
            tx
        }
    }
}

//impl actix::StreamHandler<String, std::io::Error> for Streamer {
//    fn handle(&mut self, line: String, _ctx: &mut Self::Context) {
//        match self.tx.start_send(line) {
//            Ok(_) => (),
//            Err(e) => error!("Stream failure: {}", e)
//        }
//    }
//}

// impl actix::Handler<Message> for Streamer {

// }

impl actix::Actor for Streamer {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
//        ctx.add_stream(&self.stream.into_future());

    // let openFut = tokio::fs::File::open(path)
    //          .map(|f| {
    //              let linereader = tokio::codec::FramedRead::new(f, tokio::codec::LinesCodec::new());
    //              Streamer::new(tx, linereader).start();
    //              ()
    //          }).map_err(|e|());
    // let x = tokio::spawn(openFut);
    // openFut.
  //  let result = tokio_threadpool::blocking(|| {

    }


}
