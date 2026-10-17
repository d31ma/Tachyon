use crate::models::Lifecycle;

#[RequestRoute("/lifecycle")]
#[Public]
struct LifecycleController {
  #[Needs]
  audit: std::sync::Arc<AuditService>,
  #[Needs]
  checkout: std::sync::Arc<CheckoutService>,
}
impl LifecycleController {
  #[GetRoute]
  fn get_lifecycle(&self) -> Lifecycle {
    let log = self.audit.log.lock().unwrap().clone();
    let settled = *self.audit.settled.lock().unwrap();
    let audited = *self.audit.audited.lock().unwrap();
    Lifecycle { log, settled, audited }
  }
}
