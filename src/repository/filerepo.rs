use crate::data::ApplicationError;
use crate::data::LinePattern;
use crate::data::LogFilter;
use crate::data::ParsedLine;
use crate::util;

use crate::data::StreamEntry;
use crate::data::StreamSink;
use chrono::Datelike;
use chrono::TimeZone;
use flate2::read::GzDecoder;
use futures::sink::Sink;
use futures::Async;
use notify;
use notify::Watcher;
use regex::Regex;
use std::cmp::Ordering;
use std::fs;
use std::fs::read_dir;
use std::fs::DirEntry;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Lines;
use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
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

pub struct FileRepository;

impl FileRepository {
    pub fn stream(
        file_pattern: &Regex,
        line_pattern: Arc<LinePattern>,
        tx: StreamSink,
        logfilter: Arc<LogFilter>,
    ) -> Result<(), ApplicationError> {
        let result = Self::resolve_files(&file_pattern, logfilter.from_ms);
        result.map(|files| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let result = files
                    .iter()
                    .map(|file| {
                        Self::stream_file(file, line_pattern.clone(), logfilter.clone(), &tx)
                    })
                    .collect::<Result<Vec<_>, ApplicationError>>();
                result
            });
            ()
        })
    }

    fn stream_file(
        path: &str,
        line_pattern: Arc<LinePattern>,
        logfilter: Arc<LogFilter>,
        tx: &StreamSink,
    ) -> Result<(), ApplicationError> {
        let metadata = fs::metadata(&path).map_err(|_| ApplicationError::FailedToReadSource)?;

        match metadata.is_dir() {
            false => Self::start_file_stream(path, &*line_pattern, logfilter, &tx),
            true => Err(ApplicationError::FailedToReadSource),
        }
    }

    fn watch(path: &str) -> notify::Result<()> {
        let (tx, rx) = mpsc::channel();

        // select implementation for platform
        let mut watcher: notify::RecommendedWatcher =
            notify::Watcher::new(tx, Duration::from_secs(2))?;

        watcher.watch(path, notify::RecursiveMode::NonRecursive)?;

        // This is a simple loop, but you may want to use more complex logic here,
        // for example to handle I/O.
        loop {
            match rx.recv() {
                Ok(event) => println!("{:?}", event),
                Err(e) => println!("watch error: {:?}", e),
            }
        }
    }

    fn start_file_stream(
        path: &str,
        line_pattern: &LinePattern,
        logfilter: Arc<LogFilter>,
        tx: &StreamSink,
    ) -> Result<(), ApplicationError> {
        let file = std::fs::File::open(&path);
        let year = std::fs::metadata(&path)
            .map(|meta| meta.modified())
            .map(|maybe_time| {
                maybe_time
                    .map(|systime| util::system_time_to_date_time(systime).year())
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        let line_pattern = line_pattern.clone();
        let logfilter = logfilter.clone();
        let mut tx = tx.clone();

        file.map_err(|_| ApplicationError::FailedToReadSource)
            .map(|f| {
                let lines: LinesIter = if path.ends_with(".gz") {
                    LinesIter::GZIP(BufReader::new(GzDecoder::new(f)).lines())
                } else {
                    LinesIter::PLAIN(BufReader::new(f).lines())
                };
                for lineresult in lines {
                    match lineresult {
                        Ok(line) => {
                            let parsed_line =
                                FileRepository::apply_pattern(&line, &line_pattern, year);
                            if !logfilter.matches(&parsed_line) {
                                continue;
                            }
                            match tx.poll_ready() {
                                Ok(Async::Ready(_)) => {
                                    if let Err(e) = tx.start_send(StreamEntry { line, parsed_line })
                                    {
                                        error!("Stream error: {:?}", e);
                                        break;
                                    };
                                }
                                Ok(Async::NotReady) => {
                                    if let Err(e) = tx.poll_complete() {
                                        error!("Stream error: {:?}", e);
                                        break;
                                    };
                                    if let Err(e) = tx.start_send(StreamEntry { line, parsed_line })
                                    {
                                        error!("Stream error: {:?}", e);
                                        break;
                                    };
                                }
                                Err(e) => {
                                    error!("Stream error: {:?}", e);
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            error!("Stream error: {:?}", e);
                            break;
                        }
                    }
                }
            })
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
}

#[cfg(test)]
mod tests {
    use crate::repository::filerepo::FileRepository;
    use regex::Regex;

    #[test]
    fn test_resolve_files() {
        let regex = Regex::new(r#"tests/demo\.log(\.(?P<rotation>\d)(\.gz)?)?"#).unwrap();
        let result = FileRepository::resolve_files(&regex, 0).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.get(0), Some(&"tests/demo.log.2.gz".to_string()));
        assert_eq!(result.get(1), Some(&"tests/demo.log.1".to_string()));
        assert_eq!(result.get(2), Some(&"tests/demo.log".to_string()));
    }
}
