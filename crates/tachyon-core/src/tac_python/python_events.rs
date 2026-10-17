use crate::Failure;
use crate::events::{EventListener, EventModule, EventType};
use crate::route_types::ModelTable;
use serde_json::{Map, Value, json};
use std::path::Path;

pub(super) struct PythonEvents {
    owner: Option<EventType>,
    module: Option<EventModule>,
}

impl PythonEvents {
    const PUBLISHER: &'static str = "applicationEventPublisher";
    const PENDING: &'static str = "Python Tac streams, browser API adapters and async OnStop or OnEvent methods remain pending; OnStart may be async, and synchronous OnStop and OnEvent methods and ApplicationEventPublisher are supported.";

    pub(super) fn read(
        path: &Path,
        module: Option<EventModule>,
        class: &str,
    ) -> Result<Self, Failure> {
        Self::check_supported(path, module.as_ref())?;
        let owner = module
            .iter()
            .flat_map(|module| &module.types)
            .find(|owner| owner.name == class)
            .cloned();
        Ok(Self { owner, module })
    }

    fn check_supported(path: &Path, module: Option<&EventModule>) -> Result<(), Failure> {
        let pending = module.is_some_and(Self::pending_module);
        (!pending).then_some(()).ok_or_else(|| {
            super::python_provider_source::PythonProviderSource::failure(path, Self::PENDING)
        })
    }

    pub(super) fn source(&self) -> Option<&str> {
        self.module.as_ref().map(|module| module.source.as_str())
    }

    pub(super) fn starts(&self) -> Vec<String> {
        self.owner
            .iter()
            .filter_map(|owner| owner.post_construct.clone())
            .collect()
    }

    pub(super) fn stops(&self) -> Vec<String> {
        self.owner
            .iter()
            .filter_map(|owner| owner.pre_destroy.clone())
            .collect()
    }

    pub(super) fn handles(&self, method: &str) -> bool {
        let lifecycle = self.starts().into_iter().chain(self.stops());
        lifecycle
            .chain(self.listener_names())
            .any(|name| name == method)
    }

    fn listener_names(&self) -> Vec<String> {
        self.owner
            .iter()
            .flat_map(|owner| &owner.listeners)
            .map(|listener| listener.method.clone())
            .collect()
    }

    pub(super) fn listeners(
        &self,
        methods: &[Value],
        models: &ModelTable,
    ) -> Result<Map<String, Value>, String> {
        self.owner.clone().map_or_else(
            || Ok(Map::new()),
            |owner| Self::described(owner, methods, models),
        )
    }

    fn described(
        mut owner: EventType,
        methods: &[Value],
        models: &ModelTable,
    ) -> Result<Map<String, Value>, String> {
        let mut module = EventModule {
            source: String::new(),
            types: Vec::new(),
            methods: Vec::new(),
            topics: Vec::new(),
        };
        owner.streams.clear();
        module.types.push(owner);
        module.describe_models(models)?;
        Ok(module.types[0]
            .listeners
            .iter()
            .map(|listener| Self::listener(listener, methods))
            .collect())
    }

    fn listener(listener: &EventListener, methods: &[Value]) -> (String, Value) {
        let argument = methods
            .iter()
            .find(|method| method["name"] == listener.method.as_str())
            .and_then(|method| method["arguments"].get(0))
            .cloned()
            .unwrap_or(Value::Null);
        (
            listener.method.clone(),
            json!({
                "key": listener.method,
                "events": listener.events,
                "replay": listener.replay,
                "order": listener.order,
                "asynchronous": listener.asynchronous,
                "condition": listener.condition,
                "models": listener.models,
                "returns": listener.returns,
                "argument": argument,
            }),
        )
    }

    fn pending_module(module: &EventModule) -> bool {
        module.types.iter().any(Self::pending_owner)
            || module
                .methods
                .iter()
                .any(|method| method.asynchronous && !Self::starts_owner(module, &method.name))
    }

    fn starts_owner(module: &EventModule, method: &str) -> bool {
        module
            .types
            .iter()
            .any(|owner| owner.post_construct.as_deref() == Some(method))
    }

    fn pending_owner(owner: &EventType) -> bool {
        let browser_resource = owner
            .resources
            .iter()
            .any(|resource| resource.name != Self::PUBLISHER);
        browser_resource || !owner.streams.is_empty()
    }
}
