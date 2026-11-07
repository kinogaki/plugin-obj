//! Streaming input: blocks of whole lines, each parsed while the next is
//! read, the two side by side on the SDK's workers (one after the other
//! where a host gives none), so reading overlaps parsing and memory holds
//! about two blocks of text rather than the whole file.
use super::block::Builder;
use crate::obj::{Obj, ObjError, ReadOptions, error::limit};
use kinogaki_studio_api::geometry::parallel;
use std::io::Read;
use std::sync::{Mutex, MutexGuard};

pub fn read<R: Read + Send>(reader: R, options: ReadOptions) -> Result<Obj, ObjError> {
    let mut lines = Lines {
        reader,
        block: options.block_bytes.max(1),
        line_bytes: options.line_bytes(),
        carry: Vec::new(),
        spare: Vec::new(),
        ended: false,
    };
    let mut current = lines.next()?;
    let (builder, lines) = (Mutex::new(Builder::new(options)), Mutex::new(lines));
    while let Some(text) = current.take() {
        let mut steps = parallel::map(2, |job| match job {
            0 => Step::Parsed(lock(&builder).block(&text)),
            _ => Step::Read(lock(&lines).next()),
        });
        let (Some(Step::Read(next)), Some(Step::Parsed(parsed))) = (steps.pop(), steps.pop())
        else {
            unreachable!("job 0 parses, job 1 reads");
        };
        parsed?;
        current = next?;
        lock(&lines).spare = text;
    }
    let builder = builder.into_inner().unwrap_or_else(|e| e.into_inner());
    Ok(builder.finish())
}

/// What one of a step's two jobs did.
enum Step {
    /// Parsed the block read before.
    Parsed(Result<(), ObjError>),
    /// Read the next block; none past the end.
    Read(Result<Option<Vec<u8>>, ObjError>),
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// A reader's text in blocks of whole lines.
struct Lines<R> {
    reader: R,
    /// Bytes read a block, more where one line is longer.
    block: usize,
    /// The longest line read.
    line_bytes: usize,
    /// What followed the last block's last line feed.
    carry: Vec<u8>,
    /// A parsed block's buffer, to read the next into.
    spare: Vec<u8>,
    /// Whether the reader has ended.
    ended: bool,
}

impl<R: Read> Lines<R> {
    /// The next block of whole lines (the last may lack a final line feed);
    /// none once the input has ended. A line longer than `line_bytes` is an
    /// error.
    fn next(&mut self) -> Result<Option<Vec<u8>>, ObjError> {
        if self.ended {
            return Ok(None);
        }
        let mut buffer = std::mem::take(&mut self.spare);
        buffer.clear();
        buffer.reserve(self.block + self.carry.len());
        buffer.append(&mut self.carry);
        let mut target = self.block;
        // Fill to `target`; a line longer than that raises it.
        loop {
            while buffer.len() < target {
                let wanted = (target - buffer.len()) as u64;
                match (&mut self.reader).take(wanted).read_to_end(&mut buffer) {
                    Ok(0) => {
                        self.ended = true;
                        break;
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => return Err(error.into()),
                }
            }
            if self.ended {
                break;
            }
            match kinogaki_bytes::rfind(b'\n', &buffer) {
                Some(at) => {
                    self.carry.extend_from_slice(&buffer[at + 1..]);
                    buffer.truncate(at + 1);
                    break;
                }
                None if buffer.len() > self.line_bytes => {
                    return Err(limit("line exceeds the length limit").into());
                }
                None => target += self.block,
            }
        }
        Ok((!buffer.is_empty()).then_some(buffer))
    }
}
