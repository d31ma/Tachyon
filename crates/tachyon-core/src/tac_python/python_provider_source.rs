use super::python_browser_apis::PythonBrowserApis;
use super::python_events::PythonEvents;
use crate::Failure;
use crate::behavior::{PythonMembers, ServiceRoles};
use crate::route_types::{ModelTable, TypeText};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(super) struct PythonProviderSource {
    pub(super) path: PathBuf,
    pub(super) class: String,
    pub(super) source: String,
    members: Value,
    roles: ServiceRoles,
    helper: bool,
    events: PythonEvents,
}

impl PythonProviderSource {
    const PUBLISHER_TYPE: &'static str = "ApplicationEventPublisher";
    const CONTRACT_FOLDERS: [&'static str; 2] = ["client/stores", "client/remotes"];

    pub(super) fn read(path: &Path, source: &str) -> Result<Self, Failure> {
        let class = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        PythonBrowserApis::refuse(path, source)?;
        let roles = ServiceRoles::read(path, source)?;
        let plain = ServiceRoles::without_roles(path, source)?;
        let members = Self::members(path, &plain, &class)?;
        let module = crate::events::extract(path, &plain, "tac")?;
        let events = PythonEvents::read(path, module, &class)?;
        let source = events.source().map_or(plain, str::to_owned);
        Ok(Self {
            path: path.to_path_buf(),
            class,
            source,
            members,
            roles,
            helper: path.starts_with("client/helpers"),
            events,
        })
    }

    fn members(path: &Path, plain: &str, class: &str) -> Result<Value, Failure> {
        if Self::contract(path) {
            return Ok(json!({ "fields": [], "methods": [], "constructor": [], "qualifiers": {} }));
        }
        PythonMembers::read(path, plain, class)
    }

    fn contract(path: &Path) -> bool {
        Self::CONTRACT_FOLDERS
            .iter()
            .any(|folder| path.starts_with(folder))
    }

    pub(super) fn manual(&self, names: &BTreeSet<String>) -> bool {
        self.is_manual(names)
    }

    fn is_manual(&self, names: &BTreeSet<String>) -> bool {
        let constructed = self.members["constructor"]
            .as_array()
            .is_some_and(|names| !names.is_empty());
        self.helper && names.contains(&self.class) && constructed
    }

    pub(super) fn describe(
        &self,
        providers: &BTreeSet<String>,
        models: &ModelTable,
    ) -> Result<Value, Failure> {
        self.require_constructor()?;
        let constructor = self.constructor(providers, models)?;
        let fields = self.fields(&constructor, models)?;
        let methods = self.methods(models)?;
        let listeners = self
            .events
            .listeners(self.declared_methods(), models)
            .map_err(|message| Self::failure(&self.path, &message))?;
        let writable = fields
            .iter()
            .filter(|(_, field)| field["immutable"] != true)
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        Ok(
            json!({ "prototype": self.roles.prototype, "constructor": constructor,
                   "fields": fields, "writable": writable, "methods": methods,
                   "starts": self.events.starts(), "stops": self.events.stops(),
                   "listeners": listeners, "publishers": self.publishers() }),
        )
    }

    fn require_constructor(&self) -> Result<(), Failure> {
        let constructor = self.members["constructor"].as_array();
        let has_arguments = constructor.is_some_and(|arguments| !arguments.is_empty());
        if has_arguments && self.members["inferred"] != true {
            return Err(Self::failure(
                &self.path,
                "Managed Python Tac constructors use typed Needs or NeedsAll fields; caller-owned Helpers need a real authored construction.",
            ));
        }
        Ok(())
    }

    fn constructor(
        &self,
        providers: &BTreeSet<String>,
        models: &ModelTable,
    ) -> Result<Vec<Value>, Failure> {
        self.members["constructor"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|name| !self.publishers().iter().any(|publisher| publisher == name))
            .map(|name| self.binding(name, providers, models))
            .collect()
    }

    fn declared_methods(&self) -> &[Value] {
        self.members["methods"]
            .as_array()
            .map_or(&[], Vec::as_slice)
    }

    fn publishers(&self) -> Vec<String> {
        self.members["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|field| field["type"] == Self::PUBLISHER_TYPE)
            .filter_map(|field| field["name"].as_str())
            .map(str::to_owned)
            .collect()
    }

    fn binding(
        &self,
        name: &str,
        providers: &BTreeSet<String>,
        models: &ModelTable,
    ) -> Result<Value, Failure> {
        let field = self.members["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|field| field["name"].as_str() == Some(name))
            .ok_or_else(|| {
                Self::failure(
                    &self.path,
                    "Python Tac constructor fields require explicit instance type declarations.",
                )
            })?;
        let written = field["type"].as_str().unwrap_or_default();
        self.require_qualifier(name, providers)?;
        self.bind_provider(field, providers)
            .map_or_else(|| self.bind_property(field, written, models), Ok)
    }

    fn bind_provider(&self, field: &Value, providers: &BTreeSet<String>) -> Option<Value> {
        let name = field["name"].as_str()?;
        let written = self.members["qualifiers"][name]
            .as_str()
            .or_else(|| field["type"].as_str())?;
        providers
            .contains(written)
            .then(|| json!({ "provider": written, "name": field["name"] }))
    }

    fn require_qualifier(&self, name: &str, providers: &BTreeSet<String>) -> Result<(), Failure> {
        let unknown = self.members["qualifiers"][name]
            .as_str()
            .is_some_and(|name| !providers.contains(name));
        if unknown {
            return Err(Self::failure(
                &self.path,
                "Python Tac Named must select a declared managed provider.",
            ));
        }
        Ok(())
    }

    fn bind_property(
        &self,
        field: &Value,
        written: &str,
        models: &ModelTable,
    ) -> Result<Value, Failure> {
        self.require_property_owner()?;
        let shape = TypeText::shape("py", written)
            .map_err(|message| Self::failure(&self.path, &message))?;
        self.require_model(&shape, models)?;
        Ok(json!({ "name": field["name"], "page": field["page"], "shape": shape }))
    }

    fn require_property_owner(&self) -> Result<(), Failure> {
        if self.helper || self.roles.required.is_empty() {
            return Err(Self::failure(
                &self.path,
                "Python Tac Needs must name a declared same-language provider; only a Service NeedsAll field can be a page property.",
            ));
        }
        Ok(())
    }

    fn fields(
        &self,
        constructor: &[Value],
        models: &ModelTable,
    ) -> Result<Map<String, Value>, Failure> {
        let injected = constructor
            .iter()
            .filter(|binding| binding.get("provider").is_some())
            .filter_map(|binding| binding["name"].as_str())
            .collect::<BTreeSet<_>>();
        self.members["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|field| !injected.contains(field["name"].as_str().unwrap_or_default()))
            .filter(|field| field["type"] != Self::PUBLISHER_TYPE)
            .map(|field| self.field(field, models))
            .collect()
    }

    fn field(&self, field: &Value, models: &ModelTable) -> Result<(String, Value), Failure> {
        let written = field["type"].as_str().unwrap_or_default();
        let shape = TypeText::shape("py", written)
            .map_err(|message| Self::failure(&self.path, &message))?;
        self.require_model(&shape, models)?;
        Ok((
            field["page"].as_str().unwrap_or_default().to_owned(),
            json!({ "name": field["name"], "shape": shape, "immutable": field["immutable"] }),
        ))
    }

    fn methods(&self, models: &ModelTable) -> Result<Map<String, Value>, Failure> {
        self.members["methods"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|method| {
                !self
                    .events
                    .handles(method["name"].as_str().unwrap_or_default())
            })
            .map(|method| self.method(method, models))
            .collect()
    }

    fn method(&self, method: &Value, models: &ModelTable) -> Result<(String, Value), Failure> {
        method["argumentTypes"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(std::iter::once(&method["returnType"]))
            .try_for_each(|shape| self.require_shape(shape, models))?;
        Ok((
            method["page"].as_str().unwrap_or_default().to_owned(),
            method.clone(),
        ))
    }

    fn require_shape(&self, value: &Value, models: &ModelTable) -> Result<(), Failure> {
        let written = value.as_str().unwrap_or_default();
        let shape = TypeText::shape("py", written)
            .map_err(|message| Self::failure(&self.path, &message))?;
        self.require_model(&shape, models)
    }

    fn require_model(
        &self,
        shape: &crate::route_types::Shape,
        models: &ModelTable,
    ) -> Result<(), Failure> {
        models.check_names(shape).map_err(|message| {
            Self::failure(
                &self.path,
                &message.replace("server/models/", "client/models/"),
            )
        })?;
        models
            .check_readable(shape)
            .map_err(|message| Self::failure(&self.path, &message))
    }

    pub(super) fn failure(path: &Path, message: &str) -> Failure {
        Failure::one(crate::failure::diagnostic(
            1605,
            format!("Cannot compile Python Tac service: {message}"),
            Some(
                "Keep Python Tac OnStop and OnEvent methods synchronous and pass only typed JSON values."
                    .to_owned(),
            ),
            crate::failure::source_span(&path.to_string_lossy(), 0, 1),
        ))
    }
}
