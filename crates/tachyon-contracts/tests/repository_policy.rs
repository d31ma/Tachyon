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
    let mut links = Vec::new();
    let mut fence = None;
    for line in contents.lines() {
        if fenced_code_line(line, &mut fence) {
            continue;
        }
        collect_line_links(line, &mut links);
    }
    links
}

fn fenced_code_line(line: &str, fence: &mut Option<(char, usize)>) -> bool {
    let text = line.trim_start_matches(' ');
    if line.len() - text.len() > 3 || text.starts_with('\t') {
        return fence.is_some();
    }
    let marker = text.chars().next().unwrap_or(' ');
    let length = text.chars().take_while(|value| *value == marker).count();
    let is_marker = matches!(marker, '`' | '~') && length >= 3;
    if let Some((opened, minimum)) = *fence {
        if is_marker && marker == opened && length >= minimum && text[length..].trim().is_empty() {
            *fence = None;
        }
        return true;
    }
    if !is_marker || (marker == '`' && text[length..].contains('`')) {
        return false;
    }
    *fence = Some((marker, length));
    true
}

fn collect_line_links<'a>(line: &'a str, links: &mut Vec<&'a str>) {
    for tail in line.split("](").skip(1) {
        let Some(target) = tail.split(')').next() else {
            continue;
        };
        let target = target.trim();
        if target.is_empty()
            || target.starts_with('#')
            || target.contains("://")
            || target.starts_with("mailto:")
        {
            continue;
        }
        links.push(target);
    }
}

#[test]
fn document_links_ignore_fenced_code_without_hiding_real_links() {
    let source = "[before](before.md)\n```go\nBehaviorsProperties[CatalogSettings](application)\n```\n[after](after.md)\n~~~~text\n[example](not-a-link)\n~~~\n[still code](not-a-link-either)\n~~~~\n[final](final.md)\n";
    assert_eq!(
        relative_markdown_links(source),
        ["before.md", "after.md", "final.md"]
    );
}

#[test]
fn document_links_accept_up_to_three_spaces_before_fences() {
    for width in 0..=3 {
        let indent = " ".repeat(width);
        for marker in ["```", "~~~"] {
            let source = format!(
                "[before](before.md)\n{indent}{marker}go\n[code](ignored)\n{indent}{marker}\n[after](after.md)\n"
            );
            assert_eq!(relative_markdown_links(&source), ["before.md", "after.md"]);
        }
    }
}

#[test]
fn document_links_do_not_open_indented_or_invalid_backtick_fences() {
    for opening in ["    ```go", "\t```go", "   \t```go", "```go`value"] {
        let source = format!("{opening}\n\n[real](real.md)\n");
        assert_eq!(relative_markdown_links(&source), ["real.md"], "{opening:?}");
    }
}

#[test]
fn document_links_do_not_close_on_indented_fences() {
    for indent in ["    ", "\t", "   \t"] {
        let source = format!("```go\n{indent}```\n[still code](ignored)\n```\n[after](after.md)\n");
        assert_eq!(relative_markdown_links(&source), ["after.md"], "{indent:?}");
    }
}

#[test]
fn document_links_allow_backticks_in_tilde_fence_info() {
    let source = "~~~`go`\n[code](ignored)\n~~~\n[after](after.md)\n";
    assert_eq!(relative_markdown_links(source), ["after.md"]);
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
        "yon_annotations",
        "application_config",
        "managed_resource_headers",
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
    assert!(workflow.contains(
        "for target in html_frontend template_frontend handler_frame native_planner yon_annotations application_config managed_resource_headers"
    ));
    assert!(workflow.contains("-max_total_time=${FUZZ_SECONDS}"));
    assert!(scheduled.contains("name: Fuzz ${{ matrix.target }}"));
    assert!(scheduled.contains("target: [html_frontend, template_frontend, handler_frame, native_planner, yon_annotations, application_config, managed_resource_headers]"));
    assert!(scheduled.contains("path: fuzz/corpus/${{ matrix.target }}"));
    assert!(scheduled.contains("actions/cache/restore@caa296126883cff596d87d8935842f9db880ef25"));
    assert!(scheduled.contains("actions/cache/save@caa296126883cff596d87d8935842f9db880ef25"));
    assert!(
        scheduled
            .contains("fuzz-corpus-${{ runner.os }}-${{ matrix.target }}-${{ github.run_id }}")
    );
    Ok(())
}

