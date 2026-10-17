pub struct ApplicationEventPublisher;
impl ApplicationEventPublisher {
    pub fn new() -> Self { Self }
    pub fn publish_event<T: Into<Json>>(&self, value: T, retain: bool) {
        let full_name = std::any::type_name::<T>();
        let name = full_name.rsplit("::").next().unwrap_or(full_name);
        let is_model = !full_name.contains('<') && name.starts_with(|first: char| first.is_ascii_uppercase()) && !matches!(name, "Json" | "String");
        if !is_model { eprintln!("Yon local event publication failed."); return; }
        __yon_event_publish(name, &value.into(), retain);
    }
}
#[derive(Default)]
struct __YonEventState { queue: std::collections::VecDeque<(String,String)>, retained: std::collections::BTreeMap<String,String>, closed: bool, topics: std::collections::BTreeSet<String>, outbound: Vec<(String,String)> }
thread_local! { static __YON_EVENT_STATE: std::cell::RefCell<Option<__YonEventState>> = const { std::cell::RefCell::new(None) }; }
fn __yon_event_publish(name: &str, value: &Json, retain: bool) {
    let encoded = value.stringify().ok().filter(|encoded| encoded.len() <= 65536);
    let delivered = encoded.is_some_and(|encoded| __YON_EVENT_STATE.with(|state| {
        let Ok(mut owner) = state.try_borrow_mut() else { return false; };
        let Some(owner) = owner.as_mut() else { return false; };
        if owner.closed || owner.queue.len() >= 128 { return false; }
        if retain { owner.retained.insert(name.to_owned(),encoded.clone()); }
        if owner.topics.contains(name) { owner.outbound.push((name.to_owned(),encoded.clone())); }
        owner.queue.push_back((name.to_owned(),encoded));
        true
    }));
    if !delivered { eprintln!("Yon local event publication failed."); }
}
fn __yon_event_fits(models: &Json, shape: &str, value: &Json) -> bool {
    if let Some(inner) = shape.strip_prefix('?') { return value.is_null() || __yon_event_fits(models, inner, value); }
    if let Some(item) = shape.strip_prefix("[]") { return matches!(value, Json::List(_)) && value.items().iter().all(|entry| __yon_event_fits(models, item, entry)); }
    match shape {
        "any" => return true,
        "string" => return matches!(value, Json::Text(_)),
        "integer" => return matches!(value, Json::Integer(_) | Json::Unsigned(_)),
        "number" => return matches!(value, Json::Number(_) | Json::Integer(_) | Json::Unsigned(_)),
        "boolean" => return matches!(value, Json::Bool(_)),
        _ => {}
    }
    let model = models.get(shape);
    let Json::Map(entries) = value else { return false; };
    if model.is_null() { return false; }
    let fields = model.get("fields").entries();
    let open = matches!(model.get("open"), Json::Bool(true));
    if !open && entries.iter().any(|(key, _)| !fields.iter().any(|(name, _)| name == key)) { return false; }
    for (name, declared) in fields {
        let field = declared.text();
        let required = field.strip_prefix('!');
        let present = entries.iter().find(|(key, _)| key == name);
        let Some((_, found)) = present else {
            if required.is_some() { return false; }
            continue;
        };
        if !__yon_event_fits(models, required.unwrap_or(&field), found) { return false; }
    }
    true
}
fn __yon_event_matches(matcher: &Json, value: &Json, event: &str) -> bool {
    fn operand(node: &Json, value: &Json, event: &str) -> Json {
        match node.get("k").text().as_str() {
            "lit" => node.get("v").clone(),
            "event" => Json::from(event),
            _ => {
                let mut current = value;
                for segment in node.get("path").items() {
                    current = match current {
                        Json::Map(_) => current.get(&segment.text()),
                        Json::List(items) => match segment {
                            Json::Integer(index) => usize::try_from(*index).ok().and_then(|index| items.get(index)).unwrap_or(&Json::Null),
                            Json::Unsigned(index) => usize::try_from(*index).ok().and_then(|index| items.get(index)).unwrap_or(&Json::Null),
                            _ => return Json::Null,
                        },
                        _ => return Json::Null,
                    };
                }
                current.clone()
            }
        }
    }
    fn number(value: &Json) -> Option<f64> {
        match value { Json::Number(value) => Some(*value), Json::Integer(value) => Some(*value as f64), Json::Unsigned(value) => Some(*value as f64), _ => None }
    }
    fn truthy(value: &Json) -> bool {
        match value { Json::Null | Json::Bool(false) => false, Json::Text(text) => !text.is_empty(), other => number(other).is_none_or(|value| value != 0.0) }
    }
    fn compare(op: &str, left: &Json, right: &Json) -> bool {
        let numbers = number(left).zip(number(right));
        if matches!(op, "eq" | "ne") {
            let equal = match (numbers, left, right) {
                (Some((left, right)), _, _) => left == right,
                (None, Json::Text(left), Json::Text(right)) => left == right,
                (None, Json::Bool(left), Json::Bool(right)) => left == right,
                (None, Json::Null, Json::Null) => true,
                _ => false,
            };
            return (op == "eq") == equal;
        }
        let order = match (numbers, left, right) {
            (Some((left, right)), _, _) => left.partial_cmp(&right),
            (None, Json::Text(left), Json::Text(right)) => Some(left.cmp(right)),
            _ => None,
        };
        let Some(order) = order else { return false; };
        match op { "lt" => order.is_lt(), "le" => order.is_le(), "gt" => order.is_gt(), _ => order.is_ge() }
    }
    fn test(node: &Json, value: &Json, event: &str) -> bool {
        match node.get("k").text().as_str() {
            "and" => test(node.get("l"), value, event) && test(node.get("r"), value, event),
            "or" => test(node.get("l"), value, event) || test(node.get("r"), value, event),
            "not" => !test(node.get("e"), value, event),
            "truthy" => truthy(&operand(node.get("e"), value, event)),
            "cmp" => compare(&node.get("op").text(), &operand(node.get("l"), value, event), &operand(node.get("r"), value, event)),
            _ => false,
        }
    }
    test(matcher, value, event)
}
struct __YonListener { name: String, order: i64, condition: Option<Json>, asynchronous: bool, models: Option<Json>, returns: Option<String>, sequence: usize, callback: std::rc::Rc<dyn Fn(Json) -> Json> }
struct __YonEvents {
    listeners: Vec<__YonListener>, destroyers: Vec<Box<dyn Fn()>>, deliveries: usize,
    only: Option<std::collections::BTreeSet<String>>, delivered: Option<(String,String)>,
}
impl __YonEvents {
    fn new(topics: &[&str]) -> Self {
        let topics = topics.iter().map(|topic| (*topic).to_owned()).collect();
        __YON_EVENT_STATE.with(|state| { assert!(state.borrow().is_none(), "Nested local event invocation"); *state.borrow_mut() = Some(__YonEventState { topics, ..__YonEventState::default() }); });
        Self { listeners: Vec::new(), destroyers: Vec::new(), deliveries: 0, only: None, delivered: None }
    }
    fn delivery(body: &str, topics: &[&str]) -> Self {
        let mut events = Self::new(topics);
        let delivery = Json::parse(body).unwrap_or(Json::Null);
        events.only = Some(delivery.get("listeners").items().iter().map(Json::text).collect());
        events.delivered = delivery.get("payload").stringify().ok().map(|payload| (delivery.get("topic").text(), payload));
        events
    }
    fn receive_delivery(&mut self) {
        let Some(delivered) = self.delivered.take() else { return; };
        __YON_EVENT_STATE.with(|state| { if let Some(owner) = state.borrow_mut().as_mut() { owner.queue.push_back(delivered); } });
    }
    fn send_topics() {
        let outbound = __YON_EVENT_STATE.with(|state| state.borrow_mut().as_mut().map(|owner| std::mem::take(&mut owner.outbound)).unwrap_or_default());
        for (name, encoded) in outbound {
            let command = Json::map([("operation", Json::from("topic")), ("topic", Json::from(name.as_str())), ("payload_json", Json::from(encoded.as_str()))]);
            if yon_resource(command).is_err() { eprintln!("Yon topic publication failed."); }
        }
    }
    fn listen(&mut self, options: &str, listener: impl Fn(Json) -> Json + 'static, replay: bool, key: &str) {
        if self.only.as_ref().is_some_and(|only| !only.contains(key)) { return; }
        let options = Json::parse(options).expect("Compiled listener options");
        let callback: std::rc::Rc<dyn Fn(Json) -> Json> = std::rc::Rc::new(listener);
        let condition = Some(options.get("condition")).filter(|matcher| !matcher.is_null()).cloned();
        let models = Some(options.get("models")).filter(|models| !models.is_null()).cloned();
        let returns = Some(options.get("returns")).filter(|returns| !returns.is_null()).map(Json::text);
        let asynchronous = matches!(options.get("async"), Json::Bool(true));
        for name in options.get("events").items() {
            assert!(self.listeners.len() < 1024, "Local listener capacity exceeded");
            let name = name.text();
            if replay { __YON_EVENT_STATE.with(|state| { if let Some(owner) = state.borrow_mut().as_mut() { if let Some(encoded) = owner.retained.get(&name).cloned() { owner.queue.push_back((name.clone(),encoded)); } } }); }
            self.listeners.push(__YonListener { name, order: options.get("order").number() as i64, condition: condition.clone(), asynchronous, models: models.clone(), returns: returns.clone(), sequence: self.listeners.len(), callback: callback.clone() });
        }
        self.listeners.sort_by_key(|listener| (listener.order, listener.sequence));
    }
    fn republish(result: &Json, returns: Option<&str>) {
        let Some(name) = returns else { return; };
        let items = match result { Json::List(items) => &items[..items.len().min(32)], single => std::slice::from_ref(single) };
        for item in items {
            if !item.is_null() { __yon_event_publish(name, item, false); }
        }
    }
    fn drain(&mut self) {
        loop {
            let next = __YON_EVENT_STATE.with(|state| state.borrow_mut().as_mut().and_then(|owner| owner.queue.pop_front()));
            let Some((name, encoded)) = next else { Self::send_topics(); return; };
            self.deliveries += 1;
            if self.deliveries > 4096 { __YON_EVENT_STATE.with(|state| { if let Some(owner) = state.borrow_mut().as_mut() { owner.queue.clear(); } }); eprintln!("Yon local event delivery bound exceeded."); return; }
            let mut deferred = Vec::new();
            for listener in &self.listeners { if listener.name == name {
                let value = Json::parse(&encoded).expect("Validated local event JSON");
                if listener.models.as_ref().is_some_and(|models| !__yon_event_fits(models, &name, &value)) { eprintln!("Yon local event payload does not fit the listener model."); continue; }
                if listener.condition.as_ref().is_some_and(|matcher| !__yon_event_matches(matcher, &value, &name)) { continue; }
                if listener.asynchronous { deferred.push((listener, value)); continue; }
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (listener.callback)(value))) {
                    Ok(result) => Self::republish(&result, listener.returns.as_deref()),
                    Err(_) => eprintln!("Yon local event listener failed."),
                }
            }}
            for (listener, value) in deferred {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (listener.callback)(value))).is_err() { eprintln!("Yon local event listener failed."); }
            }
        }
    }
}
impl Drop for __YonEvents {
    fn drop(&mut self) {
        __YON_EVENT_STATE.with(|state| { if let Some(owner) = state.borrow_mut().as_mut() { owner.closed = true; owner.queue.clear(); } });
        self.listeners.clear();
        for destroy in self.destroyers.iter().rev() { if std::panic::catch_unwind(std::panic::AssertUnwindSafe(destroy)).is_err() { eprintln!("Yon local lifecycle cleanup failed."); } }
        __YON_EVENT_STATE.with(|state| *state.borrow_mut() = None);
    }
}
