use codexmanager_core::storage::{Account, Token};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{default_codex_db_path, sync_provider_to_cm, ProviderSyncResult};

const TARGET_PROVIDER: &str = "cm";
const CM_KEY_NAME: &str = "Codex App Gateway";
const CM_KEY_ROTATION_STRATEGY: &str = crate::apikey_profile::ROTATION_HYBRID;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CmConfigResult {
    pub codex_home: PathBuf,
    pub auth_path: PathBuf,
    pub config_path: PathBuf,
    pub selected_account_id: String,
    pub selected_account_label: String,
    pub api_key_id: String,
    pub api_key_created: bool,
    pub auth_updated: bool,
    pub config_updated: bool,
    pub backup_dir: Option<PathBuf>,
    pub provider_sync: ProviderSyncResult,
}

#[derive(Debug, Clone)]
struct SelectedAccount {
    account: Account,
    token: Token,
}

#[derive(Debug, Clone)]
struct SelectedApiKey {
    id: String,
    secret: String,
    created: bool,
}

pub fn configure_cm_for_codex_app() -> Result<CmConfigResult, String> {
    let codex_home = default_codex_home_dir();
    let storage =
        crate::storage_helpers::open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let selected_account = select_login_account(&storage)?;
    let selected_key = select_or_create_api_key(&storage)?;
    configure_cm_for_codex_home(&codex_home, &selected_account, &selected_key)
}

fn configure_cm_for_codex_home(
    codex_home: &Path,
    selected_account: &SelectedAccount,
    selected_key: &SelectedApiKey,
) -> Result<CmConfigResult, String> {
    fs::create_dir_all(codex_home).map_err(|err| err.to_string())?;
    let auth_path = codex_home.join("auth.json");
    let config_path = codex_home.join("config.toml");

    let next_auth = merge_auth_json(
        read_json_object(&auth_path)?,
        &selected_account.token,
        selected_account.account.label.as_str(),
    )?;
    let gateway_base_url = format!(
        "http://{}/v1",
        crate::current_saved_service_addr()
            .trim()
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_end_matches('/')
    );
    let existing_config = fs::read_to_string(&config_path).unwrap_or_default();
    let next_config = upsert_cm_provider_config(
        &existing_config,
        &gateway_base_url,
        selected_key.secret.as_str(),
    );

    let auth_updated = file_json_changed(&auth_path, &next_auth)?;
    let config_updated = fs::read_to_string(&config_path)
        .map(|existing| normalize_eol(&existing) != normalize_eol(&next_config))
        .unwrap_or(true);
    let backup_dir = if auth_updated || config_updated {
        Some(create_config_backup(
            codex_home,
            auth_updated,
            config_updated,
        )?)
    } else {
        None
    };
    if auth_updated {
        fs::write(
            &auth_path,
            serde_json::to_vec_pretty(&next_auth).map_err(|err| err.to_string())?,
        )
        .map_err(|err| err.to_string())?;
    }
    if config_updated {
        fs::write(&config_path, next_config).map_err(|err| err.to_string())?;
    }

    let provider_sync = sync_provider_to_cm(codex_home)?;
    Ok(CmConfigResult {
        codex_home: codex_home.to_path_buf(),
        auth_path,
        config_path,
        selected_account_id: selected_account.account.id.clone(),
        selected_account_label: selected_account.account.label.clone(),
        api_key_id: selected_key.id.clone(),
        api_key_created: selected_key.created,
        auth_updated,
        config_updated,
        backup_dir,
        provider_sync,
    })
}

fn default_codex_home_dir() -> PathBuf {
    default_codex_db_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".codex")
        })
}

fn select_login_account(
    storage: &crate::storage_helpers::StorageHandle,
) -> Result<SelectedAccount, String> {
    let candidates = storage
        .list_gateway_candidates()
        .map_err(|err| format!("list gateway candidates failed: {err}"))?;
    if candidates.is_empty() {
        return Err("没有可用账号，请先登录至少一个可用于网关的 ChatGPT 账号".to_string());
    }
    let preferred = storage
        .preferred_account_id()
        .map_err(|err| format!("read preferred account failed: {err}"))?;
    let (account, token) = preferred
        .as_deref()
        .and_then(|preferred_id| {
            candidates
                .iter()
                .find(|(account, _)| account.id == preferred_id)
                .cloned()
        })
        .unwrap_or_else(|| candidates[0].clone());
    if token.access_token.trim().is_empty()
        && token.id_token.trim().is_empty()
        && token.refresh_token.trim().is_empty()
    {
        return Err("选中的账号缺少 ChatGPT 登录 token".to_string());
    }
    Ok(SelectedAccount { account, token })
}

