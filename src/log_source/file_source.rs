use crate::data::ApplicationError;
use crate::data::LinePattern;
use crate::data::LogQueryContext;
use crate::data::LogStream;
use crate::data::ParsedLine;
use crate::data::StreamEntry;
use crate::util;
use chrono::Datelike;
use chrono::TimeZone;
use core::pin::Pin;
use flate2::read::GzDecoder;
use futures::stream::Stream;
use futures::task::Context;
use futures::task::Poll;
use futures_util::stream::StreamExt;
use regex::Regex;
use std::cmp::Ordering;
use std::fs;
use std::fs::read_dir;
use std::fs::DirEntry;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Lines;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

enum LinesIter {
    GZIP(Lines<BufReader<GzDecoder<std::fs::File>>>),
    PLAIN(Lines<BufReader<std::fs::File>>),
}

impl Iterator for LinesIter {
    type Item = std::io::Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            LinesIter::GZIP(it) => it.next(),
            LinesIter::PLAIN(it) => it.next(),
        }
    }
}

struct FileLogStream {
    path: String,
    line_pattern: Arc<LinePattern>,
    context: Arc<LogQueryContext>,
    lines_iter: Option<LinesIter>,
    year: i32,
    watch: bool,
}

impl FileLogStream {
    fn new(path: &str, line_pattern: &Arc<LinePattern>, context: &Arc<LogQueryContext>) -> Self {
        FileLogStream {
            path: path.to_owned(),
            line_pattern: line_pattern.clone(),
            context: context.clone(),
            lines_iter: None,
            year: 0,
            watch: false,
        }
    }

    fn with_watch(mut self) -> Self {
        self.watch = true;
        self
    }

