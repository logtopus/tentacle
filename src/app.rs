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
    #[fail(display = "Failed to store source")]
    FailedToWriteSource,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd, Clone)]
pub enum SourceType {
    File,
    Url,
    Journal, // see https://docs.rs/systemd/0.0.8/systemd/journal/index.html
}

#[derive(Debug, PartialEq, PartialOrd)]
pub struct Source {
    srctype: SourceType,
    path: String,
}

impl Source {
    pub fn into_dto(&self) -> server::SourceDTO {
        server::SourceDTO {
            srctype: self.srctype.clone(),
            path: self.path.clone(),
        }
    }
}

impl From<server::SourceDTO> for Source {
    fn from(dto: server::SourceDTO) -> Self {
        Source {
            srctype: dto.srctype,
            path: dto.path,
        }
    }
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
            }
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string()))
        }
    }

    pub fn add_source(&mut self, source: Source) -> Result<(), ApiError> {
        let mut locked_vec = self.sources.write().map_err(|_| ApiError::FailedToWriteSource)?;

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