fn select_or_create_api_key(
    storage: &crate::storage_helpers::StorageHandle,
) -> Result<SelectedApiKey, String> {
    for key in storage
        .list_api_keys()
        .map_err(|err| format!("list api keys failed: {err}"))?
    {
        if key.status == "active" && key.name.as_deref() == Some(CM_KEY_NAME) {
            if let Some(secret) = storage
                .find_api_key_secret_by_id(&key.id)
                .map_err(|err| format!("read api key secret failed: {err}"))?
            {
                if !secret.trim().is_empty() {
                    if key.service_tier.as_deref() == Some("fast") {
                        storage
                            .update_api_key_model_config(
                                &key.id,
                                key.model_slug.as_deref(),
                                key.reasoning_effort.as_deref(),
                                None,
                            )
                            .map_err(|err| format!("reset api key service tier failed: {err}"))?;
                    }
                    if key.rotation_strategy == crate::apikey_profile::ROTATION_ACCOUNT {
                        storage
                            .update_api_key_rotation_config(
                                &key.id,
                                CM_KEY_ROTATION_STRATEGY,
                                key.aggregate_api_id.as_deref(),
                                key.account_plan_filter.as_deref(),
                            )
                            .map_err(|err| {
                                format!("upgrade api key rotation strategy failed: {err}")
                            })?;
                    }
                    return Ok(SelectedApiKey {
                        id: key.id,
                        secret,
                        created: false,
                    });
                }
            }
        }
    }

    let created = crate::apikey_create::create_api_key(
        Some(CM_KEY_NAME.to_string()),
        None,
        None,
        None,
        None,
        None,
        None,
        Some(CM_KEY_ROTATION_STRATEGY.to_string()),
        None,
        None,
        None,
        None,
    )?;
    Ok(SelectedApiKey {
        id: created.id,
        secret: created.key,
        created: true,
    })
}

fn read_json_object(path: &Path) -> Result<Map<String, Value>, String> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let text = fs::read_to_string(path).map_err(|err| err.to_string())?;
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    let value = serde_json::from_str::<Value>(&text)
        .map_err(|err| format!("parse auth.json failed: {err}"))?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| "auth.json must be a JSON object".to_string())
}

fn merge_auth_json(
    mut object: Map<String, Value>,
    token: &Token,
    account_label: &str,
) -> Result<Value, String> {
    object.insert(
        "auth_mode".to_string(),
        Value::String("chatgpt".to_string()),
    );
    object.insert(
        "tokens".to_string(),
        json!({
            "access_token": token.access_token,
            "id_token": token.id_token,
            "refresh_token": token.refresh_token,
            "account_id": token.account_id,
            "account_label": account_label,
            "last_refresh": token.last_refresh,
        }),
    );
    Ok(Value::Object(object))
}

fn file_json_changed(path: &Path, next: &Value) -> Result<bool, String> {
    if !path.exists() {
        return Ok(true);
    }
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(true);
    };
    let Ok(existing) = serde_json::from_str::<Value>(&text) else {
        return Ok(true);
    };
    Ok(existing != *next)
}

