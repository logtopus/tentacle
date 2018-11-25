use failure::Fail;
use server;
use std::sync::Arc;
use std::sync::RwLock;
use tokio::runtime::Runtime;
use futures::Future;
use std::sync::Mutex;
use std::io;

#[derive(Fail, Debug)]
pub enum ApiError {
    #[fail(display = "Source already exists")]
    SourceAlreadyAdded,
    #[fail(display = "Unknown source type")]
    UnknownSourceType,
}

#[derive(Debug, PartialEq, PartialOrd)]
enum SourceType {
    File,
    Url,
    Journal, // see https://docs.rs/systemd/0.0.8/systemd/journal/index.html
}

#[derive(Debug, PartialEq, PartialOrd)]
pub struct Source {
    srctype: SourceType,
    path: String,
}

// TODO: better use TryFrom trait if stable
pub fn map_dto_to_source(dto: server::SourceDTO) -> Result<Source, ApiError> {
    match dto.srctype.as_ref() {
        "file" => Ok(SourceType::File),
        "url" => Ok(SourceType::Url),
        "journal" => Ok(SourceType::Journal),
        _ => Err(ApiError::UnknownSourceType)
    }.map(|t| Source { srctype: t, path: dto.path })
}

pub struct ServerState {
    sources: Arc<RwLock<Vec<Source>>>,
    blk_rt: Arc<Mutex<Runtime>>,
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        ServerState {
            sources: self.sources.clone(),
            blk_rt: self.blk_rt.clone(),
        }
    }
}

impl ServerState {
    pub fn new() -> ServerState {
        ServerState {
            sources: Arc::new(RwLock::new(vec![])),
            blk_rt: Arc::new(Mutex::new(Runtime::new().unwrap())),
        }
    }

    pub fn spawn_blocking(&self, f: impl Future<Item=(), Error=()> + Send + 'static) -> Result<(), io::Error> {
        let mutex = self.blk_rt.lock();
        match mutex {
            Ok(mut rt) => {
                (*rt).spawn(f);
                Ok(())
            },
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, "oh no!"))
        }
    }

    pub fn add_source(&mut self, source: Source) -> Result<(), ApiError> {
        let mut locked_vec = self.sources.write().unwrap();

        if locked_vec.contains(&source) {
            Err(ApiError::SourceAlreadyAdded)
        } else {
            locked_vec.push(source);
            Ok(())
        }
    }

    pub fn get_sources(&self) -> Arc<RwLock<Vec<Source>>> {
        self.sources.clone()
    }
}
