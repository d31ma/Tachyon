use super::RuntimeProfile;
use super::python_build::PythonBuild;
use super::python_graph_guard::PythonGraphGuard;
use super::python_provider_source::PythonProviderSource;
use super::runtime_assets::RuntimeAssets;
use crate::Failure;
use crate::annotation_modules::Module;
use crate::model_sources::ModelSources;
use crate::route_types::ModelTable;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub(crate) struct PythonWebServices;

impl PythonWebServices {
    const PRELUDE: &'static str = concat!(
        include_str!("python_codec.py"),
        "\n",
        include_str!("python_owner.py"),
        "\n",
        include_str!("python_publisher.py"),
        "\n",
        include_str!("python_graph.py"),
        "\n",
        include_str!("python_runtime.py"),
    );
    const ENGINE: &'static str = concat!(
        include_str!("../native/browser/tac-task-answers.js"),
        include_str!("python_host.js")
    );

    pub(crate) fn build(
        root: &Path,
        services: &[(String, String, PathBuf)],
    ) -> Result<PythonBuild, Failure> {
        Self::require_profile(root)?;
        let program = Self::program(root, services)?;
        let quoted = serde_json::to_string(&program).map_err(|error| {
            RuntimeAssets::failure(
                "client/services",
                &format!("cannot encode Python program: {error}"),
            )
        })?;
        let engine = Self::ENGINE.replace("__TAC_PYTHON_PROGRAM__", &quoted);
        let assets = RuntimeAssets::read(&RuntimeAssets::cached_root())?
            .into_iter()
            .chain([RuntimeAssets::notices()])
            .map(Self::service_asset)
            .collect();
        Ok(PythonBuild { engine, assets })
    }

    pub(crate) fn program(
        root: &Path,
        services: &[(String, String, PathBuf)],
    ) -> Result<String, Failure> {
        let entry = Self::entry(services)?;
        let relative = entry.strip_prefix(root).unwrap_or(entry);
        let units = Module::captured_units(root, relative)?;
        let authored = Module::with_models(relative, "", units.clone())?;
        let manual = crate::yon::ManualHelpers::read(relative, &authored)?;
        let providers = Self::providers(&units, &manual)?;
        let models = ModelSources::read(root, "client", "py")?;
        let table = ModelTable::read_realm(root, "client", "py")
            .map_err(|message| PythonProviderSource::failure(relative, &message))?;
        let descriptions = Self::descriptions(&providers, &table)?;
        PythonGraphGuard::check(&descriptions)?;
        let metadata = Self::metadata(services, &descriptions, &table);
        let source = Self::prepared(relative, units, &providers)?;
        let staged = Self::lowered(relative, &source, &models)?;
        Self::configured(&staged, &providers, &metadata)
    }

    fn require_profile(root: &Path) -> Result<(), Failure> {
        if RuntimeProfile::for_project(root)?.is_none() {
            return Err(super::RuntimeInventory::profile_missing());
        }
        Ok(())
    }

    fn service_asset((name, bytes): (String, Vec<u8>)) -> (String, Vec<u8>) {
        (
            name.trim_start_matches(".tachyon/services/").to_owned(),
            bytes,
        )
    }

    fn entry(services: &[(String, String, PathBuf)]) -> Result<&Path, Failure> {
        services
            .first()
            .map(|(_, _, path)| path.as_path())
            .ok_or_else(|| {
                PythonProviderSource::failure(
                    Path::new("client/services"),
                    "Python build requires a reachable service.",
                )
            })
    }

    fn providers(
        units: &[(PathBuf, String)],
        manual: &BTreeSet<String>,
    ) -> Result<Vec<PythonProviderSource>, Failure> {
        units
            .iter()
            .filter(|(path, _)| Self::provider_path(path))
            .map(|(path, source)| PythonProviderSource::read(path, source))
            .collect::<Result<Vec<_>, _>>()
            .map(|sources| {
                sources
                    .into_iter()
                    .filter(|source| !source.manual(manual))
                    .collect()
            })
    }

    fn provider_path(path: &Path) -> bool {
        [
            "client/services",
            "client/helpers",
            "client/stores",
            "client/remotes",
        ]
        .iter()
        .any(|folder| path.starts_with(folder))
    }

    fn descriptions(
        providers: &[PythonProviderSource],
        models: &ModelTable,
    ) -> Result<Map<String, Value>, Failure> {
        let names = providers
            .iter()
            .map(|provider| provider.class.clone())
            .collect::<BTreeSet<_>>();
        providers
            .iter()
            .map(|provider| {
                provider
                    .describe(&names, models)
                    .map(|description| (provider.class.clone(), description))
            })
            .collect()
    }

    fn metadata(
        services: &[(String, String, PathBuf)],
        providers: &Map<String, Value>,
        models: &ModelTable,
    ) -> Value {
        let services = services
            .iter()
            .map(|(name, class, _)| (name.clone(), json!(class)))
            .collect::<Map<_, _>>();
        json!({ "services": services, "providers": providers, "models": models.model_schemas() })
    }

    fn prepared(
        entry: &Path,
        units: Vec<(PathBuf, String)>,
        providers: &[PythonProviderSource],
    ) -> Result<String, Failure> {
        let plain = units
            .into_iter()
            .map(|(path, source)| Self::plain_unit(path, &source, providers))
            .collect::<Result<Vec<_>, _>>()?;
        Module::with_models(entry, "", plain)
    }

    fn plain_unit(
        path: PathBuf,
        source: &str,
        providers: &[PythonProviderSource],
    ) -> Result<(PathBuf, String), Failure> {
        let prepared = providers
            .iter()
            .find(|provider| provider.path == path)
            .map(|provider| provider.source.clone());
        let source = prepared.map_or_else(
            || crate::behavior::ServiceRoles::without_roles(&path, source),
            Ok,
        )?;
        Ok((path, source))
    }

    fn lowered(entry: &Path, source: &str, models: &ModelSources) -> Result<String, Failure> {
        let metadata = models.metadata();
        let assembled = metadata.assembled(entry, source)?;
        let lowered = crate::behavior::lower(entry, &assembled, "tac")?.unwrap_or(assembled);
        let stripped = metadata.without_declarations(entry, &lowered)?;
        let imports = Self::model_imports(models);
        let bridge = format!("from __future__ import annotations\n{imports}");
        let program = Module::with_models(
            entry,
            &stripped,
            vec![(
                PathBuf::from("client/models/__tachyon_python_imports__.py"),
                bridge,
            )],
        )?;
        Ok(models.add_to_program(&program))
    }

    fn model_imports(models: &ModelSources) -> String {
        let names = models
            .project_files()
            .iter()
            .flat_map(|(path, source)| {
                crate::annotation_layout::declaration_names(path, source).unwrap_or_default()
            })
            .collect::<BTreeSet<_>>();
        let mut imports = String::new();
        for name in names {
            let _ = writeln!(imports, "from models import {name}");
        }
        imports
    }

    fn configured(
        program: &str,
        providers: &[PythonProviderSource],
        metadata: &Value,
    ) -> Result<String, Failure> {
        let types = providers
            .iter()
            .map(|provider| format!("{:?}: {}", provider.class, provider.class))
            .collect::<Vec<_>>()
            .join(", ");
        let (import, models) = Self::model_namespace(metadata);
        let metadata = serde_json::to_string(&metadata.to_string()).map_err(|error| {
            PythonProviderSource::failure(
                Path::new("client/services"),
                &format!("Cannot encode Python metadata: {error}"),
            )
        })?;
        Ok(format!(
            "{program}\n{}\n{import}\n_TacPythonRuntime.configure({{{types}}}, _tac_json.loads({metadata}), {models})\n",
            Self::PRELUDE
        ))
    }
    fn model_namespace(metadata: &Value) -> (&'static str, &'static str) {
        if metadata["models"]
            .as_object()
            .is_some_and(|models| !models.is_empty())
        {
            return ("import models as _tac_models", "_tac_models.__dict__");
        }
        ("", "{}")
    }
}