    fn apply_pattern(line: &str, line_pattern: &LinePattern, year: i32) -> ParsedLine {
        let maybe_matches = line_pattern.grok.match_against(&line);
        if let Some(matches) = maybe_matches {
            let timestamp = matches
                .get("timestamp")
                .map(|ts| {
                    let parse_result = if line_pattern.syslog_ts {
                        let ts_w_year = format!("{} {}", year, ts);
                        chrono::NaiveDateTime::parse_from_str(&ts_w_year, &line_pattern.chrono)
                    } else {
                        chrono::NaiveDateTime::parse_from_str(&ts, &line_pattern.chrono)
                    };
                    parse_result
                        .map(|ndt| {
                            line_pattern
                                .timezone
                                .from_local_datetime(&ndt)
                                .single()
                                .map(|dt| {
                                    dt.timestamp() as u128 * 1000
                                        + (dt.timestamp_subsec_millis() as u128)
                                })
                                .unwrap_or(0)
                        })
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            ParsedLine {
                timestamp,
                loglevel: matches.get("loglevel").map(|s| s.to_string()),
                message: matches.get("message").unwrap_or("").to_string(),
            }
        } else {
            ParsedLine {
                timestamp: 0,
                loglevel: None,
                message: format!("Failed to parse: {}", line),
            }
        }
    }

    fn next_line(&mut self) -> Poll<Option<Result<StreamEntry, ApplicationError>>> {
        let lines_iter = self.lines_iter.as_mut();
        let lines_iter = lines_iter.unwrap(); // should panic, if this is called with None option
        while let Some(nextline) = lines_iter.next() {
            match nextline {
                Ok(line) => {
                    let parsed_line =
                        FileLogStream::apply_pattern(&line, &self.line_pattern, self.year);
                    if !self.context.matches(&parsed_line) {
                        continue;
                    } else {
                        return Poll::Ready(Some(Ok(StreamEntry { line, parsed_line })));
                    }
                }
                Err(e) => {
                    error!("Stream error: {:?}", e);
                    break;
                }
            }
        }

        if self.watch {
            Poll::Pending // why is this awakened?
        } else {
            Poll::Ready(None)
        }
    }
}

impl Stream for FileLogStream {
    type Item = Result<StreamEntry, ApplicationError>;

    fn poll_next(self: Pin<&mut Self>, _ctx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut inner_self = self.get_mut();
        match &mut inner_self.lines_iter {
            Some(_) => inner_self.next_line(),
            None => {
                inner_self.year = std::fs::metadata(&inner_self.path)
                    .map(|meta| meta.modified())
                    .map(|maybe_time| {
                        maybe_time
                            .map(|systime| util::system_time_to_date_time(systime).year())
                            .unwrap_or(0)
                    })
                    .unwrap_or(0);
                match std::fs::File::open(&inner_self.path) {
                    Ok(file) => {
                        let lines_iter = if inner_self.path.ends_with(".gz") {
                            LinesIter::GZIP(BufReader::new(GzDecoder::new(file)).lines())
                        } else {
                            LinesIter::PLAIN(BufReader::new(file).lines())
                        };
                        inner_self.lines_iter = Some(lines_iter);
                        inner_self.next_line()
                    }
                    Err(e) => {
                        error!("Stream error: {:?}", e);
                        Poll::Ready(None)
                    }
                }
            }
        }
    }
}

pub struct FileSource;

impl FileSource {
    pub fn create_stream(
        file_pattern: &Regex,
        line_pattern: &Arc<LinePattern>,
        context: &Arc<LogQueryContext>,
    ) -> Result<LogStream, ApplicationError> {
        let files = Self::resolve_files(&file_pattern, context.from_ms)?;
        let mut peekable_iter = files.iter().peekable();
        let mut streams = Vec::<FileLogStream>::new();

        while let Some(file) = peekable_iter.next() {
            let metadata = fs::metadata(&file).map_err(|_| ApplicationError::FailedToReadSource);

            let stream = match metadata {
                Ok(meta) if !meta.is_dir() => {
                    let fstream = FileLogStream::new(&file, &line_pattern, &context);
                    // for the last file, add a watch flag if requested
                    let fstream = if let Some(true) = context.watch {
                        if let None = peekable_iter.peek() {
                            fstream.with_watch()
                        } else {
                            fstream
                        }
                    } else {
                        fstream
                    };
                    Ok(fstream)
                }
                _ => Err(ApplicationError::FailedToReadSource),
            }?;
            streams.push(stream);
        }

        let fullstream = futures::stream::iter(streams).flatten();
        Ok(fullstream.boxed_local())
    }

    fn resolve_files(file_pattern: &Regex, from_ms: u128) -> Result<Vec<String>, ApplicationError> {
        let folder = Path::new(file_pattern.as_str())
            .parent()
            .ok_or(ApplicationError::FailedToReadSource)?;

        debug!("Reading folder {:?}", folder);

        let now = SystemTime::now();

        let files_iter = read_dir(folder)
            .map_err(|e| {
                error!("{}", e);
                ApplicationError::FailedToReadSource
            })?
            .filter_map(Result::ok)
            .flat_map(|entry: DirEntry| {
                trace!("Found entry {:?}", entry);
                let t = entry
                    .path()
                    .to_str()
                    .map(|path| {
                        let maybe_matches = file_pattern.captures(path);
                        if let Some(captures) = maybe_matches {
                            if from_ms > 0 {
                                let modtime = fs::metadata(&path)
                                    .map(|meta| meta.modified())
                                    .map(|maybe_time| maybe_time.unwrap_or_else(|_| now))
                                    .unwrap_or_else(|_| now);
                                let modtime_ms = modtime
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .expect("Time went backwards")
                                    .as_millis();
                                if modtime_ms < from_ms {
                                    debug!("{} older than timestamp filter", path);
                                    return None;
                                }
                            }
                            debug!("matching file: {}", path);
                            let rotation_idx = captures
                                .name("rotation")
                                .map(|e| e.as_str().parse::<i32>())
                                .and_then(|r| r.ok());
                            Some((path.to_string(), rotation_idx.unwrap_or(0)))
                        } else {
                            None
                        }
                    })
                    .and_then(|t| t);
                t
            });

        let mut vec: Vec<(String, i32)> = files_iter.collect();

        vec.sort_by(|(path_a, idx_a), (path_b, idx_b)| match idx_b.cmp(idx_a) {
            Ordering::Equal => {
                let modtime_a = fs::metadata(&path_a)
                    .map(|meta| meta.modified())
                    .map(|maybe_time| maybe_time.unwrap_or_else(|_| now))
                    .unwrap_or_else(|_| now);
                let modtime_b = fs::metadata(&path_b)
                    .map(|meta| meta.modified())
                    .map(|maybe_time| maybe_time.unwrap_or_else(|_| now))
                    .unwrap_or_else(|_| now);
                modtime_a.cmp(&modtime_b)
            }
            ord => ord,
        });
        Ok(vec.into_iter().map(|(p, _)| p).collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::log_source::file_source::FileSource;
    use regex::Regex;

    #[test]
    fn test_resolve_files() {
        let regex = Regex::new(r#"tests/demo\.log(\.(?P<rotation>\d)(\.gz)?)?"#).unwrap();
        let result = FileSource::resolve_files(&regex, 0).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.get(0), Some(&"tests/demo.log.2.gz".to_string()));
        assert_eq!(result.get(1), Some(&"tests/demo.log.1".to_string()));
        assert_eq!(result.get(2), Some(&"tests/demo.log".to_string()));
    }
}
