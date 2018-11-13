use failure::Fail;
use server;
use std::sync::Arc;
use std::sync::RwLock;

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
    sources: Arc<RwLock<Vec<Source>>>
    // blocking_pool: Arc<ThreadPool>
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        ServerState {
            sources: self.sources.clone()
            // blocking_pool: self.blocking_pool.clone()
        }
    }
}

impl ServerState {
    pub fn new() -> ServerState {
        ServerState {
            sources: Arc::new(RwLock::new(vec![]))
            // blocking_pool: Arc::new(ThreadPool::new())
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
