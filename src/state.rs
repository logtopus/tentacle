use failure::Fail;
use server;
use std::sync::Arc;
use std::sync::RwLock;
use tokio::runtime::Runtime;
use futures::Future;
use std::sync::Mutex;
use std::io;
use uuid::Uuid;

type Sources = Arc<RwLock<Vec<Source>>>;

#[derive(Fail, Debug)]
pub enum ApplicationError {
    #[fail(display = "Source already exists")]
    SourceAlreadyAdded,
    #[fail(display = "Failed to store source")]
    FailedToWriteSource,
    #[fail(display = "Failed to read source")]
    FailedToReadSource,
    #[fail(display = "Missing attribute: {}", attr)]
    MissingAttribute { attr: String },
}

#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
pub enum SourceType {
    File,
    Url,
    Journal, // see https://docs.rs/systemd/0.0.8/systemd/journal/index.html
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
pub enum Source {
    File { key: String, path: String },
    Url,
    Journal,
}

impl Source {
    pub fn try_from_spec(dto: server::SourceSpec) -> Result<Source, ApplicationError> {
        match dto.src_type {
            SourceType::File => {
                match dto.path {
                    Some(p) => Ok(Source::File {
                        key: dto.key.unwrap_or_else(|| Uuid::new_v4().to_string()),
                        path: p,
                    }),
                    None => Err(ApplicationError::MissingAttribute { attr: "path".to_string() })
                }
            }
            SourceType::Url =>
                unimplemented!(),
            SourceType::Journal =>
                unimplemented!()
        }
    }

    pub fn into_repr(src: &Source) -> server::SourceRepr {
        match src {
            Source::File { key, path } =>
                server::SourceRepr {
                    key: key.clone(),
                    src_type: SourceType::File,
                    path: Some(path.clone()),
                },
            Source::Url =>
                unimplemented!(),
            Source::Journal =>
                unimplemented!()
        }
    }
}

pub struct ServerState {
    sources: Sources,
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

    pub fn add_source(&mut self, source: Source) -> Result<String, ApplicationError> {
        let key = Self::extract_source_key(&source);
        let mut locked_vec = self.sources.write().map_err(|_| ApplicationError::FailedToWriteSource)?;

        if locked_vec.iter().find(|src| Self::extract_source_key(src) == key).is_some() {
            Err(ApplicationError::SourceAlreadyAdded)
        } else {
            let key = ServerState::extract_source_key(&source);
            locked_vec.push(source);
            Ok(key)
        }
    }

    fn extract_source_key(source: &Source) -> String {
        match source {
            Source::File { key, path: _ } => key.clone(),
            Source::Url =>
                unimplemented!(),
            Source::Journal =>
                unimplemented!(),
        }
    }

    pub fn get_sources(&self) -> Sources {
        self.sources.clone()
    }

    pub fn get_source<'a>(&'a self, key: &'a str) -> Result<Option<Source>, ApplicationError> {
        let locked_vec = self.sources.read().map_err(|_| ApplicationError::FailedToReadSource)?;
        let maybe_src = locked_vec.iter().find(|src| Self::extract_source_key(src) == key);

        Ok(maybe_src.map(|s| (*s).clone()))
    }
}
