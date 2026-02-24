use rustc_hash::FxHashMap;

pub type ClientMap = FxHashMap<ClientId, Client>;

pub type ClientId = u32;

pub struct Client {
    pub send_buffer: Vec<u8>,
    pub receive_buffer: Vec<u8>,
}
