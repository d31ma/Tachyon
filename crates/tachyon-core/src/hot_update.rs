use crate::Failure;
use crate::failure::diagnostic;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use std::collections::BTreeSet;
use std::path::{Component, Path};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

const WATCH_QUEUE_CAPACITY: usize = 1_024;
const MAX_CHANGED_PATHS: usize = 4_096;

/// One bounded source change batch and its narrowest safe browser action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceChanges {
    paths: BTreeSet<String>,
    force_reload: bool,
}

impl SourceChanges {
    pub(crate) fn new() -> Self {
        Self {
            paths: BTreeSet::new(),
            force_reload: false,
        }
    }

    pub(crate) fn record_event(&mut self, project_root: &Path, event: Event) {
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        if event.paths.is_empty() {
            self.force_reload = true;
            return;
        }
        for path in event.paths {
            self.record_path(project_root, &path);
        }
    }

    pub(crate) fn force_reload(&mut self) {
        self.force_reload = true;
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.paths.is_empty() && !self.force_reload
    }

    pub(crate) fn paths(&self) -> Vec<String> {
        self.paths.iter().cloned().collect()
    }

    pub(crate) fn action(&self) -> SourceAction {
        if self.force_reload || self.paths.is_empty() {
            return SourceAction::Reload;
        }
        if self.paths.iter().all(|path| is_stylesheet(path)) {
            return SourceAction::Css;
        }

        let boundaries = self
            .paths
            .iter()
            .map(|path| island_boundary(path))
            .collect::<Option<BTreeSet<_>>>();
        boundaries.map_or(SourceAction::Reload, |boundaries| SourceAction::Island {
            boundaries: boundaries.into_iter().collect(),
        })
    }

    fn record_path(&mut self, project_root: &Path, path: &Path) {
        let Ok(relative) = path.strip_prefix(project_root) else {
            self.force_reload = true;
            return;
        };
        if !is_source_path(relative) {
            return;
        }
        let Some(portable) = portable_path(relative) else {
            self.force_reload = true;
            return;
        };
        if self.paths.len() >= MAX_CHANGED_PATHS {
            self.force_reload = true;
            return;
        }
        self.paths.insert(portable);
    }
}

/// Narrowest browser action supported for one successful rebuild.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SourceAction {
    Css,
    Island { boundaries: Vec<String> },
    Reload,
}

/// An operating-system source watcher and its bounded event receiver.
pub(crate) struct SourceWatcher {
    _watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<Result<Event, notify::Error>>,
    overflowed: Arc<AtomicBool>,
}

impl SourceWatcher {
    pub(crate) fn start(project_root: &Path) -> Result<Self, Failure> {
        let (sender, receiver) = mpsc::channel(WATCH_QUEUE_CAPACITY);
        let overflowed = Arc::new(AtomicBool::new(false));
        let callback_overflowed = Arc::clone(&overflowed);
        let mut watcher = notify::recommended_watcher(move |event| {
            if sender.try_send(event).is_err() {
                callback_overflowed.store(true, Ordering::Release);
            }
        })
        .map_err(|error| watcher_failure(&error))?;

        watcher
            .watch(project_root, RecursiveMode::NonRecursive)
            .map_err(|error| watcher_failure(&error))?;
        for directory in ["client", "server"] {
            let path = project_root.join(directory);
            if path.is_dir() {
                watcher
                    .watch(&path, RecursiveMode::Recursive)
                    .map_err(|error| watcher_failure(&error))?;
            }
        }

        Ok(Self {
            _watcher: watcher,
            receiver,
            overflowed,
        })
    }

    pub(crate) async fn receive(&mut self) -> Option<Result<Event, notify::Error>> {
        self.receiver.recv().await
    }

    pub(crate) fn drain(&mut self, changes: &mut SourceChanges, project_root: &Path) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Ok(event) => changes.record_event(project_root, event),
                Err(_) => changes.force_reload(),
            }
        }
        if self.overflowed.swap(false, Ordering::AcqRel) {
            changes.force_reload();
        }
    }
}

fn watcher_failure(error: &notify::Error) -> Failure {
    Failure::one(diagnostic(
        1305,
        format!("Cannot watch project sources: {error}"),
        Some(String::from(
            "Check source-directory permissions or run the development server with --no-watch.",
        )),
        None,
    ))
}

