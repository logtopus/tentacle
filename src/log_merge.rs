use crate::data::{ApplicationError, LogStream, ParsedLine, StreamEntry};
use core::pin::Pin;
use core::task::Context;
use futures::stream::{Stream, StreamExt};
use futures::task::Poll;
use log::*;
use std::vec::Vec;

#[derive(PartialEq)]
enum SourceState {
    NeedsPoll,
    Delivered,
    Finished,
    Failed,
}

#[derive(Debug)]
struct BufferEntry {
    log_line: StreamEntry,
    source_idx: usize,
}

pub struct LogMerge {
    running_sources: usize,
    sources: Vec<LogStream>,
    source_state: Vec<SourceState>,
    buffer: Vec<BufferEntry>,
    current_timestamp: u128,
}

impl LogMerge {
    pub fn new(sources: Vec<LogStream>) -> LogMerge {
        let num_sources = sources.len();
        let mut source_state = Vec::with_capacity(sources.len());
        for _ in 0..sources.len() {
            source_state.push(SourceState::NeedsPoll);
        }
        LogMerge {
            running_sources: num_sources,
            sources: sources,
            source_state: source_state,
            buffer: Vec::with_capacity(num_sources),
            current_timestamp: 0,
        }
    }

    fn next_entry(&mut self) -> BufferEntry {
        // TODO: better error handling, remove_item -> rust nightly / 2019-02-20
        let e = self.buffer.remove(0);
        return e;
    }

    fn insert_into_buffer(&mut self, log_line: StreamEntry, source_idx: usize) {
        let line = BufferEntry {
            log_line,
            source_idx,
        };
        let buffer_size = self.buffer.len();
        let mut insert_at = 0;
        for idx in 0..buffer_size {
            if line.log_line.timestamp() < self.buffer[idx].log_line.timestamp() {
                break;
            }
            insert_at += 1;
        }
        self.buffer.insert(insert_at, line);
    }

    fn inject_error(&mut self, _err: ApplicationError, source_idx: usize) {
        let error = ParsedLine {
            timestamp: self.current_timestamp,
            loglevel: Some(format!("ERROR")),
            message: format!("A tentacle failed while retrieving the log."),
        };
        let log_line = StreamEntry::LogLine {
            line: String::from("Failed while retrieving the log."),
            parsed_line: error,
        };
        self.insert_into_buffer(log_line, source_idx);
    }

    // async fn poll_source(&mut self, source_idx: usize) -> () {
    //     match self.sources[source_idx].next().await {
    //         Some(Ok(line)) => {
    //             self.insert_into_buffer(line, source_idx);
    //             self.source_state[source_idx] = SourceState::Delivered;
    //         }
    //         Ok(Ready(None)) => {
    //             self.source_state[source_idx] = SourceState::Finished;
    //             self.running_sources -= 1;
    //         }
    //         Ok(NotReady) => {
    //             self.source_state[source_idx] = SourceState::NeedsPoll;
    //         }
    //         Err(e) => {
    //             error!("Source failed: {}", e);
    //             self.inject_error(e, source_idx);
    //             self.source_state[source_idx] = SourceState::Failed;
    //             self.running_sources -= 1;
    //         }
    //     }
    //     ()
    // }
}

impl Stream for LogMerge {
    type Item = StreamEntry;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        for s in 0..(*self).source_state.len() {
            match (*self).source_state[s] {
                SourceState::NeedsPoll => match (*self).sources[s].poll_next_unpin(cx) {
                    Poll::Ready(Some(Ok(line))) => {
                        (*self).insert_into_buffer(line, s);
                        (*self).source_state[s] = SourceState::Delivered;
                    }
                    Poll::Ready(None) => {
                        (*self).source_state[s] = SourceState::Finished;
                        (*self).running_sources -= 1;
                    }
                    Poll::Pending => {
                        (*self).source_state[s] = SourceState::NeedsPoll;
                    }
                    Poll::Ready(Some(Err(e))) => {
                        error!("Source failed: {}", e);
                        (*self).inject_error(e, s);
                        (*self).source_state[s] = SourceState::Failed;
                        (*self).running_sources -= 1;
                    }
                },
                _ => {}
            }
        }
        if (*self).running_sources == 0 && (*self).buffer.is_empty() {
            Poll::Ready(None)
        } else if (*self).running_sources <= (*self).buffer.len() {
            let entry = (*self).next_entry();
            if (*self).source_state[entry.source_idx] == SourceState::Delivered {
                (*self).source_state[entry.source_idx] = SourceState::NeedsPoll;
            }
            (*self).current_timestamp = entry.log_line.timestamp();
            Poll::Ready(Some(entry.log_line))
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::data::{LogStream, ParsedLine, StreamEntry};
    use crate::log_merge::LogMerge;
    use async_std::task;
    use futures::stream;
    use futures::stream::{empty, once, StreamExt};

    fn line_at(timestamp: u128, line: &str) -> StreamEntry {
        let error = ParsedLine {
            timestamp: timestamp,
            message: line.to_string(),
            loglevel: None,
        };
        StreamEntry::LogLine {
            line: String::from("Failed while retrieving the log."),
            parsed_line: error,
        }
    }

    #[test]
    fn test_new() {
        let s1: LogStream = once(async { Ok(line_at(0, "s1")) }).boxed_local();
        let s2: LogStream = once(async { Ok(line_at(0, "s2")) }).boxed_local();
        let sources = vec![s1, s2];
        let merge = LogMerge::new(sources);
        assert!(merge.sources.len() == 2);
        assert!(merge.source_state.len() == 2);
    }

    #[test]
    fn empty_streams() {
        let s1: LogStream = empty().boxed_local();
        let sources = vec![s1];
        let merge = LogMerge::new(sources);
        let col = merge.collect::<Vec<StreamEntry>>();
        let result = task::block_on(col);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_stream() {
        let l1 = line_at(0, "s1");
        let l2 = line_at(1, "s1");
        let s1: LogStream = stream::iter(vec![Ok(l1.clone()), Ok(l2.clone())]).boxed_local();
        let sources = vec![s1];
        let merge = LogMerge::new(sources);
        let result = task::block_on(merge.collect::<Vec<StreamEntry>>());
        assert_eq!(vec![l1.clone(), l2.clone()], result);
    }

    #[test]
    fn test_multiple_streams() {
        let l11 = line_at(100, "s11");
        let l12 = line_at(300, "s12");
        let l13 = line_at(520, "s13");
        let l21 = line_at(90, "s21");
        let l22 = line_at(430, "s22");
        let l31 = line_at(120, "s31");
        let l32 = line_at(120, "s32");
        let l33 = line_at(320, "s33");
        let l34 = line_at(520, "s34");
        let s1: LogStream =
            stream::iter(vec![Ok(l11.clone()), Ok(l12.clone()), Ok(l13.clone())]).boxed_local();
        let s2: LogStream = stream::iter(vec![Ok(l21.clone()), Ok(l22.clone())]).boxed_local();
        let s3: LogStream = stream::iter(vec![
            Ok(l31.clone()),
            Ok(l32.clone()),
            Ok(l33.clone()),
            Ok(l34.clone()),
        ])
        .boxed_local();
        let sources = vec![s1, s2, s3];
        let merge = LogMerge::new(sources);
        let result = task::block_on(merge.collect::<Vec<StreamEntry>>());
        assert_eq!(
            vec![
                l21.clone(),
                l11.clone(),
                l31.clone(),
                l32.clone(),
                l12.clone(),
                l33.clone(),
                l22.clone(),
                l13.clone(),
                l34.clone()
            ],
            result
        );
    }
}
