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

struct Reason {
    text: String,
    authoritative: bool,
}

#[derive(Default)]
pub(crate) struct Closed(Mutex<Option<Reason>>);

impl Closed {
    pub(crate) fn mark(&self, text: &str) {
        self.set(text, true);
    }

    pub(crate) fn hint(&self, text: &str) {
        self.set(text, false);
    }

    fn set(&self, text: &str, authoritative: bool) {
        let mut slot = self.0.lock().unwrap();
        if slot
            .as_ref()
            .is_none_or(|reason| authoritative && !reason.authoritative)
        {
            *slot = Some(Reason {
                text: text.to_string(),
                authoritative,
            });
        }
    }

    pub(crate) fn status(&self) -> ConnectionStatus {
        match self.0.lock().unwrap().as_ref() {
            Some(reason) => ConnectionStatus::Closed {
                reason: reason.text.clone(),
            },
            None => ConnectionStatus::Connected,
        }
    }
}