fn is_source_path(path: &Path) -> bool {
    let Some(first) = path.components().next() else {
        return false;
    };
    match first {
        Component::Normal(value) if value == "client" || value == "server" => true,
        Component::Normal(_) if path.components().count() == 1 => path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| {
                name == "tachyon.json"
                    || name == "tac.config.js"
                    || name == "tac.config.mjs"
                    || name == "tac.config.ts"
                    || name == "manifest.json"
                    || name == ".tachyonrc"
                    || name.starts_with("middleware.")
            }),
        _ => false,
    }
}

fn portable_path(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(value) = component else {
            return None;
        };
        parts.push(value.to_str()?);
    }
    Some(parts.join("/"))
}

fn is_stylesheet(path: &str) -> bool {
    path.starts_with("client/")
        && Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("css"))
}

fn island_boundary(path: &str) -> Option<String> {
    let relative = path.strip_prefix("client/components/")?;
    let (directory, file) = relative.rsplit_once('/')?;
    if !matches!(
        file,
        "tac.js" | "tac.ts" | "tachyon-island.js" | "tachyon-island.ts"
    ) {
        return None;
    }
    let parts = directory.split('/').collect::<Vec<_>>();
    if parts.is_empty()
        || !parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
    {
        return None;
    }
    Some(parts.join("-"))
}

#[cfg(test)]
mod tests {
    use super::{SourceAction, SourceChanges};
    use notify::{Event, EventKind};
    use std::path::Path;

    fn changes(paths: &[&str]) -> SourceChanges {
        let root = Path::new("/project");
        let mut changes = SourceChanges::new();
        changes.record_event(
            root,
            Event {
                kind: EventKind::Any,
                paths: paths.iter().map(|path| root.join(path)).collect(),
                attrs: notify::event::EventAttributes::new(),
            },
        );
        changes
    }

    #[test]
    fn stylesheets_are_the_only_css_hot_update_boundary() {
        assert_eq!(
            changes(&["client/pages/tac.css"]).action(),
            SourceAction::Css
        );
        assert_eq!(
            changes(&["client/components/card/tac.css", "client/shared/theme.css"]).action(),
            SourceAction::Css
        );
        assert_eq!(
            changes(&["client/pages/tac.css", "client/pages/tac.html"]).action(),
            SourceAction::Reload
        );
    }

    #[test]
    fn companion_code_maps_to_canonical_island_boundaries() {
        assert_eq!(
            changes(&[
                "client/components/product/card/tac.js",
                "client/components/cart-panel/tachyon-island.ts"
            ])
            .action(),
            SourceAction::Island {
                boundaries: vec![String::from("cart-panel"), String::from("product-card")]
            }
        );
        assert_eq!(
            changes(&["client/components/product/card/tac.html"]).action(),
            SourceAction::Reload
        );
    }

    #[test]
    fn only_browser_companions_receive_component_hot_updates() {
        for name in ["tac.js", "tac.ts", "tachyon-island.js", "tachyon-island.ts"] {
            assert_eq!(
                changes(&[&format!("client/components/card/{name}")]).action(),
                SourceAction::Island {
                    boundaries: vec![String::from("card")]
                }
            );
        }
        for name in [
            "tac.rs",
            "tac.kt",
            "tac.swift",
            "tac.cs",
            "tac.dart",
            "tachyon-wasm.rs",
            "tachyon-wasm.kt",
            "tachyon-wasm.swift",
            "tachyon-wasm.cs",
            "tachyon-wasm.dart",
        ] {
            for directory in ["client/components/card", "client/pages"] {
                let changed = changes(&[&format!("{directory}/{name}")]);
                assert!(!changed.is_empty(), "{directory}/{name}");
                assert_eq!(changed.action(), SourceAction::Reload, "{directory}/{name}");
            }
        }
    }

    #[test]
    fn every_supported_root_configuration_invalidates_the_dev_build() {
        for name in [
            "tachyon.json",
            "tac.config.js",
            "tac.config.mjs",
            "tac.config.ts",
            "manifest.json",
            ".tachyonrc",
        ] {
            let changed = changes(&[name]);
            assert_eq!(changed.paths(), vec![String::from(name)]);
            assert_eq!(changed.action(), SourceAction::Reload);
        }
    }

    #[test]
    fn generated_output_and_dependency_events_are_ignored() {
        let changes = changes(&["dist/index.html", "target/debug/ty", "node_modules/x.js"]);
        assert!(changes.is_empty());
    }
}
