//! Repository-wide policy gates owned by the canonical contract crate.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use tachyon_contracts::CONTRACTS;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            Path::to_path_buf,
        )
}

fn collect_files(root: &Path, extension: &str) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();

    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
                files.push(path);
            }
        }
    }

    files.sort();
    Ok(files)
}

fn relative_markdown_links(contents: &str) -> Vec<&str> {
    contents
        .split("](")
        .skip(1)
        .filter_map(|tail| tail.split(')').next())
        .map(str::trim)
        .filter(|target| {
            !target.is_empty()
                && !target.starts_with('#')
                && !target.contains("://")
                && !target.starts_with("mailto:")
        })
        .collect()
}

#[test]
fn required_foundation_documents_are_present_and_substantive()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let required = [
        "AGENTS.md",
        "CHANGELOG.md",
        "CODE_OF_CONDUCT.md",
        "CONTEXT.md",
        "CONTRIBUTING.md",
        "GOVERNANCE.md",
        "README.md",
        "SECURITY.md",
        "SUPPORT.md",
        "docs/ENGINEERING_STANDARDS.md",
        "docs/PHASE_1_EVIDENCE.md",
        "docs/PHASE_1_SPEC.md",
        "docs/PHASE_2_EVIDENCE.md",
        "docs/PHASE_2_SPEC.md",
        "docs/PHASE_3_EVIDENCE.md",
        "docs/PHASE_3_IMPLEMENTATION_PLAN.md",
        "docs/PHASE_3_SPEC.md",
        "docs/PHASE_4_EVIDENCE.md",
        "docs/PHASE_4_IMPLEMENTATION_PLAN.md",
        "docs/PHASE_4_SPEC.md",
        "docs/PROJECT_PLAN.md",
        "docs/RELEASE_ENGINEERING.md",
        "docs/SUPPORT_TIERS.md",
        "docs/THREAT_MODEL.md",
        "docs/architecture/OVERVIEW.md",
    ];

    for relative in required {
        let contents = fs::read_to_string(root.join(relative))?;
        assert!(
            contents.lines().count() >= 10,
            "{relative} is missing or not substantive"
        );
    }

    Ok(())
}

#[test]
fn release_artifact_inputs_are_present_and_nonempty() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    for relative in ["LICENSE", "NOTICE", "VERSION", "docs/SUPPORT_TIERS.md"] {
        let contents = fs::read(root.join(relative))?;
        assert!(!contents.is_empty(), "{relative} is empty");
    }
    Ok(())
}

#[test]
fn rewrite_is_the_only_in_tree_framework_implementation() -> Result<(), Box<dyn std::error::Error>>
{
    let root = repository_root();
    for obsolete in [
        "src",
        "tests",
        "bunfig.toml",
        "tsconfig.src.json",
        "tsconfig.tests.json",
        "scripts/typecheck.js",
        "scripts/windows-native-compile-smoke.js",
        "scripts/bench",
    ] {
        assert!(
            !root.join(obsolete).exists(),
            "the removed JavaScript implementation path was reintroduced: {obsolete}"
        );
    }

    assert!(root.join("api/types/tachyon-env.d.ts").is_file());
    assert!(
        root.join("crates/tachyon-cli/tests/fixtures/migration-fullstack")
            .is_dir()
    );

    let package: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("package.json"))?)?;
    let scripts = package["scripts"]
        .as_object()
        .ok_or("package.json scripts must be an object")?;
    for command in ["init", "serve", "bundle", "preview", "start"] {
        assert!(
            scripts[command]
                .as_str()
                .is_some_and(|script| script.starts_with("cargo run --locked --bin ty --")),
            "package script {command} does not invoke the Rust CLI"
        );
    }
    for script in scripts.values().filter_map(serde_json::Value::as_str) {
        assert!(!script.contains("src/cli"));
        assert!(!script.contains("bun test ./tests"));
    }

    let workflow = fs::read_to_string(root.join(".github/workflows/rust-ci.yml"))?;
    assert!(workflow.contains("Rewrite-only repository policy"));
    assert!(workflow.contains("RELEASED_TY_BIN="));
    assert!(workflow.contains("crates/tachyon-cli/tests/fixtures/migration-fullstack"));
    Ok(())
}

