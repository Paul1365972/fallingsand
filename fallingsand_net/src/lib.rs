pub trait NetworkInterface {
    fn new() -> Self;
    fn connect(&mut self, url: &str);
    fn disconnect(&mut self);
    fn send(&mut self, message: &[u8]);
    fn recv(&mut self) -> Vec<u8>;
    fn get_state(&mut self) -> NetworkState;
}

#[derive(Debug)]
pub enum NetworkState {
    Connecting,
    Connected,
    Disconnected,
}
