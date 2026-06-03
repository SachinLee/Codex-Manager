use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const FEATURE_TABLE: &str = "local_app_server_feature_enablement";
const REMOTE_CONTROL_FEATURE: &str = "remote_control";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAppBridgeStatus {
    pub codex_home: PathBuf,
    pub auth_path: PathBuf,
    pub config_path: PathBuf,
    pub app_db_path: PathBuf,
    pub auth_mode_chatgpt: bool,
    pub has_access_token: bool,
    pub has_id_token: bool,
    pub has_refresh_token: bool,
    pub last_refresh: Option<i64>,
    pub provider: Option<String>,
    pub provider_base_url: Option<String>,
    pub provider_wire_api: Option<String>,
    pub provider_requires_openai_auth: bool,
    pub provider_is_cm: bool,
    pub remote_connections_enabled: bool,
    pub legacy_remote_control_present: bool,
    pub db_exists: bool,
    pub db_table_exists: bool,
    pub db_remote_control_enabled: bool,
    pub db_updated_at: Option<i64>,
    pub log_enablement_seen: bool,
    pub desktop_sign_in_required: bool,
    pub remote_control_log_error: Option<String>,
    pub remote_control_log_path: Option<PathBuf>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexRemoteControlEnablement {
    pub codex_home: PathBuf,
    pub config_updated: bool,
    pub db_updated: bool,
    pub backup_dir: Option<PathBuf>,
    pub status: CodexAppBridgeStatus,
}

#[derive(Debug, Clone)]
struct FeatureDbStatus {
    exists: bool,
    table_exists: bool,
    enabled: bool,
    updated_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct LogDiagnostics {
    enablement_seen: bool,
    sign_in_required: bool,
    latest_error: Option<String>,
    latest_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct TableColumn {
    name: String,
    not_null: bool,
    has_default: bool,
    pk: bool,
}

pub fn codex_app_bridge_status(codex_home: &Path) -> Result<CodexAppBridgeStatus, String> {
    let auth_path = codex_home.join("auth.json");
    let config_path = codex_home.join("config.toml");
    let app_db_path = app_db_path(codex_home);
    let auth = read_auth_status(&auth_path);
    let config_text = fs::read_to_string(&config_path).unwrap_or_default();
    let provider = read_root_key(&config_text, "model_provider");
    let provider_name = provider.as_deref().unwrap_or("cm");
    let provider_base_url = read_table_key(
        &config_text,
        &format!("model_providers.{provider_name}"),
        "base_url",
    );
    let provider_wire_api = read_table_key(
        &config_text,
        &format!("model_providers.{provider_name}"),
        "wire_api",
    );
    let provider_requires_openai_auth = read_table_bool(
        &config_text,
        &format!("model_providers.{provider_name}"),
        "requires_openai_auth",
    )
    .unwrap_or(false);
    let remote_connections_enabled =
        read_table_bool(&config_text, "features", "remote_connections").unwrap_or(false);
    let legacy_remote_control_present = read_table_key(&config_text, "features", "remote_control")
        .is_some()
        || read_table_key(&config_text, "features", "remote_control_enabled").is_some();
    let db_status = read_feature_db_status(&app_db_path);
    let log_diagnostics = scan_remote_control_logs(codex_home);

    let provider_is_cm = provider.as_deref() == Some("cm")
        && provider_wire_api.as_deref() == Some("responses")
        && provider_requires_openai_auth
        && provider_base_url
            .as_deref()
            .map(|url| url.contains("/v1"))
            .unwrap_or(false);

    let mut issues = Vec::new();
    if !auth.auth_mode_chatgpt {
        issues.push("auth.json 未处于 chatgpt 登录模式".to_string());
    }
    if !auth.has_access_token || !auth.has_id_token || !auth.has_refresh_token {
        issues.push("auth.json 缺少 ChatGPT access/id/refresh token".to_string());
    }
    if !provider_is_cm {
        issues.push("config.toml 当前默认 provider 不是 CodexManager 桥接配置".to_string());
    }
    if !remote_connections_enabled {
        issues.push("config.toml 未开启 features.remote_connections".to_string());
    }
    if !db_status.exists {
        issues.push(format!("Codex App 数据库不存在: {}", app_db_path.display()));
    } else if !db_status.table_exists {
        issues.push(format!("Codex App 数据库缺少 {FEATURE_TABLE} 表"));
    } else if !db_status.enabled {
        issues.push("Codex App 数据库未启用 remote_control 特性".to_string());
    }
    if log_diagnostics.sign_in_required {
        issues.push("Codex Desktop 日志显示尚未在桌面端完成 ChatGPT 登录/远控授权".to_string());
    } else if !log_diagnostics.enablement_seen && db_status.enabled && remote_connections_enabled {
        issues.push(
            "尚未在 Codex Desktop 日志中看到 remote_control enablement 成功，需完整重启 Codex Desktop 并确认桌面端已登录 ChatGPT".to_string(),
        );
    }
    if let Some(error) = &log_diagnostics.latest_error {
        issues.push(format!("Codex Desktop 远控日志错误: {error}"));
    }

    Ok(CodexAppBridgeStatus {
        codex_home: codex_home.to_path_buf(),
        auth_path,
        config_path,
        app_db_path,
        auth_mode_chatgpt: auth.auth_mode_chatgpt,
        has_access_token: auth.has_access_token,
        has_id_token: auth.has_id_token,
        has_refresh_token: auth.has_refresh_token,
        last_refresh: auth.last_refresh,
        provider,
        provider_base_url,
        provider_wire_api,
        provider_requires_openai_auth,
        provider_is_cm,
        remote_connections_enabled,
        legacy_remote_control_present,
        db_exists: db_status.exists,
        db_table_exists: db_status.table_exists,
        db_remote_control_enabled: db_status.enabled,
        db_updated_at: db_status.updated_at,
        log_enablement_seen: log_diagnostics.enablement_seen,
        desktop_sign_in_required: log_diagnostics.sign_in_required,
        remote_control_log_error: log_diagnostics.latest_error,
        remote_control_log_path: log_diagnostics.latest_path,
        issues,
    })
}

pub fn enable_codex_mobile_remote_control(
    codex_home: &Path,
) -> Result<CodexRemoteControlEnablement, String> {
    let config_path = codex_home.join("config.toml");
    let app_db_path = app_db_path(codex_home);
    validate_feature_table_for_write(&app_db_path)?;

    fs::create_dir_all(codex_home).map_err(|err| err.to_string())?;
    let existing_config = fs::read_to_string(&config_path).unwrap_or_default();
    let next_config = upsert_remote_connections_feature(&existing_config);
    let config_updated = normalize_eol(&existing_config) != normalize_eol(&next_config);

    let db_will_update = !read_feature_db_status(&app_db_path).enabled;
    let backup_dir = if config_updated || db_will_update {
        Some(create_remote_control_backup(
            codex_home,
            config_updated,
            db_will_update,
        )?)
    } else {
        None
    };

    if config_updated {
        fs::write(&config_path, next_config).map_err(|err| err.to_string())?;
    }
    let db_updated = enable_remote_control_in_db(&app_db_path)?;
    let status = codex_app_bridge_status(codex_home)?;

    Ok(CodexRemoteControlEnablement {
        codex_home: codex_home.to_path_buf(),
        config_updated,
        db_updated,
        backup_dir,
        status,
    })
}

pub(super) fn upsert_remote_connections_feature(contents: &str) -> String {
    let mut lines = contents
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    let Some(start) = table_header_index(&lines, "features") else {
        if !lines.is_empty()
            && lines
                .last()
                .map(|line| !line.trim().is_empty())
                .unwrap_or(false)
        {
            lines.push(String::new());
        }
        lines.push("[features]".to_string());
        lines.push("remote_connections = true".to_string());
        let mut output = lines.join("\n");
        output.push('\n');
        return output;
    };

    let mut remote_connections_index = None;
    let mut index = start + 1;
    while index < lines.len() {
        if lines[index].trim_start().starts_with('[') {
            break;
        }
        if root_line_key(&lines[index]) == Some("remote_control")
            || root_line_key(&lines[index]) == Some("remote_control_enabled")
        {
            lines.remove(index);
            continue;
        }
        if root_line_key(&lines[index]) == Some("remote_connections") {
            remote_connections_index = Some(index);
        }
        index += 1;
    }

    if let Some(index) = remote_connections_index {
        lines[index] = "remote_connections = true".to_string();
    } else {
        let insert_at = next_table_index(&lines, start + 1).unwrap_or(lines.len());
        lines.insert(insert_at, "remote_connections = true".to_string());
    }

    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn app_db_path(codex_home: &Path) -> PathBuf {
    codex_home.join("sqlite").join("codex-dev.db")
}

#[derive(Debug, Clone)]
struct AuthStatus {
    auth_mode_chatgpt: bool,
    has_access_token: bool,
    has_id_token: bool,
    has_refresh_token: bool,
    last_refresh: Option<i64>,
}

fn read_auth_status(path: &Path) -> AuthStatus {
    let value = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or(Value::Null);
    let tokens = value.get("tokens").and_then(Value::as_object);
    AuthStatus {
        auth_mode_chatgpt: value
            .get("auth_mode")
            .and_then(Value::as_str)
            .map(|mode| mode.eq_ignore_ascii_case("chatgpt"))
            .unwrap_or(false),
        has_access_token: tokens
            .and_then(|items| items.get("access_token"))
            .and_then(Value::as_str)
            .map(|token| !token.trim().is_empty())
            .unwrap_or(false),
        has_id_token: tokens
            .and_then(|items| items.get("id_token"))
            .and_then(Value::as_str)
            .map(|token| !token.trim().is_empty())
            .unwrap_or(false),
        has_refresh_token: tokens
            .and_then(|items| items.get("refresh_token"))
            .and_then(Value::as_str)
            .map(|token| !token.trim().is_empty())
            .unwrap_or(false),
        last_refresh: tokens
            .and_then(|items| items.get("last_refresh"))
            .and_then(Value::as_i64),
    }
}

fn read_feature_db_status(path: &Path) -> FeatureDbStatus {
    if !path.exists() {
        return FeatureDbStatus {
            exists: false,
            table_exists: false,
            enabled: false,
            updated_at: None,
        };
    }

    let Ok(conn) = Connection::open(path) else {
        return FeatureDbStatus {
            exists: true,
            table_exists: false,
            enabled: false,
            updated_at: None,
        };
    };
    if !table_exists(&conn, FEATURE_TABLE) {
        return FeatureDbStatus {
            exists: true,
            table_exists: false,
            enabled: false,
            updated_at: None,
        };
    }

    let row = conn
        .query_row(
            &format!("SELECT enabled, updated_at FROM {FEATURE_TABLE} WHERE feature_name = ?1"),
            params![REMOTE_CONTROL_FEATURE],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?)),
        )
        .optional()
        .ok()
        .flatten();

    FeatureDbStatus {
        exists: true,
        table_exists: true,
        enabled: row
            .as_ref()
            .map(|(enabled, _)| *enabled != 0)
            .unwrap_or(false),
        updated_at: row.and_then(|(_, updated_at)| updated_at),
    }
}

fn validate_feature_table_for_write(path: &Path) -> Result<Vec<TableColumn>, String> {
    if !path.exists() {
        return Err(format!("Codex App 数据库不存在: {}", path.display()));
    }
    let conn = Connection::open(path).map_err(|err| format!("打开 Codex App 数据库失败: {err}"))?;
    if !table_exists(&conn, FEATURE_TABLE) {
        return Err(format!("Codex App 数据库缺少 {FEATURE_TABLE} 表"));
    }
    let columns = read_table_columns(&conn, FEATURE_TABLE)?;
    let has_column = |name: &str| columns.iter().any(|column| column.name == name);
    for required in ["feature_name", "enabled", "updated_at"] {
        if !has_column(required) {
            return Err(format!(
                "Codex App 数据库 {FEATURE_TABLE} 表缺少 {required} 列"
            ));
        }
    }
    for column in &columns {
        let known = matches!(
            column.name.as_str(),
            "feature_name" | "enabled" | "updated_at" | "created_at"
        );
        if !known && column.not_null && !column.has_default && !column.pk {
            return Err(format!(
                "Codex App 数据库 {FEATURE_TABLE} 表包含不支持的必填列 {}",
                column.name
            ));
        }
    }
    Ok(columns)
}

fn enable_remote_control_in_db(path: &Path) -> Result<bool, String> {
    let columns = validate_feature_table_for_write(path)?;
    let conn = Connection::open(path).map_err(|err| format!("打开 Codex App 数据库失败: {err}"))?;
    let current = read_feature_db_status(path);
    if current.enabled {
        return Ok(false);
    }

    let now = current_millis();
    let updated = conn
        .execute(
            &format!(
                "UPDATE {FEATURE_TABLE} SET enabled = 1, updated_at = ?1 WHERE feature_name = ?2"
            ),
            params![now, REMOTE_CONTROL_FEATURE],
        )
        .map_err(|err| format!("更新 remote_control 特性失败: {err}"))?;
    if updated > 0 {
        return Ok(true);
    }

    if columns.iter().any(|column| column.name == "created_at") {
        conn.execute(
            &format!(
                "INSERT INTO {FEATURE_TABLE} (feature_name, enabled, updated_at, created_at) VALUES (?1, 1, ?2, ?2)"
            ),
            params![REMOTE_CONTROL_FEATURE, now],
        )
        .map_err(|err| format!("写入 remote_control 特性失败: {err}"))?;
    } else {
        conn.execute(
            &format!(
                "INSERT INTO {FEATURE_TABLE} (feature_name, enabled, updated_at) VALUES (?1, 1, ?2)"
            ),
            params![REMOTE_CONTROL_FEATURE, now],
        )
        .map_err(|err| format!("写入 remote_control 特性失败: {err}"))?;
    }

    Ok(true)
}

fn read_table_columns(conn: &Connection, table: &str) -> Result<Vec<TableColumn>, String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|err| format!("读取 {table} 表结构失败: {err}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(TableColumn {
                name: row.get::<_, String>(1)?,
                not_null: row.get::<_, i64>(3)? != 0,
                has_default: row.get::<_, Option<String>>(4)?.is_some(),
                pk: row.get::<_, i64>(5)? != 0,
            })
        })
        .map_err(|err| format!("读取 {table} 表结构失败: {err}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 {table} 表结构失败: {err}"))
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        params![table],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .unwrap_or(false)
}