#[test]
fn architecture_adrs_have_final_statuses_and_are_numbered() -> Result<(), Box<dyn std::error::Error>>
{
    let root = repository_root();
    let adrs = collect_files(&root.join("docs/adr"), "md")?;
    let numbered = adrs
        .iter()
        .filter(|path| is_numbered_adr(path))
        .collect::<Vec<_>>();

    assert!(!numbered.is_empty());
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
                .any(|line| line.contains("Accepted") || line.contains("Superseded")),
            "{} has no final status",
            adr.display()
        );
    }

    Ok(())
}

fn is_numbered_adr(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let bytes = name.as_bytes();
    bytes.len() > 5 && bytes[..4].iter().all(u8::is_ascii_digit) && bytes[4] == b'-'
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
        assert_pinned_actions(&root, &workflow, &contents);
    }
    Ok(())
}

fn assert_pinned_actions(root: &Path, workflow: &Path, contents: &str) {
    for line in contents.lines() {
        let Some(action) = line.trim().strip_prefix("uses: ") else {
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

#[test]
fn project_environment_variables_use_domain_namespaces() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let mut files = Vec::new();
    for directory in [".github", "crates", "scripts", "website/tests"] {
        for extension in ["js", "mjs", "ps1", "rs", "sh", "ts", "yaml", "yml"] {
            files.extend(collect_files(&root.join(directory), extension)?);
        }
    }

    let old_namespace = ["TACH", "YON_"].concat();
    let removed_data_namespace = ["FY", "LO_"].concat();
    let removed_packaging_name = ["YON", "_DIST_PATH"].concat();
    let forbidden = [
        format!("process.env.{old_namespace}"),
        format!("std::env::var(\"{old_namespace}"),
        format!("std::env::var_os(\"{old_namespace}"),
        format!(".env(\"{old_namespace}"),
        format!(".env_remove(\"{old_namespace}"),
        format!("env!(\"{old_namespace}"),
        format!("cargo:rustc-env={old_namespace}"),
        format!("$env:{old_namespace}"),
        format!("${{{old_namespace}"),
    ];
    let mut violations = Vec::new();
    for file in files {
        if file
            .components()
            .any(|component| component.as_os_str() == "fixtures")
        {
            continue;
        }
        let contents = fs::read_to_string(&file)?;
        for (index, line) in contents.lines().enumerate() {
            let workflow_environment = line
                .trim_start()
                .strip_prefix(&old_namespace)
                .is_some_and(|value| value.contains(':'));
            if workflow_environment
                || forbidden.iter().any(|pattern| line.contains(pattern))
                || line.contains(&removed_data_namespace)
                || line.contains(&removed_packaging_name)
            {
                violations.push(format!("{}:{}", file.display(), index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "project environment variables must start with TAC_ or YON_, and removed namespaces must not return: {violations:?}"
    );
    Ok(())
}

#[test]
fn legacy_framework_tree_cannot_return() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    for relative in [
        "src",
        "tests",
        "scripts/bench",
        "scripts/install-vendor-bins.sh",
        "scripts/typecheck.js",
        "scripts/windows-native-compile-smoke.js",
        "bunfig.toml",
        "tsconfig.json",
        "tsconfig.base.json",
        "tsconfig.src.json",
        "tsconfig.tests.json",
        "tsconfig.website.json",
    ] {
        assert!(
            !root.join(relative).exists(),
            "legacy implementation path returned: {relative}"
        );
    }

    let package = fs::read_to_string(root.join("package.json"))?;
    for legacy_command in ["src/cli/", "bun test ./tests", "tests/.env.test"] {
        assert!(
            !package.contains(legacy_command),
            "legacy package command returned: {legacy_command}"
        );
    }
    assert!(root.join("types/tachyon-env.d.ts").is_file());
    assert!(
        root.join("crates/tachyon-cli/tests/fixtures/migration-fullstack")
            .is_dir()
    );

    let workflow = fs::read_to_string(root.join(".github/workflows/rust-ci.yml"))?;
    assert!(workflow.contains("Rewrite-only repository policy"));
    assert!(workflow.contains("TAC_LEGACY_BIN="));
    assert!(workflow.contains("crates/tachyon-cli/tests/fixtures/migration-fullstack"));
    Ok(())
}

#[test]
fn the_clients_folder_is_named_only_where_it_stays_an_alias()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let allowed = [
        "crates/tachyon-core/src/annotation_layout.rs",
        "crates/tachyon-core/src/migrate.rs",
        "crates/tachyon-core/src/migrate/fix/rename_tests.rs",
        "crates/tachyon-core/src/migrate/fix/route_tests.rs",
        "crates/tachyon-core/src/project.rs",
        "crates/tachyon-core/src/project/captured_sources.rs",
        "crates/tachyon-core/src/template/service.rs",
    ];
    let mut files = Vec::new();
    for folder in [
        "crates/tachyon-core/src",
        "crates/tachyon-core/tests/it",
        "crates/tachyon-cli/src",
        "crates/tachyon-cli/tests",
    ] {
        files.extend(collect_files(&root.join(folder), "rs")?);
    }
    files.extend(collect_files(&root.join("scripts"), "mjs")?);
    let mut stale = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(&root)?
            .to_string_lossy()
            .replace('\\', "/");
        let contents = fs::read_to_string(&file)?;
        let names_old_folder =
            contents.contains("client/clients") || contents.contains("server/clients");
        if names_old_folder && !allowed.contains(&relative.as_str()) {
            stale.push(relative);
        }
    }
    assert!(
        stale.is_empty(),
        "outbound clients live in client/remotes or server/remotes (ADR 0088); clients/ stays only as an alias: {stale:?}"
    );
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
    assert!(!workflow.contains("environment: packages-prod"));
    assert!(workflow.contains("needs.verify-release.result == 'success'"));

    assert!(unix_installer.contains("No checksum published for ${asset}. Aborting."));
    assert!(unix_installer.contains("mktemp \"${dest}/.ty.download.XXXXXX\""));
    assert!(unix_installer.contains("chmod 0755 \"$tmp\""));
    assert!(unix_installer.contains("mv \"$tmp\" \"$dest/ty\""));
    assert!(!unix_installer.contains("mktemp -d"));
    assert!(windows_installer.contains("No checksum published for $asset. Aborting."));
    assert!(windows_installer.contains("[Text.Encoding]::UTF8.GetString($sumsContent)"));
    assert!(!windows_installer.contains("Could not verify checksum"));
    assert!(windows_installer.contains("[IO.File]::Replace($download, $exe, $backup)"));
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
fn ci_runs_on_the_develop_integration_branch() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let rust = fs::read_to_string(root.join(".github/workflows/rust-ci.yml"))?;
    let ruby = fs::read_to_string(root.join(".github/workflows/ruby-runtime.yml"))?;

    assert!(rust.contains("branches: [main, develop, codex/rust-rewrite]"));
    assert!(rust.contains("branches: [develop, codex/rust-rewrite, \"work/**\"]"));
    assert!(ruby.contains("branches: [main, develop, codex/rust-rewrite]"));
    Ok(())
}

#[test]
fn self_hosted_runners_serve_only_opted_in_branch_pushes() -> Result<(), Box<dyn std::error::Error>>
{
    let root = repository_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/rust-ci.yml"))?;
    let guard = "github.event_name == 'push' && startsWith(github.ref, 'refs/heads/') && vars.TACHYON_SELF_HOSTED == 'true'";
    let hosted_fallbacks = [
        "|| 'ubuntu-latest' }}",
        "|| 'macos-latest' }}",
        "|| 'windows-latest' }}",
        "|| matrix.os }}",
    ];

    let runner_lines = workflow
        .lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("runs-on:"))
        .collect::<Vec<_>>();
    assert!(!runner_lines.is_empty());
    for line in runner_lines {
        assert!(
            line.contains(guard),
            "every Rust CI job chooses a self-hosted runner only behind `{guard}`: {line}"
        );
        assert!(
            hosted_fallbacks
                .iter()
                .any(|fallback| line.ends_with(fallback)),
            "every Rust CI job falls back to a GitHub-hosted runner: {line}"
        );
    }

    for line in workflow
        .lines()
        .map(str::trim_start)
        .filter(|line| line.contains("self-hosted"))
    {
        let guarded_choice = line.starts_with("runs-on:") && line.contains(guard);
        let matrix_labels = line.starts_with("self_hosted_labels: '[\"self-hosted\",\"tachyon\",");
        assert!(
            guarded_choice || matrix_labels,
            "self-hosted labels appear only in guarded runner choices and matrix label entries: {line}"
        );
    }
    assert!(workflow.contains("vars.TACHYON_SELF_HOSTED_ANDROID == 'true'"));
    assert!(!workflow.contains("pull_request_target"));

    for other in collect_files(&root.join(".github/workflows"), "yml")? {
        if other.ends_with("rust-ci.yml") {
            continue;
        }
        let contents = fs::read_to_string(&other)?;
        assert!(
            !contents.contains("self-hosted"),
            "{} must stay on GitHub-hosted runners",
            other.display()
        );
    }
    Ok(())
}

#[test]
fn ci_runs_the_browser_gates_that_exist() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/rust-ci.yml"))?;

    for step in [
        "bun run test:cached-client-browser",
        "bun run test:authored-node-browser",
        "bun run test:svg-namespace-browser",
        "bun run test:logic-controls-browser",
        "bun run test:scaffold-browser",
        "node website/scripts/build-styles.mjs --check",
        "node website/scripts/language-coverage.mjs --strict",
        "node website/scripts/check-responsive.mjs",
        "node scripts/ignored-browser-gates.mjs --groups=dotnet,kotlin,java",
        "TAC_KOTLIN_WEB_REQUIRE_TOOLCHAINS: \"1\"",
    ] {
        assert!(workflow.contains(step), "CI does not run `{step}`");
    }
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
    assert!(workflow.contains("actions/setup-node@820762786026740c76f36085b0efc47a31fe5020"));
    assert!(workflow.contains("node-version: 24.18.0"));
    assert!(workflow.contains("actions/setup-python@5fda3b95a4ea91299a34e894583c3862153e4b97"));
    assert!(workflow.contains("python-version: 3.14.6"));
    assert!(workflow.contains("ruby/setup-ruby@14594264cd68ce8a2345dd349bc3d138a4ef85c8"));
    assert!(workflow.contains("ruby-version: \"4.0.6\""));
    assert!(workflow.contains("cargo run --release --locked --bin ty -- --version"));
    assert!(workflow.contains("--fail-under-lines 80"));
    assert!(workflow.contains("--fail-under-functions 80"));
    assert!(workflow.contains("--fail-under-regions 80"));
    assert!(workflow.contains(
        "--ignore-filename-regex 'crates/tachyon-core/src/native/(compiler|macos)\\.rs'"
    ));
    assert!(workflow.contains("Phase 3 Chromium component and service behavior"));
    assert!(workflow.contains("bun run test:phase3-browser"));
    assert!(workflow.contains("Phase 4 macOS native and mobile-web parity"));
    assert!(workflow.contains("bun run test:phase4-macos"));
    assert!(plan.contains("## Phase 3: Tac View Semantics"));
    assert!(plan.contains("## Phase 4: Native Vertical Slice"));
    assert!(plan.matches("Status: complete on 2026-07-26").count() >= 4);
    assert_eq!(version.trim(), "26.36.01");
    assert_eq!(version.trim(), tachyon_contracts::PRODUCT_VERSION);
    Ok(())
}

#[test]
fn cohesion_violations_cannot_grow_above_the_baseline() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let baseline: usize = fs::read_to_string(root.join("tools/cohesion-scan/BASELINE"))?
        .trim()
        .parse()?;
    let report = cohesion_scan::Scan::repository(&root)?;
    assert!(
        report.total <= baseline,
        "cohesion violations grew from {baseline} to {}; run `cargo run -p cohesion-scan` to list them\n{}",
        report.total,
        report.table()
    );
    Ok(())
}

