#[Service]
struct AuditService {
  log: std::sync::Mutex<Vec<String>>,
  settled: std::sync::Mutex<i64>,
  audited: std::sync::Mutex<i64>,
}
impl AuditService {
  fn new() -> Self {
    Self {
      log: std::sync::Mutex::new(Vec::new()),
      settled: std::sync::Mutex::new(0),
      audited: std::sync::Mutex::new(0),
    }
  }

  #[OnEvent({"id":"audit-log"})]
  #[Order(-10)]
  fn record(&self, order: crate::models::PlacedOrder) {
    self.log.lock().unwrap().push(order.status);
  }

  #[OnEvent({"condition":"value.status == 'paid' && value.total >= 10"})]
  #[Order(10)]
  fn settle(&self, order: crate::models::PlacedOrder) -> crate::models::SettledOrder {
    crate::models::SettledOrder { total: order.total }
  }

  #[OnEvent]
  fn save_settled(&self, order: crate::models::SettledOrder) {
    *self.settled.lock().unwrap() = order.total;
  }

  #[OnEvent]
  #[InBackground]
  fn audit(&self, order: crate::models::SettledOrder) {
    *self.audited.lock().unwrap() = order.total;
  }
}