fn create_remote_control_backup(
    codex_home: &Path,
    include_config: bool,
    include_db: bool,
) -> Result<PathBuf, String> {
    let backup_root = codex_home.join("backups_state").join("cm-remote-control");
    let mut backup_dir = backup_root.join(timestamp_name());
    let mut suffix = 0;
    while backup_dir.exists() {
        suffix += 1;
        backup_dir = backup_root.join(format!("{}-{suffix}", timestamp_name()));
    }
    fs::create_dir_all(&backup_dir).map_err(|err| err.to_string())?;
    if include_config {
        copy_if_exists(
            &codex_home.join("config.toml"),
            &backup_dir.join("config.toml"),
        )?;
    }
    if include_db {
        copy_if_exists(&app_db_path(codex_home), &backup_dir.join("codex-dev.db"))?;
    }
    Ok(backup_dir)
}

fn copy_if_exists(source: &Path, target: &Path) -> Result<(), String> {
    if source.exists() {
        fs::copy(source, target).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn scan_remote_control_logs(codex_home: &Path) -> LogDiagnostics {
    let mut diagnostics = LogDiagnostics::default();
    for path in remote_control_log_candidates(codex_home) {
        merge_log_diagnostics(&mut diagnostics, scan_log_path(&path, 0));
        if diagnostics.enablement_seen && diagnostics.sign_in_required {
            break;
        }
    }
    diagnostics
}

fn remote_control_log_candidates(codex_home: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![
        codex_home.join("logs"),
        codex_home.join("log"),
        codex_home.join("local_app_server.log"),
    ];

    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let local_app_data = PathBuf::from(local_app_data);
        candidates.push(local_app_data.join("Codex").join("Logs"));
        candidates.push(local_app_data.join("OpenAI").join("Codex").join("Logs"));

        let packages = local_app_data.join("Packages");
        if let Ok(entries) = fs::read_dir(packages) {
            for entry in entries.filter_map(Result::ok) {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.starts_with("openai.codex_") {
                    candidates.push(
                        entry
                            .path()
                            .join("LocalCache")
                            .join("Local")
                            .join("Codex")
                            .join("Logs"),
                    );
                }
            }
        }
    }

    candidates
}

fn merge_log_diagnostics(target: &mut LogDiagnostics, next: LogDiagnostics) {
    target.enablement_seen |= next.enablement_seen;
    target.sign_in_required |= next.sign_in_required;
    if next.latest_error.is_some() {
        target.latest_error = next.latest_error;
        target.latest_path = next.latest_path;
    } else if target.latest_path.is_none() && next.latest_path.is_some() {
        target.latest_path = next.latest_path;
    }
}

fn scan_log_path(path: &Path, depth: usize) -> LogDiagnostics {
    if depth > 6 || !path.exists() {
        return LogDiagnostics::default();
    }
    if path.is_file() {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !(name.ends_with(".log") || name.ends_with(".txt") || name.contains("codex")) {
            return LogDiagnostics::default();
        }
        return read_log_tail(path)
            .map(|text| parse_log_diagnostics(path, &text))
            .unwrap_or_default();
    }

    let Ok(entries) = fs::read_dir(path) else {
        return LogDiagnostics::default();
    };
    let mut diagnostics = LogDiagnostics::default();
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| {
        entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    for entry in entries.into_iter().rev().take(100) {
        merge_log_diagnostics(&mut diagnostics, scan_log_path(&entry.path(), depth + 1));
        if diagnostics.enablement_seen && diagnostics.sign_in_required {
            break;
        }
    }
    diagnostics
}

fn read_log_tail(path: &Path) -> Result<String, String> {
    const MAX_BYTES: u64 = 2 * 1024 * 1024;
    let mut file = fs::File::open(path).map_err(|err| err.to_string())?;
    let len = file.metadata().map_err(|err| err.to_string())?.len();
    if len > MAX_BYTES {
        file.seek(SeekFrom::End(-(MAX_BYTES as i64)))
            .map_err(|err| err.to_string())?;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn parse_log_diagnostics(path: &Path, text: &str) -> LogDiagnostics {
    let enablement_seen = text.contains("experimentalFeature/enablement/set")
        && (text.contains("errorCode=null")
            || text.contains("\"errorCode\":null")
            || text.contains("'errorCode':null"));
    let sign_in_required =
        text.contains("Sign in to ChatGPT in Codex Desktop to check remote control authorization");
    let latest_error = if sign_in_required {
        Some("桌面端需要先登录 ChatGPT 才能注册远程控制授权".to_string())
    } else if text.contains("method=experimentalFeature/enablement/set")
        && text.contains("errorCode=-32600")
    {
        Some(
            "experimentalFeature/enablement/set 返回 -32600，Codex Desktop 未完成远控特性同步"
                .to_string(),
        )
    } else {
        None
    };

    LogDiagnostics {
        enablement_seen,
        sign_in_required,
        latest_error,
        latest_path: Some(path.to_path_buf()),
    }
}

fn read_root_key(contents: &str, key: &str) -> Option<String> {
    contents
        .lines()
        .take_while(|line| !line.trim_start().starts_with('['))
        .find(|line| root_line_key(line) == Some(key))
        .and_then(|line| line.split_once('=').map(|(_, raw)| parse_toml_scalar(raw)))
}

fn read_table_key(contents: &str, table: &str, key: &str) -> Option<String> {
    let lines = contents.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| line.trim() == format!("[{table}]"))?;
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .map(|offset| start + 1 + offset)
        .unwrap_or(lines.len());
    lines[start + 1..end]
        .iter()
        .find(|line| root_line_key(line) == Some(key))
        .and_then(|line| line.split_once('=').map(|(_, raw)| parse_toml_scalar(raw)))
}

fn read_table_bool(contents: &str, table: &str, key: &str) -> Option<bool> {
    read_table_key(contents, table, key).and_then(|value| match value.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}

fn table_header_index(lines: &[String], table: &str) -> Option<usize> {
    let header = format!("[{table}]");
    lines.iter().position(|line| line.trim() == header)
}

fn next_table_index(lines: &[String], start: usize) -> Option<usize> {
    lines[start..]
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .map(|offset| start + offset)
}

fn root_line_key(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') || trimmed.starts_with('[') {
        return None;
    }
    trimmed.split_once('=').map(|(key, _)| key.trim())
}

fn parse_toml_scalar(value: &str) -> String {
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

fn current_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn timestamp_name() -> String {
    chrono::Local::now().format("%Y%m%d%H%M%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_remote_connections_updates_features_table() {
        let before = r#"model_provider = "cm"

[features]
remote_control = true
remote_connections = false

[profiles.work]
model = "gpt-5"
"#;

        let after = upsert_remote_connections_feature(before);

        assert!(after.contains("[features]"));
        assert!(after.contains("remote_connections = true"));
        assert!(!after.contains("remote_control = true"));
        assert!(after.contains("[profiles.work]"));
    }

    #[test]
    fn upsert_remote_connections_handles_legacy_only_features_table() {
        let after = upsert_remote_connections_feature("[features]\nremote_control = true\n");

        assert_eq!(after, "[features]\nremote_connections = true\n");
    }

    #[test]
    fn enable_remote_control_backs_up_and_upserts_db() {
        let home = temp_home("remote-control-upsert");
        fs::create_dir_all(home.join("sqlite")).expect("create sqlite dir");
        fs::write(
            home.join("config.toml"),
            "[features]\nremote_connections = false\n",
        )
        .expect("write config");
        let db_path = app_db_path(&home);
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "CREATE TABLE local_app_server_feature_enablement (
                feature_name TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                created_at INTEGER
            )",
            [],
        )
        .expect("create table");
        drop(conn);

        let result = enable_codex_mobile_remote_control(&home).expect("enable remote control");

        assert!(result.config_updated);
        assert!(result.db_updated);
        assert!(result.backup_dir.as_ref().is_some_and(|path| path.exists()));
        let config = fs::read_to_string(home.join("config.toml")).expect("read config");
        assert!(config.contains("remote_connections = true"));
        let conn = Connection::open(db_path).expect("open db");
        let enabled: i64 = conn
            .query_row(
                "SELECT enabled FROM local_app_server_feature_enablement WHERE feature_name = 'remote_control'",
                [],
                |row| row.get(0),
            )
            .expect("read enabled");
        assert_eq!(enabled, 1);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn enable_remote_control_reports_missing_app_db() {
        let home = temp_home("remote-control-missing-db");
        fs::create_dir_all(&home).expect("create home");

        let err = enable_codex_mobile_remote_control(&home).expect_err("missing db");

        assert!(err.contains("Codex App 数据库不存在"));
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn parse_log_diagnostics_detects_desktop_sign_in_requirement() {
        let diagnostics = parse_log_diagnostics(
            Path::new("codex.log"),
            "refresh_local_remote_control_client_id_failed errorMessage=\"Sign in to ChatGPT in Codex Desktop to check remote control authorization.\"",
        );

        assert!(diagnostics.sign_in_required);
        assert!(!diagnostics.enablement_seen);
        assert!(diagnostics.latest_error.is_some());
    }

    #[test]
    fn parse_log_diagnostics_detects_successful_enablement() {
        let diagnostics = parse_log_diagnostics(
            Path::new("codex.log"),
            r#"method=experimentalFeature/enablement/set errorCode=null"#,
        );

        assert!(diagnostics.enablement_seen);
        assert!(!diagnostics.sign_in_required);
    }

    fn temp_home(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "codexmanager-{name}-{}-{}",
            std::process::id(),
            current_millis()
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }
}
