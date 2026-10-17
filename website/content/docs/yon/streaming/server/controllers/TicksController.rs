use crate::models::Tick;

#[RequestRoute("/ticks")]
#[Public]
struct TicksController {}
impl TicksController {
    fn new() -> Self { Self {} }

    #[StreamRoute]
    fn stream_ticks(&self) -> impl Iterator<Item = Tick> + '_ {
        self.ticks_up_to(3)
    }

    fn ticks_up_to(&self, last: i64) -> impl Iterator<Item = Tick> + '_ {
        (1..=last).map(|level| Tick { sku: String::from("ANVIL-1"), level })
    }
}
