//! Streaming input: a producer thread reads blocks of whole lines while the
//! caller's thread parses the previous one, so reading overlaps parsing and
//! memory holds about two blocks of text rather than the whole file.
use super::block::Builder;
use crate::obj::{Obj, ObjError, ReadOptions, error::limit};
use std::{
    io::Read,
    sync::mpsc::{Receiver, SyncSender, channel, sync_channel},
};

pub fn read<R: Read + Send>(reader: R, options: ReadOptions) -> Result<Obj, ObjError> {
    let block = options.block_bytes.max(1);
    let line_bytes = options.line_bytes();
    let mut builder = Builder::new(options);
    std::thread::scope(|scope| {
        let (full_sender, full) = sync_channel(1);
        let (empty, empty_receiver) = channel();
        scope.spawn(move || produce(reader, block, line_bytes, full_sender, empty_receiver));
        // Returning early drops `full`, which stops the producer.
        while let Ok(message) = full.recv() {
            let buffer: Vec<u8> = message?;
            builder.block(&buffer)?;
            let _ = empty.send(buffer);
        }
        Ok(builder.finish())
    })
}

/// Send blocks of whole lines (the last may lack a final line feed) until
/// the input ends, an error occurs or the receiver hangs up; a line longer
/// than `line_bytes` is an error.
fn produce<R: Read>(
    mut reader: R,
    block: usize,
    line_bytes: usize,
    full: SyncSender<Result<Vec<u8>, ObjError>>,
    empty: Receiver<Vec<u8>>,
) {
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let mut buffer = empty.try_recv().unwrap_or_default();
        buffer.clear();
        buffer.reserve(block + carry.len());
        buffer.append(&mut carry);
        let mut target = block;
        let mut ended = false;
        // Fill to `target`; a line longer than that raises it.
        loop {
            while buffer.len() < target {
                let wanted = (target - buffer.len()) as u64;
                match (&mut reader).take(wanted).read_to_end(&mut buffer) {
                    Ok(0) => {
                        ended = true;
                        break;
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => {
                        let _ = full.send(Err(error.into()));
                        return;
                    }
                }
            }
            if ended {
                break;
            }
            match kinogaki_bytes::rfind(b'\n', &buffer) {
                Some(at) => {
                    carry.extend_from_slice(&buffer[at + 1..]);
                    buffer.truncate(at + 1);
                    break;
                }
                None if buffer.len() > line_bytes => {
                    let _ = full.send(Err(limit("line exceeds the length limit").into()));
                    return;
                }
                None => target += block,
            }
        }
        if !buffer.is_empty() && full.send(Ok(buffer)).is_err() {
            return;
        }
        if ended {
            return;
        }
    }
}
