use super::{
    CppChecks, CppConverter, CppFolder, CppLowering, CppModel, CppPrelude, CppService,
    CppServiceGraph,
};
use crate::Failure;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub(crate) struct CppProgram {
    models: Vec<CppModel>,
    services: Vec<CppService>,
    registered: Vec<(String, String)>,
    records_calls: bool,
}

impl CppProgram {
    const MODELS: &'static str = "client/models";
    const GENERATED: &'static str = "#line 1 \"tachyon-generated.cpp\"\n";

    pub(crate) fn read(root: &Path, chosen: &[(String, String, PathBuf)]) -> Result<Self, Failure> {
        let mut graph = CppServiceGraph::new(root)?;
        let registered = Self::register(&mut graph, chosen)?;
        Self {
            models: Self::models(root)?,
            services: graph.finish(),
            registered,
            records_calls: false,
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
        }
        .checked()
    }

    fn checked(self) -> Result<Self, Failure> {
        let checked: Vec<String> = self
            .models
            .iter()
            .filter(|model| model.is_checked())
            .map(|model| model.name().to_owned())
            .collect();
        for service in &self.services {
            for (type_name, at) in CppChecks::checked_types(service.class()) {
                CppChecks::require_checked(&service.path, at, &type_name, &checked)?;
            }
            if service.is_converter() {
                CppConverter::new(&service.path, &self.models).check_methods(service.class())?;
            }
        }
        Ok(self)
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
        let mut models = Vec::new();
        for path in CppFolder::files(root, Self::MODELS)? {
            let (relative, source) = CppFolder::read(root, &path)?;
            models.push(CppModel::read(&relative, &source)?);
        }
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
        }
        text
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
