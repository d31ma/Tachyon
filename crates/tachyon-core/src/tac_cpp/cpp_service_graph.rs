use super::{CppFailure, CppFolder, CppService};
use crate::Failure;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub(crate) struct CppServiceGraph<'a> {
    root: &'a Path,
    available: BTreeMap<String, PathBuf>,
    visiting: BTreeSet<PathBuf>,
    ordered: Vec<CppService>,
}

impl<'a> CppServiceGraph<'a> {
    const FOLDERS: [&'static str; 5] = [
        "client/services",
        "client/remotes",
        "client/converters",
        "client/helpers",
        "client/stores",
    ];
    const CIRCLE: &'static str =
        "C++ services need each other in a circle; break the circle with an event";

    pub(crate) fn new(root: &'a Path) -> Result<Self, Failure> {
        let mut files = Vec::new();
        for folder in Self::FOLDERS {
            files.extend(CppFolder::files(root, folder)?);
        }
        Ok(Self {
            root,
            available: Self::by_class(files),
            visiting: BTreeSet::new(),
            ordered: Vec::new(),
        })
    }

    pub(crate) fn add(&mut self, path: &Path) -> Result<(), Failure> {
        self.add_service(path)
    }

    pub(crate) fn add_every(&mut self) -> Result<(), Failure> {
        self.add_all()
    }

    pub(crate) fn finish(self) -> Vec<CppService> {
        self.ordered
    }

    fn by_class(files: Vec<PathBuf>) -> BTreeMap<String, PathBuf> {
        let mut available = BTreeMap::new();
        for path in files {
            let stem = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            available.insert(stem, path);
        }
        available
    }

    fn add_all(&mut self) -> Result<(), Failure> {
        let paths: Vec<PathBuf> = self.available.values().cloned().collect();
        for path in paths {
            self.add_service(&path)?;
        }
        Ok(())
    }

    fn add_service(&mut self, path: &Path) -> Result<(), Failure> {
        let added = self
            .ordered
            .iter()
            .any(|service| self.root.join(&service.path) == path);
        added.then_some(()).map_or_else(|| self.add_new(path), Ok)
    }

    fn add_new(&mut self, path: &Path) -> Result<(), Failure> {
        let (relative, source) = CppFolder::read(self.root, path)?;
        self.visiting
            .insert(relative.clone())
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(&relative, 0, Self::CIRCLE))?;
        let service = CppService::read(&relative, &source)?;
        self.add_needed(&relative, &service)?;
        self.ordered.push(service);
        Ok(())
    }

    fn add_needed(&mut self, relative: &Path, service: &CppService) -> Result<(), Failure> {
        for (field, needed) in service.needs() {
            let path = self.needed_path(relative, service, &field, &needed)?;
            self.add_service(&path)?;
        }
        Ok(())
    }

    fn needed_path(
        &self,
        relative: &Path,
        service: &CppService,
        field: &str,
        needed: &str,
    ) -> Result<PathBuf, Failure> {
        self.available.get(needed).cloned().ok_or_else(|| {
            CppFailure::pending(relative, service.field_at(field), &format!(
                "A C++ [[Needs]] field names another C++ service in client/services, a C++ [[HTTPClient]] in client/remotes, a C++ [[Converter]] in client/converters, a C++ [[Helper]] in client/helpers or a C++ [[Store]] in client/stores; there is no {needed}.hpp in any of them. Other C++ roles come later (ADR 0099)"
            ))
        })
    }
}
