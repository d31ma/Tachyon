<?php
use Models\Lifecycle;

#[RequestRoute("/lifecycle")]
#[Public]
class LifecycleController {
  #[Needs]
  public AuditService $audit;
  #[Needs]
  public CheckoutService $checkout;
  #[GetRoute]
  function getLifecycle(): Lifecycle {
    return new Lifecycle($this->audit->log, $this->audit->settled, $this->audit->audited);
  }
}