fn create_config_backup(
    codex_home: &Path,
    include_auth: bool,
    include_config: bool,
) -> Result<PathBuf, String> {
    let backup_root = codex_home.join("backups_state").join("cm-config");
    let mut backup_dir = backup_root.join(timestamp_name());
    let mut suffix = 0;
    while backup_dir.exists() {
        suffix += 1;
        backup_dir = backup_root.join(format!("{}-{suffix}", timestamp_name()));
    }
    fs::create_dir_all(&backup_dir).map_err(|err| err.to_string())?;
    if include_auth {
        copy_if_exists(&codex_home.join("auth.json"), &backup_dir.join("auth.json"))?;
    }
    if include_config {
        copy_if_exists(
            &codex_home.join("config.toml"),
            &backup_dir.join("config.toml"),
        )?;
    }
    fs::write(
        backup_dir.join("metadata.json"),
        serde_json::to_vec_pretty(
            &json!({"managedBy": "CodexManager cm config", "targetProvider": TARGET_PROVIDER}),
        )
        .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    Ok(backup_dir)
}

fn copy_if_exists(source: &Path, target: &Path) -> Result<(), String> {
    if source.exists() {
        fs::copy(source, target).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn upsert_cm_provider_config(contents: &str, base_url: &str, bearer_token: &str) -> String {
    let mut updated = remove_root_key_if_value(contents, "service_tier", "fast");
    updated = upsert_root_keys(
        &updated,
        &[("model_provider", toml_string(TARGET_PROVIDER))],
    );
    updated = remove_table(&updated, &format!("model_providers.{TARGET_PROVIDER}"));

    let mut lines = updated.lines().map(ToString::to_string).collect::<Vec<_>>();
    let insert_at = first_non_provider_table_index(&lines).unwrap_or(lines.len());
    let provider_lines = vec![
        format!("[model_providers.{TARGET_PROVIDER}]"),
        "name = \"CodexManager\"".to_string(),
        "wire_api = \"responses\"".to_string(),
        "requires_openai_auth = true".to_string(),
        format!("base_url = {}", toml_string(base_url)),
        format!("experimental_bearer_token = {}", toml_string(bearer_token)),
        String::new(),
    ];
    lines.splice(insert_at..insert_at, provider_lines);
    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn upsert_root_keys(contents: &str, entries: &[(&str, String)]) -> String {
    let mut lines = contents
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let root_end = lines
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .unwrap_or(lines.len());

    for (key, value) in entries {
        if let Some(index) = lines[..root_end]
            .iter()
            .position(|line| root_line_key(line) == Some(*key))
        {
            lines[index] = format!("{key} = {value}");
        } else {
            lines.insert(root_end, format!("{key} = {value}"));
        }
    }

    let mut updated = lines.join("\n");
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated
}

fn remove_table(contents: &str, table: &str) -> String {
    let header = format!("[{table}]");
    let mut lines = Vec::new();
    let mut skipping = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if trimmed == header {
                skipping = true;
                continue;
            }
            skipping = false;
        }
        if !skipping {
            lines.push(line.to_string());
        }
    }
    lines.join("\n")
}

fn remove_root_key_if_value(contents: &str, key: &str, value: &str) -> String {
    let mut lines = Vec::new();
    let mut in_root = true;
    for line in contents.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_root = false;
        }
        if in_root && root_line_key(line) == Some(key) {
            let current = line
                .split_once('=')
                .map(|(_, raw)| unquote_toml_string(raw))
                .unwrap_or_default();
            if current.eq_ignore_ascii_case(value) {
                continue;
            }
        }
        lines.push(line.to_string());
    }
    let mut updated = lines.join("\n");
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated
}

fn first_non_provider_table_index(lines: &[String]) -> Option<usize> {
    lines.iter().position(|line| {
        let trimmed = line.trim();
        trimmed.starts_with('[') && !trimmed.starts_with("[model_providers.")
    })
}

fn root_line_key(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') || trimmed.starts_with('[') {
        return None;
    }
    trimmed.split_once('=').map(|(key, _)| key.trim())
}

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn unquote_toml_string(value: &str) -> String {
    let value = value.trim();
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .trim()
        .to_string()
}

fn normalize_eol(value: &str) -> String {
    value.replace("\r\n", "\n")
}

