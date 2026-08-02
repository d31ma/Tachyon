struct PortableBridge {
    count: i64,
    label: String,
}

impl PortableBridge {
    fn new() -> Self {
        Self {
            count: 6,
            label: "from Rust",
        }
    }

    fn doubled(&self) -> i64 {
        self.count * 2
    }
}
