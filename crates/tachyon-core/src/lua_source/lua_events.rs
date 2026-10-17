use super::{LuaModels, LuaSource};
use crate::events::{EventListener, EventModule, EventType};
use crate::route_types::TypeText;
use crate::yon::plan::Plan;
use crate::yon::route_calls::RouteCalls;
use std::fmt::Write as _;

pub(super) struct LuaEvents<'p> {
    plan: &'p Plan,
    events: &'p EventModule,
}

impl<'p> LuaEvents<'p> {
    pub(super) const RUNTIME: &'static str = include_str!("lua_events.lua");
    const TYPE_SYNTAX: &'static str = "rb";
    const SCOPE: &'static str = "function YonGenerated.within(only, work, ...)\n  local bus = YonEvents.new(only)\n  local cache = {}\n  bus.cache = cache\n  local ok, result = pcall(work, bus, cache, ...)\n  bus:close()\n  if not ok then\n    error(result, 0)\n  end\n  return result\nend\n\n";
    const DELIVERY: &'static str = "function YonGenerated.deliver(bus, cache, delivery)\n  YonGenerated.listen(bus, cache)\n  bus:receive(delivery.topic, YonJson.encode(delivery.payload))\n  bus:drain()\nend\n\nfunction Handler.yon_deliver(delivery)\n  YonGenerated.within(delivery.listeners, YonGenerated.deliver, delivery)\nend\n\n";

    pub(super) fn new(plan: &'p Plan, events: &'p EventModule) -> Self {
        Self { plan, events }
    }

    pub(super) fn functions(&self, owners: &str, controller: usize) -> Result<String, String> {
        crate::events::yon::EventLowering::check(self.plan, self.events, "lua")?;
        Ok(format!(
            "{}{}{}{}{}",
            self.topics_line(),
            self.listen_function()?,
            Self::SCOPE,
            self.handler_functions(owners, controller),
            self.delivery_function()
        ))
    }

    fn topics_line(&self) -> String {
        let names: Vec<String> = self
            .events
            .topics
            .iter()
            .map(|name| format!("[{name:?}] = true"))
            .collect();
        format!("YonEvents.topics = {{ {} }}\n\n", names.join(", "))
    }

    fn delivery_function(&self) -> &'static str {
        if self.events.topic_listeners().is_empty() {
            return "";
        }
        Self::DELIVERY
    }

    fn listen_function(&self) -> Result<String, String> {
        Ok(format!(
            "function YonGenerated.listen(bus, cache)\n{}{}  bus:start(cache, {{ {} }})\n  bus:stop_later(cache, {{ {} }})\nend\n\n",
            self.eager_lines()?,
            self.listener_lines()?,
            self.hooks(|event_type| event_type.post_construct.as_deref())?,
            self.hooks(|event_type| event_type.pre_destroy.as_deref())?,
        ))
    }

    fn eager_lines(&self) -> Result<String, String> {
        Ok(self
            .eager()?
            .iter()
            .map(|index| Self::resolve_line(*index))
            .collect())
    }

    fn resolve_line(index: usize) -> String {
        format!("  YonGenerated.resolve_{index}(cache)\n")
    }

    fn eager(&self) -> Result<Vec<usize>, String> {
        let mut indexes = vec![self.index(&self.plan.controller)?];
        indexes.extend(
            RouteCalls::error_owners(self.plan)
                .into_iter()
                .map(|(index, _)| index),
        );
        indexes.extend(self.owner_indexes()?);
        Ok(indexes.into_iter().fold(Vec::new(), Self::kept_once))
    }

    fn owner_indexes(&self) -> Result<Vec<usize>, String> {
        self.events
            .types
            .iter()
            .map(|event_type| self.index(&event_type.name))
            .collect()
    }

    fn kept_once(mut kept: Vec<usize>, index: usize) -> Vec<usize> {
        if !kept.contains(&index) {
            kept.push(index);
        }
        kept
    }

    fn index(&self, name: &str) -> Result<usize, String> {
        self.plan
            .providers
            .iter()
            .position(|provider| provider.name == name)
            .ok_or_else(|| format!("Event owner {name} needs a managed Yon role"))
    }

    fn listener_lines(&self) -> Result<String, String> {
        let mut lines = String::new();
        for event_type in &self.events.types {
            lines.push_str(&self.type_listener_lines(event_type)?);
        }
        Ok(lines)
    }

    fn type_listener_lines(&self, event_type: &EventType) -> Result<String, String> {
        let owner = self.index(&event_type.name)?;
        Ok(event_type
            .listeners
            .iter()
            .map(|listener| self.listener_line(owner, event_type, listener))
            .collect())
    }

    fn listener_line(
        &self,
        owner: usize,
        event_type: &EventType,
        listener: &EventListener,
    ) -> String {
        format!(
            "  bus:listen({}, function(value) return cache[{owner}]:{}({}) end, {}, {})\n",
            LuaSource::long_string(&crate::events::yon::EventLowering::listener_options(
                listener
            )),
            listener.method,
            self.payload(event_type, listener),
            listener.replay,
            LuaSource::long_string(&format!("{}.{}", event_type.name, listener.method))
        )
    }

    fn payload(&self, event_type: &EventType, listener: &EventListener) -> String {
        self.events
            .methods
            .iter()
            .find(|method| {
                [
                    method.owner == event_type.name,
                    method.name == listener.method,
                ] == [true, true]
            })
            .and_then(|method| method.parameters.first())
            .and_then(|parameter| TypeText::shape(Self::TYPE_SYNTAX, &parameter.type_source).ok())
            .and_then(|shape| LuaModels::descriptor(&shape))
            .map_or_else(
                || String::from("value"),
                |descriptor| format!("YonModels.read({descriptor}, value)"),
            )
    }

    fn hooks(&self, method: impl Fn(&EventType) -> Option<&str>) -> Result<String, String> {
        let mut entries = Vec::new();
        for event_type in &self.events.types {
            let index = self.index(&event_type.name)?;
            entries.extend(method(event_type).map(|name| format!("[{index}] = {name:?}")));
        }
        Ok(entries.join(", "))
    }

    fn handler_functions(&self, owners: &str, controller: usize) -> String {
        RouteCalls::verbs(self.plan)
            .iter()
            .fold(String::new(), |mut functions, verb| {
                let _ = write!(
                    functions,
                    "function YonGenerated.handle_{verb}(bus, cache, request)\n  YonGenerated.listen(bus, cache)\n  bus:drain()\n  local result = YonGenerated.route_{verb}(YonGenerated.resolve_{controller}(cache), request{owners})\n  bus:drain()\n  return result\nend\n\nfunction Handler.{verb}(request)\n  return YonGenerated.within(nil, YonGenerated.handle_{verb}, request)\nend\n\n"
                );
                functions
            })
    }
}
