use crate::data::ApplicationError;
use crate::data::LinePattern;
use crate::data::LogFilter;
use crate::data::ParsedLine;
use crate::data::StreamEntry;

use chrono::DateTime;
use chrono::Datelike;
use chrono::NaiveDateTime;
use chrono::TimeZone;
use chrono::Utc;
use flate2::read::GzDecoder;
use futures::sink::Sink;
use futures::sync::mpsc::Sender;
use futures::Async;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Lines;
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

pub struct StreamLogFile {
    pub path: String,
    pub line_pattern: LinePattern,
    pub filter: Arc<LogFilter>,
    pub tx: Sender<StreamEntry>,
}

impl actix::Message for StreamLogFile {
    type Result = Result<(), ApplicationError>;
}

pub struct LogFileStreamer;

impl LogFileStreamer {
    fn system_time_to_date_time(t: SystemTime) -> NaiveDateTime {
        let (sec, nsec) = match t.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(dur) => (dur.as_secs() as i64, dur.subsec_nanos()),
            Err(e) => {
                // unlikely but should be handled
                let dur = e.duration();
                let (sec, nsec) = (dur.as_secs() as i64, dur.subsec_nanos());
                if nsec == 0 {
                    (-sec, 0)
                } else {
                    (-sec - 1, 1_000_000_000 - nsec)
                }
            }
        };
        NaiveDateTime::from_timestamp(sec, nsec)
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

impl actix::Actor for LogFileStreamer {
    type Context = actix::sync::SyncContext<Self>;
}

impl actix::Handler<StreamLogFile> for LogFileStreamer {
    type Result = Result<(), ApplicationError>;

    fn handle(&mut self, msg: StreamLogFile, _: &mut Self::Context) -> Self::Result {
        let file = std::fs::File::open(&msg.path);
        let year = std::fs::metadata(&msg.path)
            .map(|meta| meta.modified())
            .map(|maybe_time| {
                maybe_time
                    .map(|systime| LogFileStreamer::system_time_to_date_time(systime).year())
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        let line_pattern = msg.line_pattern.clone();
        let logfilter = msg.filter.clone();
        let mut tx = msg.tx.clone();

        file.map_err(|_| ApplicationError::FailedToReadSource)
            .map(|f| {
                let lines: LinesIter = if msg.path.ends_with(".gz") {
                    LinesIter::GZIP(BufReader::new(GzDecoder::new(f)).lines())
                } else {
                    LinesIter::PLAIN(BufReader::new(f).lines())
                };
                for lineresult in lines {
                    match lineresult {
                        Ok(line) => {
                            let parsed_line =
                                LogFileStreamer::apply_pattern(&line, &line_pattern, year);
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
}

#[cfg(test)]
mod tests {
    use crate::data::LinePattern;
    use crate::log_streamer::LogFileStreamer;
    use chrono_tz::UTC;
    use std::sync::Arc;

    #[test]
    fn test_apply_pattern() {
        let mut grok_default = grok::Grok::default();

        let raw = "%{TIMESTAMP_ISO8601:timestamp} %{GREEDYDATA:message}";
        let grok = Arc::new(grok_default.compile(raw, true).unwrap());
        let chrono = Arc::new("%Y-%m-%d %H:%M:%S".to_string());
        let line_pattern = LinePattern {
            raw: raw.to_string(),
            grok,
            chrono: chrono.clone(),
            timezone: UTC,
            syslog_ts: false,
        };

        let line = "2018-01-01 12:39:01 first message";
        let parsed_line = LogFileStreamer::apply_pattern(line, &line_pattern, 2018);
        assert_eq!(1514810341000, parsed_line.timestamp);
        assert_eq!("first message", parsed_line.message);

        let grok = Arc::new(
            grok_default
                .compile("%{SYSLOGTIMESTAMP:timestamp} %{GREEDYDATA:message}", true)
                .unwrap(),
        );
        let chrono = Arc::new("%Y %b %d %H:%M:%S".to_string());
        let line_pattern = LinePattern {
            raw: raw.to_string(),
            grok,
            chrono: chrono.clone(),
            timezone: UTC,
            syslog_ts: true,
        };

        let line = "Feb 28 13:29:46 second message";
        let parsed_line = LogFileStreamer::apply_pattern(line, &line_pattern, 2019);
        assert_eq!(1551360586000, parsed_line.timestamp);
        assert_eq!("second message", parsed_line.message);
    }
}
