use crate::models::Ticket;

#[RequestRoute("/events")]
#[Public]
struct EventsController {
    #[Needs]
    tickets: std::sync::Arc<TicketService>,
}
impl EventsController {
    #[StreamRoute]
    fn stream_events(&self) -> impl Iterator<Item = Ticket> + '_ {
        self.tickets_for(self.tickets.levels())
    }

    fn tickets_for(&self, levels: Vec<i64>) -> impl Iterator<Item = Ticket> + '_ {
        levels.into_iter().map(|level| Ticket { sku: String::from("anvil"), level })
    }
}