#[test]
fn every_phase7_fuzz_boundary_has_a_target_seed_and_ci_campaign()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let manifest = fs::read_to_string(root.join("fuzz/Cargo.toml"))?;
    let lockfile = fs::read_to_string(root.join("fuzz/Cargo.lock"))?;
    let workflow = fs::read_to_string(root.join(".github/workflows/rust-ci.yml"))?;
    let scheduled = fs::read_to_string(root.join(".github/workflows/fuzz-scheduled.yml"))?;

    assert!(manifest.contains("libfuzzer-sys = \"=0.4.10\""));
    assert!(lockfile.contains("name = \"libfuzzer-sys\""));
    for target in [
        "html_frontend",
        "template_frontend",
        "handler_frame",
        "native_planner",
    ] {
        assert!(manifest.contains(&format!("name = \"{target}\"")));
        assert!(
            root.join(format!("fuzz/fuzz_targets/{target}.rs"))
                .is_file()
        );
        let seeds = fs::read_dir(root.join(format!("fuzz/corpus/{target}")))?
            .collect::<Result<Vec<_>, _>>()?;
        assert!(!seeds.is_empty(), "{target} has no hand-written seed");
    }
    assert!(
        workflow
            .contains("for target in html_frontend template_frontend handler_frame native_planner")
    );
    assert!(workflow.contains("-max_total_time=${FUZZ_SECONDS}"));
    assert!(scheduled.contains("Long-form persistent-corpus fuzzing"));
    assert!(scheduled.contains("actions/cache/restore@caa296126883cff596d87d8935842f9db880ef25"));
    assert!(scheduled.contains("actions/cache/save@caa296126883cff596d87d8935842f9db880ef25"));
    assert!(scheduled.contains("fuzz-corpus-${{ runner.os }}-${{ github.run_id }}"));
    Ok(())
}

#[test]
fn architecture_adrs_are_accepted_and_numbered() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let adrs = collect_files(&root.join("docs/adr"), "md")?;
    let numbered = adrs
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| {
                    name.len() > 5
                        && name.as_bytes()[..4].iter().all(u8::is_ascii_digit)
                        && name.as_bytes()[4] == b'-'
                })
        })
        .collect::<Vec<_>>();

    assert_eq!(numbered.len(), 12);
    for (index, adr) in numbered.into_iter().enumerate() {
        let expected_prefix = format!("{:04}-", index + 1);
        assert!(
            adr.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.starts_with(&expected_prefix)),
            "{} is not the expected ADR {expected_prefix}",
            adr.display()
        );
        let contents = fs::read_to_string(adr)?;
        assert!(
            contents
                .lines()
                .take(12)
                .any(|line| line.contains("Accepted")),
            "{} is not accepted",
            adr.display()
        );
    }

    Ok(())
}

#[test]
fn schema_directories_and_registry_cannot_drift() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let schemas = collect_files(&root.join("api"), "json")?
        .into_iter()
        .filter(|path| path.file_name().and_then(|name| name.to_str()) == Some("schema.json"))
        .collect::<Vec<_>>();

    let registered = CONTRACTS
        .iter()
        .map(|contract| contract.name)
        .collect::<BTreeSet<_>>();
    let on_disk = schemas
        .iter()
        .filter_map(|path| {
            path.ancestors()
                .nth(2)
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
        })
        .collect::<BTreeSet<_>>();

    assert_eq!(on_disk, registered);
    Ok(())
}

#[test]
fn github_actions_are_pinned_to_full_commits() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let workflows = collect_files(&root.join(".github/workflows"), "yml")?;

    for workflow in workflows {
        let contents = fs::read_to_string(&workflow)?;
        for line in contents.lines() {
            let trimmed = line.trim();
            let Some(action) = trimmed.strip_prefix("uses: ") else {
                continue;
            };
            if let Some(local) = action
                .split_whitespace()
                .next()
                .filter(|value| value.starts_with("./"))
            {
                assert!(
                    root.join(local.trim_start_matches("./")).exists(),
                    "{} references a missing local workflow: {action}",
                    workflow.display()
                );
                continue;
            }
            let revision = action
                .split('#')
                .next()
                .and_then(|value| value.rsplit_once('@'))
                .map(|(_, revision)| revision.trim())
                .unwrap_or_default();
            assert!(
                revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "{} contains an unpinned action: {action}",
                workflow.display()
            );
        }
    }

    Ok(())
}

