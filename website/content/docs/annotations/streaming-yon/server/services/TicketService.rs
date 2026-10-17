#[Service]
struct TicketService {}
impl TicketService {
    fn new() -> Self { Self {} }

    fn levels(&self) -> Vec<i64> {
        vec![1, 2, 3]
    }
}
