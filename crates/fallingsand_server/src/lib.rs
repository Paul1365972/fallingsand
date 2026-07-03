pub struct Server;

impl Server {
    pub fn new() -> Self {
        Self
    }

    pub fn tick(&mut self) {}
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}
