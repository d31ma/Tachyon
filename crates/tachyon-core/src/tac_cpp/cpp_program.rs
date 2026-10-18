use super::{
    CppChecks, CppConverter, CppFailure, CppFolder, CppLowering, CppModel, CppPrelude, CppService,
    CppServiceGraph, CppStore,
};
use crate::Failure;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub(crate) struct CppProgram {
    models: Vec<CppModel>,
    services: Vec<CppService>,
    registered: Vec<(String, String)>,
    records_calls: bool,
    stores: BTreeMap<String, String>,
}

impl CppProgram {
    const MODELS: [&'static str; 2] = ["client/models", "client/entities"];
    const SHARED_ENTITY: &'static str =
        "One C++ [[Store]] keeps each [[Entity]]; another Store already names this one";
    const GENERATED: &'static str = "#line 1 \"tachyon-generated.cpp\"\n";

    pub(crate) fn read(root: &Path, chosen: &[(String, String, PathBuf)]) -> Result<Self, Failure> {
        let mut graph = CppServiceGraph::new(root)?;
        let registered = Self::register(&mut graph, chosen)?;
        Self {
            models: Self::models(root)?,
            services: graph.finish(),
            registered,
            records_calls: false,
            stores: BTreeMap::new(),
        }
        .checked()
    }

    pub(crate) fn tested(root: &Path) -> Result<Self, Failure> {
        let mut graph = CppServiceGraph::new(root)?;
        graph.add_every()?;
        Self {
            models: Self::models(root)?,
            services: graph.finish(),
            registered: Vec::new(),
            records_calls: true,
            stores: BTreeMap::new(),
        }
        .checked()
    }

    fn checked(mut self) -> Result<Self, Failure> {
        let checked: Vec<String> = self
            .models
            .iter()
            .filter(|model| model.is_checked())
            .map(|model| model.name().to_owned())
            .collect();
        self.services
            .iter()
            .try_for_each(|service| self.check_service(service, &checked))?;
        self.stores = self.store_definitions()?;
        Ok(self)
    }

    fn check_service(&self, service: &CppService, checked: &[String]) -> Result<(), Failure> {
        CppChecks::checked_types(service.class())
            .into_iter()
            .try_for_each(|(type_name, at)| {
                CppChecks::require_checked(&service.path, at, &type_name, checked)
            })?;
        service
            .is_converter()
            .then(|| CppConverter::new(&service.path, &self.models).check_methods(service.class()))
            .transpose()
            .map(drop)
    }

    fn store_definitions(&self) -> Result<BTreeMap<String, String>, Failure> {
        let entities: Vec<Value> = self
            .models
            .iter()
            .filter_map(|model| model.entity.clone())
            .collect();
        let written = self
            .services
            .iter()
            .map(|service| {
                CppStore::definitions(&service.path, &service.source, service.class(), &entities)
                    .map(|found| found.map(|(entity, text)| (service, entity, text)))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut kept: Vec<String> = Vec::new();
        let mut stores = BTreeMap::new();
        for (service, entity, text) in written.into_iter().flatten() {
            Self::keep_once(&mut kept, service, entity)?;
            stores.insert(service.class().name.clone(), text);
        }
        Ok(stores)
    }

    fn keep_once(
        kept: &mut Vec<String>,
        service: &CppService,
        entity: String,
    ) -> Result<(), Failure> {
        let repeated = kept.contains(&entity);
        kept.push(entity);
        (!repeated).then_some(()).ok_or_else(|| {
            CppFailure::unreadable(&service.path, service.class().name_at, Self::SHARED_ENTITY)
        })
    }

    pub(crate) fn has_class(&self, name: &str) -> bool {
        self.services
            .iter()
            .any(|service| service.class().name == name)
    }

    pub(crate) fn web_source(&self) -> String {
        format!(
            "{}{}{}{}{}",
            CppPrelude::web(),
            self.authored(),
            self.bindings(),
            self.registration(),
            CppPrelude::EXPORTS
        )
    }

    pub(crate) fn test_source(&self, units: &str) -> String {
        format!("{}{}{units}", CppPrelude::test(), self.authored())
    }

    pub(crate) fn located(path: &Path, lowered: &str) -> String {
        format!("#line 1 \"{}\"\n{lowered}\n", Self::portable(path))
    }

    fn register(
        graph: &mut CppServiceGraph<'_>,
        chosen: &[(String, String, PathBuf)],
    ) -> Result<Vec<(String, String)>, Failure> {
        let mut registered = Vec::new();
        for (instance, class, path) in chosen {
            registered.push((instance.clone(), class.clone()));
            graph.add(path)?;
        }
        Ok(registered)
    }

    fn models(root: &Path) -> Result<Vec<CppModel>, Failure> {
        let models = Self::MODELS
            .iter()
            .map(|folder| CppFolder::files(root, folder))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .map(|path| CppModel::read_in(root, &path))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::ordered_models(models))
    }

    fn ordered_models(mut waiting: Vec<CppModel>) -> Vec<CppModel> {
        let mut ordered: Vec<CppModel> = Vec::new();
        while !waiting.is_empty() {
            let names: Vec<String> = waiting
                .iter()
                .map(|model| model.name().to_owned())
                .collect();
            let ready = waiting
                .iter()
                .position(|model| !Self::mentions_any(model, &names))
                .unwrap_or(0);
            ordered.push(waiting.remove(ready));
        }
        ordered
    }

    fn mentions_any(model: &CppModel, names: &[String]) -> bool {
        let types = model.field_types().join(" ");
        let words: Vec<&str> = types
            .split(|character: char| {
                [character.is_ascii_alphanumeric(), character == '_'] == [false, false]
            })
            .collect();
        names
            .iter()
            .any(|name| [name != model.name(), words.contains(&name.as_str())] == [true, true])
    }

    fn authored(&self) -> String {
        format!(
            "{}{}{}{}",
            self.located_models(),
            Self::GENERATED,
            self.codecs(),
            self.located_services()
        )
    }

    fn located_models(&self) -> String {
        let mut text = String::new();
        for model in &self.models {
            text.push_str(&Self::located(&model.path, &model.lowered()));
        }
        text
    }

    fn codecs(&self) -> String {
        let mut text = String::new();
        for model in &self.models {
            text.push_str(&model.codec());
        }
        text
    }

    fn located_services(&self) -> String {
        let mut text = String::new();
        for service in &self.services {
            text.push_str(&Self::located(&service.path, &self.lowered(service)));
            text.push_str(&self.conversions(service));
            text.push_str(&self.store_text(service));
        }
        text
    }

    fn store_text(&self, service: &CppService) -> String {
        self.stores
            .get(&service.class().name)
            .map_or_else(String::new, |definitions| {
                format!("{}{definitions}", Self::GENERATED)
            })
    }

    fn conversions(&self, service: &CppService) -> String {
        if !service.is_converter() {
            return String::new();
        }
        let definitions =
            CppConverter::new(&service.path, &self.models).definitions(service.class());
        format!("{}{definitions}", Self::GENERATED)
    }

    fn lowered(&self, service: &CppService) -> String {
        let prototypes = self.prototypes();
        if self.records_calls {
            return CppLowering::recorded_service(service, &prototypes);
        }
        CppLowering::service(service, &prototypes)
    }

    fn prototypes(&self) -> Vec<String> {
        self.services
            .iter()
            .filter(|service| service.is_prototype())
            .map(|service| service.class().name.clone())
            .collect()
    }

    fn bindings(&self) -> String {
        let mut text = String::from(Self::GENERATED);
        for service in &self.services {
            text.push_str(&service.binding());
        }
        text
    }

    fn registration(&self) -> String {
        let mut entries = String::new();
        for (instance, class) in &self.registered {
            let _ = writeln!(
                entries,
                "    services[\"{instance}\"] = TachyonBinding{class}::type();"
            );
        }
        format!(
            "std::map<std::string, tachyon::TacServiceType> tachyon::TacProgram::services() {{\n    std::map<std::string, tachyon::TacServiceType> services;\n{entries}    return services;\n}}\n"
        )
    }

    fn portable(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }
}
