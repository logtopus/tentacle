use server;
use std::cell::RefCell;

pub enum ApiError {
    SourceAlreadyAdded,
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

#[derive(Debug, PartialEq, PartialOrd)]
pub struct ServerState {
    sources: RefCell<Vec<Source>>
}

impl ServerState {
    pub fn new() -> ServerState {
        ServerState {
            sources: RefCell::new(vec![])
        }
    }

    pub fn add_source(&self, source: Source) -> Result<(), ApiError> {
        if self.sources.borrow().contains(&source) {
            Err(ApiError::SourceAlreadyAdded)
        } else {
            self.sources.borrow_mut().push(source);
            Ok(())
        }
    }
}