#[test]
fn duplicated_blocks_cannot_grow_above_the_baseline() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let baseline: usize =
        fs::read_to_string(root.join("tools/cohesion-scan/DUPLICATION_BASELINE"))?
            .trim()
            .parse()?;
    let report = cohesion_scan::Scan::repository(&root)?;
    assert!(
        report.duplication_total <= baseline,
        "duplicated blocks grew from {baseline} to {}; move the repeated code into one static method of a utility type (ADR 0064) and run `cargo run -p cohesion-scan` to list them\n{}",
        report.duplication_total,
        report.duplicates.join("\n")
    );
    Ok(())
}

#[test]
fn structure_violations_cannot_grow_above_the_baseline() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let baseline: usize = fs::read_to_string(root.join("tools/cohesion-scan/STRUCTURE_BASELINE"))?
        .trim()
        .parse()?;
    let report = cohesion_scan::Scan::repository(&root)?;
    assert!(
        report.structure_total <= baseline,
        "files with more than one type, free functions or file-level values grew from {baseline} to {}; run `cargo run -p cohesion-scan` to list them\n{}",
        report.structure_total,
        report.table()
    );
    Ok(())
}

#[test]
fn file_name_violations_cannot_grow_above_the_baseline() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let baseline: usize = fs::read_to_string(root.join("tools/cohesion-scan/FILE_NAME_BASELINE"))?
        .trim()
        .parse()?;
    let report = cohesion_scan::Scan::repository(&root)?;
    assert!(
        report.file_name_total <= baseline,
        "files not named after their one type grew from {baseline} to {}; name each Rust file after its type in snake_case (ADR 0067) and run `cargo run -p cohesion-scan` to list them\n{}",
        report.file_name_total,
        report.table()
    );
    Ok(())
}
