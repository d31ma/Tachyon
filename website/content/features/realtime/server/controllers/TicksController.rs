use crate::models::Tick;

#[RequestRoute("/ticks")]
#[Public]
struct TicksController {}
impl TicksController {
    fn new() -> Self { Self {} }

    #[StreamRoute]
    fn stream_ticks(&self) -> impl Iterator<Item = Tick> + '_ {
        self.countdown(3)
    }

    fn countdown(&self, start: i64) -> impl Iterator<Item = Tick> + '_ {
        (1..=start).rev().map(|level| Tick { sku: String::from("ANVIL-1"), level })
    }
}