fn timestamp_name() -> String {
    chrono::Local::now().format("%Y%m%d%H%M%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_auth_json_preserves_existing_openai_api_key() {
        let mut existing = Map::new();
        existing.insert(
            "OPENAI_API_KEY".to_string(),
            Value::String("platform-key".to_string()),
        );
        let token = Token {
            account_id: "acc-1".to_string(),
            id_token: "id-token".to_string(),
            access_token: "access-token".to_string(),
            refresh_token: "refresh-token".to_string(),
            api_key_access_token: None,
            last_refresh: 123,
        };

        let merged = merge_auth_json(existing, &token, "Main").expect("merge");

        assert_eq!(merged["OPENAI_API_KEY"], "platform-key");
        assert_eq!(merged["auth_mode"], "chatgpt");
        assert_eq!(merged["tokens"]["access_token"], "access-token");
        assert_eq!(merged["tokens"]["refresh_token"], "refresh-token");
    }

    #[test]
    fn upsert_cm_provider_preserves_other_providers() {
        let before = r#"model_provider = "openai"

[model_providers.openai]
name = "OpenAI"
base_url = "https://api.openai.com/v1"

[profiles.work]
model = "gpt-5"
"#;

        let after = upsert_cm_provider_config(before, "http://localhost:48760/v1", "cm-key");

        assert!(after.contains("model_provider = \"cm\""));
        assert!(!after.contains("service_tier = \"fast\""));
        assert!(after.contains("[model_providers.cm]"));
        assert!(after.contains("requires_openai_auth = true"));
        assert!(after.contains("experimental_bearer_token = \"cm-key\""));
        assert!(after.contains("[model_providers.openai]"));
        assert!(after.contains("[profiles.work]"));
    }

    #[test]
    fn upsert_cm_provider_replaces_existing_cm_section() {
        let before = r#"[model_providers.cm]
name = "old"
experimental_bearer_token = "old"

[model_providers.other]
name = "Other"
"#;

        let after = upsert_cm_provider_config(before, "http://localhost:48760/v1", "new");

        assert_eq!(after.matches("[model_providers.cm]").count(), 1);
        assert!(!after.contains("experimental_bearer_token = \"old\""));
        assert!(after.contains("experimental_bearer_token = \"new\""));
        assert!(after.contains("[model_providers.other]"));
    }

    #[test]
    fn upsert_cm_provider_removes_previous_fast_default() {
        let before = r#"model_provider = "cm"
service_tier = "fast"

[profiles.work]
model = "gpt-5"
"#;

        let after = upsert_cm_provider_config(before, "http://localhost:48760/v1", "cm-key");

        assert!(!after.contains("service_tier = \"fast\""));
        assert!(after.contains("[profiles.work]"));
    }

    #[test]
    fn select_or_create_api_key_creates_hybrid_rotation_key() {
        let _guard = crate::test_env_guard();
        let db_path = setup_test_storage("cm-config-new-hybrid");
        let storage = crate::storage_helpers::open_storage().expect("open storage");

        let selected = select_or_create_api_key(&storage).expect("select key");

        assert!(selected.created);
        let key = storage
            .find_api_key_by_id(&selected.id)
            .expect("read key")
            .expect("key exists");
        assert_eq!(key.rotation_strategy, crate::apikey_profile::ROTATION_HYBRID);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn select_or_create_api_key_upgrades_account_rotation_key() {
        let _guard = crate::test_env_guard();
        let db_path = setup_test_storage("cm-config-upgrade-account");
        let storage = crate::storage_helpers::open_storage().expect("open storage");
        let account_key = insert_cm_key(&storage, crate::apikey_profile::ROTATION_ACCOUNT);

        let selected = select_or_create_api_key(&storage).expect("select key");

        assert!(!selected.created);
        assert_eq!(selected.id, account_key.id);
        let account_after = storage
            .find_api_key_by_id(&account_key.id)
            .expect("read account key")
            .expect("account key exists");
        assert_eq!(
            account_after.rotation_strategy,
            crate::apikey_profile::ROTATION_HYBRID
        );
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn select_or_create_api_key_keeps_hybrid_rotation_key() {
        let _guard = crate::test_env_guard();
        let db_path = setup_test_storage("cm-config-keep-hybrid");
        let storage = crate::storage_helpers::open_storage().expect("open storage");
        let hybrid_key = insert_cm_key(&storage, crate::apikey_profile::ROTATION_HYBRID);

        let selected = select_or_create_api_key(&storage).expect("select key");

        assert!(!selected.created);
        assert_eq!(selected.id, hybrid_key.id);
        let hybrid_after = storage
            .find_api_key_by_id(&hybrid_key.id)
            .expect("read hybrid key")
            .expect("hybrid key exists");
        assert_eq!(
            hybrid_after.rotation_strategy,
            crate::apikey_profile::ROTATION_HYBRID
        );
        let _ = std::fs::remove_file(db_path);
    }

    #[derive(Debug, Clone)]
    struct InsertedKey {
        id: String,
    }

    fn setup_test_storage(name: &str) -> String {
        let db_path = std::env::temp_dir()
            .join(format!(
                "{name}-{}-{}.sqlite",
                std::process::id(),
                codexmanager_core::storage::now_ts()
            ))
            .to_string_lossy()
            .to_string();
        let _ = std::fs::remove_file(&db_path);
        std::env::set_var("CODEXMANAGER_DB_PATH", &db_path);
        crate::storage_helpers::initialize_storage().expect("init storage");
        db_path
    }

    fn insert_cm_key(
        storage: &crate::storage_helpers::StorageHandle,
        rotation_strategy: &str,
    ) -> InsertedKey {
        use codexmanager_core::storage::{now_ts, ApiKey};

        let id = crate::storage_helpers::generate_key_id();
        let secret = crate::storage_helpers::generate_platform_key();
        let record = ApiKey {
            id: id.clone(),
            name: Some(CM_KEY_NAME.to_string()),
            model_slug: None,
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: rotation_strategy.to_string(),
            aggregate_api_id: None,
            account_plan_filter: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: crate::apikey_profile::PROTOCOL_OPENAI_COMPAT.to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            key_hash: crate::storage_helpers::hash_platform_key(&secret),
            status: "active".to_string(),
            created_at: now_ts(),
            last_used_at: None,
        };
        storage.insert_api_key(&record).expect("insert key");
        storage
            .upsert_api_key_secret(&id, &secret)
            .expect("insert key secret");
        InsertedKey { id }
    }
}