#[test]
fn stable_release_is_tag_gated_and_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/publish.yml"))?;
    let unix_installer = fs::read_to_string(root.join("install.sh"))?;
    let windows_installer = fs::read_to_string(root.join("install.ps1"))?;

    assert!(workflow.contains("tags:"));
    assert!(workflow.contains("workflow_dispatch:"));
    assert!(!workflow.contains("branches: [main]"));
    assert!(workflow.contains("RELEASE_TAG: ${{ inputs.tag || github.ref_name }}"));
    assert!(workflow.contains("DEFAULT_BRANCH: ${{ github.event.repository.default_branch }}"));
    assert!(workflow.matches("contents: write").count() >= 4);
    assert!(workflow.contains("git cat-file -t"));
    assert!(workflow.contains("uses: ./.github/workflows/rust-ci.yml"));
    assert!(workflow.contains("--verify-tag --draft"));
    assert!(workflow.contains("gh attestation verify"));
    assert!(workflow.contains("cosign verify-blob"));
    assert!(workflow.contains("sha256sum --check --strict SHA256SUMS"));
    assert!(workflow.contains("release-bad"));
    assert!(workflow.contains("Copy-Item 'release/SHA256SUMS' 'release-bad/SHA256SUMS'"));
    assert!(workflow.contains("[IO.File]::AppendAllText"));
    assert!(workflow.contains("Checksum mismatch for $env:ASSET. Aborting."));
    assert!(
        workflow.contains("gh release download \"${TAG}\" --repo \"${REPOSITORY}\" --dir release")
    );
    assert!(
        workflow
            .contains("gh release edit \"${TAG}\" --repo \"${REPOSITORY}\" --draft=false --latest")
    );
    assert!(workflow.contains("failed upgrade replaced the installed executable"));

    assert!(unix_installer.contains("No checksum published for ${asset}. Aborting."));
    assert!(unix_installer.contains("mktemp \"${dest}/.ty.download.XXXXXX\""));
    assert!(unix_installer.contains("chmod 0755 \"$tmp\""));
    assert!(unix_installer.contains("mv \"$tmp\" \"$dest/ty\""));
    assert!(!unix_installer.contains("mktemp -d"));
    assert!(windows_installer.contains("No checksum published for $asset. Aborting."));
    assert!(windows_installer.contains("[Text.Encoding]::UTF8.GetString($sumsContent)"));
    assert!(!windows_installer.contains("Could not verify checksum"));
    assert!(windows_installer.contains("[IO.File]::Replace($download, $exe, $null)"));
    assert!(!windows_installer.contains("-OutFile $exe"));
    assert!(!unix_installer.contains("d31ma/Fylo"));
    assert!(!windows_installer.contains("d31ma/Fylo"));
    assert!(!unix_installer.contains("releases/latest/download/install.sh"));
    assert!(!windows_installer.contains("releases/latest/download/install.ps1"));
    Ok(())
}

#[test]
fn foundation_document_links_resolve_locally() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let mut documents = collect_files(&root.join("docs"), "md")?;
    documents.push(root.join("AGENTS.md"));
    documents.push(root.join("CONTEXT.md"));
    documents.push(root.join("CONTRIBUTING.md"));
    documents.push(root.join("GOVERNANCE.md"));
    documents.push(root.join("SECURITY.md"));
    documents.push(root.join("SUPPORT.md"));
    documents.push(root.join("api/README.md"));

    for document in documents {
        let contents = fs::read_to_string(&document)?;
        let parent = document.parent().unwrap_or(&root);
        for target in relative_markdown_links(&contents) {
            let path = target.split('#').next().unwrap_or_default();
            assert!(
                parent.join(path).exists(),
                "{} links to missing path {target}",
                document.display()
            );
        }
    }

    Ok(())
}

#[test]
fn exact_toolchain_is_consistent_across_repository_controls()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let toolchain = fs::read_to_string(root.join("rust-toolchain.toml"))?;
    let workspace = fs::read_to_string(root.join("Cargo.toml"))?;
    let workflow = fs::read_to_string(root.join(".github/workflows/rust-ci.yml"))?;

    assert!(toolchain.contains("channel = \"1.97.1\""));
    assert!(workspace.contains("rust-version = \"1.97.1\""));
    assert!(workflow.contains("RUST_TOOLCHAIN: \"1.97.1\""));
    assert!(workflow.contains("rustup toolchain install 1.97.1"));
    Ok(())
}

#[test]
fn completed_phase_acceptance_is_enforced_by_ci_and_documentation()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/rust-ci.yml"))?;
    let plan = fs::read_to_string(root.join("docs/PROJECT_PLAN.md"))?;
    let version = fs::read_to_string(root.join("VERSION"))?;

    assert!(workflow.contains("Run unit and Phase 4 real-process acceptance tests"));
    assert!(workflow.contains("actions/setup-node@249970729cb0ef3589644e2896645e5dc5ba9c38"));
    assert!(workflow.contains("node-version: 24.18.0"));
    assert!(workflow.contains("actions/setup-python@ece7cb06caefa5fff74198d8649806c4678c61a1"));
    assert!(workflow.contains("python-version: 3.14.6"));
    assert!(workflow.contains("cargo run --release --locked --bin ty -- --version"));
    assert!(workflow.contains("--fail-under-lines 80"));
    assert!(workflow.contains("--fail-under-functions 80"));
    assert!(workflow.contains("--fail-under-regions 80"));
    assert!(workflow.contains(
        "--ignore-filename-regex 'crates/tachyon-core/src/native/(compiler|macos)\\.rs'"
    ));
    assert!(workflow.contains("Phase 3 Chromium island behavior"));
    assert!(workflow.contains("bun run test:rust-browser"));
    assert!(workflow.contains("Phase 4 macOS native and mobile-web parity"));
    assert!(workflow.contains("bun run test:rust-macos"));
    assert!(plan.contains("## Phase 3: Tac and Yon View Semantics"));
    assert!(plan.contains("## Phase 4: Native Vertical Slice"));
    assert!(plan.matches("Status: complete on 2026-07-26").count() >= 4);
    assert_eq!(version.trim(), "26.31.06");
    assert_eq!(version.trim(), tachyon_contracts::PRODUCT_VERSION);
    Ok(())
}
