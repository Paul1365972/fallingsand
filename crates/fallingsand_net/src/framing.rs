use crate::ConnectionStatus;
use bytes::{Buf, Bytes, BytesMut};
use std::sync::Mutex;

pub(crate) const MAX_FRAME: usize = 64 * 1024 * 1024;
const FRAME_HEADER: usize = 4;

pub(crate) fn encode_frame(message: &[u8]) -> Bytes {
    let mut framed = BytesMut::with_capacity(message.len() + FRAME_HEADER);
    framed.extend_from_slice(&(message.len() as u32).to_le_bytes());
    framed.extend_from_slice(message);
    framed.freeze()
}

#[derive(Default)]
pub(crate) struct FrameBuffer {
    buffer: BytesMut,
}

impl FrameBuffer {
    pub(crate) fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    pub(crate) fn next_frame(&mut self) -> Result<Option<Bytes>, ()> {
        if self.buffer.len() < FRAME_HEADER {
            return Ok(None);
        }
        let len = u32::from_le_bytes(self.buffer[..FRAME_HEADER].try_into().unwrap()) as usize;
        if len > MAX_FRAME {
            return Err(());
        }
        if self.buffer.len() < FRAME_HEADER + len {
            return Ok(None);
        }
        self.buffer.advance(FRAME_HEADER);
        Ok(Some(self.buffer.split_to(len).freeze()))
    }
}

#[derive(Default)]
pub(crate) struct Closed(Mutex<Option<String>>);

impl Closed {
    pub(crate) fn mark(&self, reason: &str) {
        let mut closed = self.0.lock().unwrap();
        if closed.is_none() {
            *closed = Some(reason.to_string());
        }
    }

    pub(crate) fn status(&self) -> ConnectionStatus {
        match self.0.lock().unwrap().clone() {
            Some(reason) => ConnectionStatus::Closed { reason },
            None => ConnectionStatus::Connected,
        }
    }
}
