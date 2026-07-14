use crate::ConnectionStatus;
use std::sync::Mutex;

pub(crate) const MAX_FRAME: usize = 64 * 1024 * 1024;
const FRAME_HEADER: usize = 4;

pub(crate) fn encode_frame(message: &[u8]) -> Vec<u8> {
    let mut framed = Vec::with_capacity(message.len() + FRAME_HEADER);
    framed.extend_from_slice(&(message.len() as u32).to_le_bytes());
    framed.extend_from_slice(message);
    framed
}

#[derive(Default)]
pub(crate) struct FrameBuffer {
    buffer: Vec<u8>,
    cursor: usize,
}

impl FrameBuffer {
    pub(crate) fn push(&mut self, bytes: &[u8]) {
        if self.cursor == self.buffer.len() {
            self.buffer.clear();
            self.cursor = 0;
        } else if self.cursor >= self.buffer.len() / 2 {
            self.buffer.drain(..self.cursor);
            self.cursor = 0;
        }
        self.buffer.extend_from_slice(bytes);
    }

    pub(crate) fn next_frame(&mut self) -> Result<Option<Vec<u8>>, ()> {
        let available = &self.buffer[self.cursor..];
        if available.len() < FRAME_HEADER {
            return Ok(None);
        }
        let len = u32::from_le_bytes(available[..FRAME_HEADER].try_into().unwrap()) as usize;
        if len > MAX_FRAME {
            return Err(());
        }
        if available.len() < FRAME_HEADER + len {
            return Ok(None);
        }
        let frame = available[FRAME_HEADER..FRAME_HEADER + len].to_vec();
        self.cursor += FRAME_HEADER + len;
        Ok(Some(frame))
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
