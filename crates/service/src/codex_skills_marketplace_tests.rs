use super::*;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "codexmanager-marketplace-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct StubRunner {
    responses: Mutex<VecDeque<Result<String, CliRunError>>>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl StubRunner {
    fn with_json(values: Vec<Value>) -> Self {
        Self {
            responses: Mutex::new(
                values
                    .into_iter()
                    .map(|value| Ok(serde_json::to_string(&value).expect("serialize stub JSON")))
                    .collect(),
            ),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().expect("calls lock").clone()
    }
}

impl CliRunner for StubRunner {
    fn run(&self, args: &[String], _codex_home: &Path) -> Result<CliOutput, CliRunError> {
        self.calls.lock().expect("calls lock").push(args.to_vec());
        let response = self
            .responses
            .lock()
            .expect("responses lock")
            .pop_front()
            .expect("stub response");
        response.map(|stdout| CliOutput { stdout })
    }
}

fn write_plugin(
    root: &Path,
    directory: &str,
    manifest_name: &str,
    version: &str,
    skill_name: Option<&str>,
    skill_description: Option<&str>,
) -> PathBuf {
    let plugin = root.join("plugins").join(directory);
    fs::create_dir_all(plugin.join(".codex-plugin")).expect("create manifest directory");
    fs::create_dir_all(plugin.join(".claude-plugin")).expect("create Claude manifest directory");
    fs::write(
        plugin.join(".claude-plugin").join("plugin.json"),
        r#"{"name":"claude-copy"}"#,
    )
    .expect("write Claude manifest");
    fs::write(
        plugin.join(".codex-plugin").join("plugin.json"),
        serde_json::to_vec(&serde_json::json!({
            "name": manifest_name,
            "version": version,
            "description": format!("{manifest_name} description"),
            "author": { "name": "CodexManager tests" },
            "skills": "./skills/",
            "interface": { "category": "Productivity" }
        }))
        .expect("serialize manifest"),
    )
    .expect("write Codex manifest");
    if let Some(skill_name) = skill_name {
        let skill = plugin.join("skills").join(skill_name);
        fs::create_dir_all(&skill).expect("create skill directory");
        let description_line = skill_description
            .map(|description| format!("description: {description}\n"))
            .unwrap_or_default();
        fs::write(
            skill.join("SKILL.md"),
            format!("---\nname: {skill_name}\n{description_line}---\n\n# Test\n"),
        )
        .expect("write SKILL.md");
    }
    plugin
}

fn marketplace_json(name: &str, root: &Path) -> Value {
    serde_json::json!({
        "marketplaces": [{
            "name": name,
            "root": root,
            "marketplaceSource": {
                "sourceType": "git",
                "source": "https://github.com/example/skills.git"
            }
        }]
    })
}

fn plugin_entry(
    name: &str,
    marketplace: &str,
    version: &str,
    path: &Path,
    installed: bool,
) -> Value {
    serde_json::json!({
        "pluginId": format!("{name}@{marketplace}"),
        "name": name,
        "marketplaceName": marketplace,
        "version": version,
        "installed": installed,
        "enabled": installed,
        "source": { "source": "local", "path": path },
        "installPolicy": "AVAILABLE",
        "authPolicy": "ON_USE"
    })
}

#[test]
fn github_source_and_ref_validation_is_strict() {
    assert_eq!(
        normalize_github_source("openai/role-specific-plugins").unwrap(),
        "https://github.com/openai/role-specific-plugins.git"
    );
    assert_eq!(
        normalize_github_source("https://github.com/openai/role-specific-plugins.git").unwrap(),
        "https://github.com/openai/role-specific-plugins.git"
    );
    for rejected in [
        "git@github.com:openai/repo.git",
        "http://github.com/openai/repo",
        "https://github.example/openai/repo",
        "https://github.com/openai/repo/tree/main",
        "https://token@github.com/openai/repo",
        "../repo",
        "openai/repo@main",
    ] {
        assert!(
            normalize_github_source(rejected).is_err(),
            "accepted {rejected}"
        );
    }

    assert_eq!(
        normalize_ref_name(Some("release/2026.07")).unwrap(),
        Some("release/2026.07".to_string())
    );
    for rejected in ["--help", "../main", "main..next", "refs/@{upstream}", "a b"] {
        assert!(
            normalize_ref_name(Some(rejected)).is_err(),
            "accepted {rejected}"
        );
    }
}

#[test]
fn inventory_keeps_only_local_standard_codex_skill_plugins() {
    let temp = TestDirectory::new("filter");
    let marketplace_root = temp.path().join("marketplace");
    fs::create_dir_all(&marketplace_root).expect("create marketplace");
    let good = write_plugin(
        &marketplace_root,
        "good-plugin",
        "good-plugin",
        "1.2.3",
        Some("good-skill"),
        Some("Use this standard Codex skill for tests."),
    );
    let missing_description = write_plugin(
        &marketplace_root,
        "missing-description",
        "missing-description",
        "1.0.0",
        Some("missing-description"),
        None,
    );
    let claude_only = marketplace_root.join("plugins").join("claude-only");
    fs::create_dir_all(claude_only.join(".claude-plugin")).expect("create Claude plugin");
    fs::write(
        claude_only.join(".claude-plugin").join("plugin.json"),
        r#"{"name":"claude-only"}"#,
    )
    .expect("write Claude manifest");
    let outside_root = temp.path().join("outside");
    let outside = write_plugin(
        &outside_root,
        "outside-plugin",
        "outside-plugin",
        "1.0.0",
        Some("outside-skill"),
        Some("This source is outside the marketplace root."),
    );
    let plugins = serde_json::json!({
        "installed": [],
        "available": [
            plugin_entry("good-plugin", "test-market", "1.2.3", &good, false),
            plugin_entry(
                "missing-description",
                "test-market",
                "1.0.0",
                &missing_description,
                false
            ),
            plugin_entry("claude-only", "test-market", "1.0.0", &claude_only, false),
            plugin_entry("outside-plugin", "test-market", "1.0.0", &outside, false),
            {
                "pluginId": "remote@test-market",
                "name": "remote",
                "marketplaceName": "test-market",
                "version": "1.0.0",
                "installed": false,
                "enabled": false,
                "source": { "source": "url", "url": "https://example.com/plugin.zip" },
                "installPolicy": "AVAILABLE"
            }
        ]
    });
    let runner = StubRunner::with_json(vec![
        marketplace_json("test-market", &marketplace_root),
        plugins,
    ]);

    let inventory = list_with_runner(temp.path(), &runner).expect("list inventory");

    assert!(inventory.cli_available);
    assert_eq!(inventory.marketplaces.len(), 1);
    assert_eq!(inventory.plugins.len(), 1);
    let plugin = &inventory.plugins[0];
    assert_eq!(plugin.plugin_id, "good-plugin@test-market");
    assert_eq!(plugin.author, "CodexManager tests");
    assert_eq!(plugin.category, "Productivity");
    assert_eq!(plugin.skills.len(), 1);
    assert_eq!(plugin.skills[0].name, "good-skill");
}

#[test]
fn available_plugin_rejects_a_cli_revision_that_differs_from_the_manifest_version() {
    let temp = TestDirectory::new("available-version-mismatch");
    let marketplace_root = temp.path().join("marketplace");
    fs::create_dir_all(&marketplace_root).expect("create marketplace");
    let plugin_path = write_plugin(
        &marketplace_root,
        "version-mismatch",
        "version-mismatch",
        "1.2.3",
        Some("version-mismatch-skill"),
        Some("Use this standard Codex skill for a version mismatch test."),
    );
    let plugins = serde_json::json!({
        "installed": [],
        "available": [plugin_entry(
            "version-mismatch",
            "test-market",
            "marketplace-snapshot-revision",
            &plugin_path,
            false
        )]
    });
    let runner = StubRunner::with_json(vec![
        marketplace_json("test-market", &marketplace_root),
        plugins,
    ]);

    let inventory = list_with_runner(temp.path(), &runner).expect("list inventory");

    assert!(inventory.plugins.is_empty());
}

#[test]
fn unavailable_or_old_cli_returns_an_explicit_inventory_warning() {
    let runner = StubRunner {
        responses: Mutex::new(VecDeque::from([Err(CliRunError::Unavailable(
            "Codex CLI was not found on PATH".to_string(),
        ))])),
        calls: Mutex::new(Vec::new()),
    };

    let inventory = list_with_runner(Path::new("/tmp/test-codex-home"), &runner)
        .expect("unavailable inventory");

    assert!(!inventory.cli_available);
    assert!(inventory.plugins.is_empty());
    assert_eq!(inventory.warnings, ["Codex CLI was not found on PATH"]);
}

#[test]
fn install_accepts_an_installed_cache_revision_and_keeps_the_manifest_version() {
    let temp = TestDirectory::new("install");
    let marketplace_root = temp.path().join("marketplace");
    fs::create_dir_all(&marketplace_root).expect("create marketplace");
    let plugin_path = write_plugin(
        &marketplace_root,
        "installable-plugin",
        "installable-plugin",
        "2.0.0",
        Some("installable-skill"),
        Some("Install this complete Codex plugin package."),
    );
    let before = serde_json::json!({
        "installed": [],
        "available": [plugin_entry(
            "installable-plugin",
            "test-market",
            "2.0.0",
            &plugin_path,
            false
        )]
    });
    let after = serde_json::json!({
        "installed": [plugin_entry(
            "installable-plugin",
            "test-market",
            "marketplace-snapshot-revision",
            &plugin_path,
            true
        )],
        "available": []
    });
    let marketplace = marketplace_json("test-market", &marketplace_root);
    let runner = StubRunner::with_json(vec![
        marketplace.clone(),
        before,
        serde_json::json!({ "pluginId": "installable-plugin@test-market" }),
        marketplace,
        after,
    ]);

    let inventory = install_with_runner("installable-plugin@test-market", temp.path(), &runner)
        .expect("install plugin");

    assert!(inventory.plugins[0].installed);
    assert_eq!(inventory.plugins[0].version, "2.0.0");
    assert_eq!(
        runner.calls()[2],
        strings(&[
            "plugin",
            "add",
            "--json",
            "--",
            "installable-plugin@test-market"
        ])
    );
}

#[test]
fn bounded_reader_drains_input_and_marks_truncation() {
    let captured = read_stream_bounded(std::io::Cursor::new(vec![b'x'; 128]), 16)
        .expect("read captured stream");
    assert_eq!(captured.bytes, vec![b'x'; 16]);
    assert!(captured.truncated);
}

#[test]
fn standard_skill_parser_requires_name_and_description() {
    assert_eq!(
        parse_standard_skill_metadata(
            "---\nname: useful-skill\ndescription: Use this when useful.\n---\n# Body\n"
        ),
        Some(CodexMarketplaceSkillSummary {
            name: "useful-skill".to_string(),
            description: "Use this when useful.".to_string(),
        })
    );
    assert!(parse_standard_skill_metadata("---\nname: useful-skill\n---\n").is_none());
    assert!(parse_standard_skill_metadata(
        "---\ndescription: Missing the required Codex skill name.\n---\n"
    )
    .is_none());
    assert!(parse_standard_skill_metadata(
        "---\nmetadata:\n  name: nested-name\n  description: Nested values are not top-level fields.\n---\n"
    )
    .is_none());
    assert!(parse_standard_skill_metadata(
        "---\nname: useful-skill\nname: duplicate-name\ndescription: Duplicate keys are ambiguous.\n---\n"
    )
    .is_none());
    assert!(parse_standard_skill_metadata(
        "---\nname: useful-skill\ndescription: First description.\ndescription: Second description.\n---\n"
    )
    .is_none());
    assert!(parse_standard_skill_metadata(&format!(
        "---\nname: useful-skill\ndescription: {}\n---\n",
        "x".repeat(1025)
    ))
    .is_none());
}

#[test]
fn codex_home_must_be_absolute() {
    assert!(resolve_absolute_codex_home(Some("relative/codex-home")).is_err());
}
