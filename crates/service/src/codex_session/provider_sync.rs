use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const TARGET_PROVIDER: &str = "cm";
const SESSION_DIRS: [&str; 2] = ["sessions", "archived_sessions"];
const BACKUP_KEEP_COUNT: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSyncStatus {
    Skipped,
    Synced,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSyncResult {
    pub status: ProviderSyncStatus,
    pub target_provider: String,
    pub changed_session_files: usize,
    pub sqlite_rows_updated: usize,
    pub backup_dir: Option<PathBuf>,
    pub message: String,
}

#[derive(Debug, Clone)]
struct SessionChange {
    path: PathBuf,
    original_first_line: String,
    next_first_line: String,
    separator: String,
    thread_id: Option<String>,
    cwd: Option<String>,
    has_user_event: bool,
    rewrite_needed: bool,
}

pub fn sync_provider_to_cm(codex_home: &Path) -> Result<ProviderSyncResult, String> {
    if !codex_home.exists() {
        return Ok(result(
            ProviderSyncStatus::Skipped,
            0,
            0,
            None,
            format!("Codex home not found: {}", codex_home.display()),
        ));
    }

    let lock_dir = codex_home.join("tmp").join("provider-sync.lock");
    if acquire_lock(&lock_dir).is_err() {
        return Ok(result(
            ProviderSyncStatus::Skipped,
            0,
            0,
            None,
            format!("Provider sync lock exists: {}", lock_dir.display()),
        ));
    }

    let sync_result = sync_provider_to_cm_locked(codex_home);
    let _ = release_lock(&lock_dir);
    sync_result
}

fn sync_provider_to_cm_locked(codex_home: &Path) -> Result<ProviderSyncResult, String> {
    let changes = collect_session_changes(codex_home).map_err(|err| err.to_string())?;
    let rewrite_changes = changes
        .iter()
        .filter(|change| change.rewrite_needed)
        .cloned()
        .collect::<Vec<_>>();
    let thread_ids_with_user_events = changes
        .iter()
        .filter(|change| change.has_user_event)
        .filter_map(|change| change.thread_id.clone())
        .collect::<HashSet<_>>();
    let cwd_by_thread_id = changes
        .iter()
        .filter_map(|change| Some((change.thread_id.clone()?, change.cwd.clone()?)))
        .collect::<HashMap<_, _>>();
    let sqlite_rows_to_update = count_sqlite_updates(
        &codex_home.join("state_5.sqlite"),
        &thread_ids_with_user_events,
        &cwd_by_thread_id,
    )
    .map_err(|err| err.to_string())?;

    if rewrite_changes.is_empty() && sqlite_rows_to_update == 0 {
        return Ok(result(
            ProviderSyncStatus::Synced,
            0,
            0,
            None,
            "Provider sync already up to date",
        ));
    }

    let backup_dir = create_backup(codex_home, &rewrite_changes).map_err(|err| err.to_string())?;
    apply_session_changes(&rewrite_changes).map_err(|err| err.to_string())?;
    let sqlite_update = match apply_sqlite_update(
        &codex_home.join("state_5.sqlite"),
        &thread_ids_with_user_events,
        &cwd_by_thread_id,
    ) {
        Ok(count) => count,
        Err(err) => {
            let _ = restore_session_changes(&rewrite_changes);
            return Err(err.to_string());
        }
    };
    let _ = prune_backups(codex_home);

    Ok(result(
        ProviderSyncStatus::Synced,
        rewrite_changes.len(),
        sqlite_update,
        Some(backup_dir),
        "Provider sync complete",
    ))
}

fn result(
    status: ProviderSyncStatus,
    changed_session_files: usize,
    sqlite_rows_updated: usize,
    backup_dir: Option<PathBuf>,
    message: impl Into<String>,
) -> ProviderSyncResult {
    ProviderSyncResult {
        status,
        target_provider: TARGET_PROVIDER.to_string(),
        changed_session_files,
        sqlite_rows_updated,
        backup_dir,
        message: message.into(),
    }
}

fn acquire_lock(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))?;
    fs::create_dir(path)?;
    fs::write(
        path.join("owner.json"),
        json!({"pid": std::process::id(), "startedAt": now_secs()}).to_string(),
    )
}

fn release_lock(path: &Path) -> std::io::Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn collect_session_changes(codex_home: &Path) -> Result<Vec<SessionChange>, String> {
    let mut changes = Vec::new();
    for path in rollout_files(codex_home)? {
        let text = fs::read_to_string(&path).map_err(|err| err.to_string())?;
        let (first_line, separator) = split_first_line(&text);
        if first_line.trim().is_empty() {
            continue;
        }
        let Ok(mut record) = serde_json::from_str::<Value>(&first_line) else {
            continue;
        };
        let Some(payload) = record.get_mut("payload").and_then(Value::as_object_mut) else {
            continue;
        };
        let thread_id = payload
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let cwd = payload
            .get("cwd")
            .and_then(Value::as_str)
            .and_then(normalize_workspace_path);
        let has_user_event =
            separator.contains("\"user_message\"") || separator.contains("\"user_input\"");
        let rewrite_needed =
            payload.get("model_provider").and_then(Value::as_str) != Some(TARGET_PROVIDER);
        if rewrite_needed {
            payload.insert(
                "model_provider".to_string(),
                Value::String(TARGET_PROVIDER.to_string()),
            );
        }
        let next_first_line = if rewrite_needed {
            serde_json::to_string(&record).map_err(|err| err.to_string())?
        } else {
            first_line.clone()
        };
        changes.push(SessionChange {
            path,
            original_first_line: first_line,
            next_first_line,
            separator,
            thread_id,
            cwd,
            has_user_event,
            rewrite_needed,
        });
    }
    Ok(changes)
}

fn rollout_files(codex_home: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for dirname in SESSION_DIRS {
        let root = codex_home.join(dirname);
        if root.exists() {
            collect_rollout_files(&root, &mut files)?;
        }
    }
    files.sort();
    Ok(files)
}

fn collect_rollout_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(|err| err.to_string())? {
        let path = entry.map_err(|err| err.to_string())?.path();
        if path.is_dir() {
            collect_rollout_files(&path, files)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn split_first_line(text: &str) -> (String, String) {
    if let Some(index) = text.find('\n') {
        (text[..index].to_string(), text[index..].to_string())
    } else {
        (text.to_string(), String::new())
    }
}

fn normalize_workspace_path(value: &str) -> Option<String> {
    let stripped = value.trim();
    if stripped.is_empty() {
        return None;
    }
    let lower = stripped.to_ascii_lowercase();
    if lower.starts_with(r"\\?\unc\") {
        return Some(format!(r"\\{}", stripped[8..].replace('/', r"\")));
    }
    if stripped.starts_with(r"\\?\") {
        return Some(stripped[4..].replace('\\', "/"));
    }
    Some(stripped.to_string())
}

fn create_backup(codex_home: &Path, changes: &[SessionChange]) -> Result<PathBuf, String> {
    let backup_root = codex_home.join("backups_state").join("provider-sync");
    let mut backup_dir = backup_root.join(timestamp_name());
    let mut suffix = 0;
    while backup_dir.exists() {
        suffix += 1;
        backup_dir = backup_root.join(format!("{}-{suffix}", timestamp_name()));
    }
    fs::create_dir_all(&backup_dir).map_err(|err| err.to_string())?;
    for name in [
        "config.toml",
        "auth.json",
        ".codex-global-state.json",
        ".codex-global-state.json.bak",
    ] {
        let source = codex_home.join(name);
        if source.exists() {
            fs::copy(&source, backup_dir.join(name)).map_err(|err| err.to_string())?;
        }
    }
    let db_dir = backup_dir.join("db");
    for name in ["state_5.sqlite", "state_5.sqlite-wal", "state_5.sqlite-shm"] {
        let source = codex_home.join(name);
        if source.exists() {
            fs::create_dir_all(&db_dir).map_err(|err| err.to_string())?;
            fs::copy(&source, db_dir.join(name)).map_err(|err| err.to_string())?;
        }
    }
    let manifest = changes
        .iter()
        .map(|change| {
            json!({
                "path": change.path.to_string_lossy(),
                "originalFirstLine": change.original_first_line,
                "separator": change.separator,
            })
        })
        .collect::<Vec<_>>();
    fs::write(
        backup_dir.join("session-meta-backup.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        backup_dir.join("metadata.json"),
        serde_json::to_vec_pretty(
            &json!({"managedBy": "CodexManager provider sync", "targetProvider": TARGET_PROVIDER}),
        )
        .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    Ok(backup_dir)
}

fn apply_session_changes(changes: &[SessionChange]) -> Result<(), String> {
    for change in changes {
        fs::write(
            &change.path,
            format!("{}{}", change.next_first_line, change.separator),
        )
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn restore_session_changes(changes: &[SessionChange]) -> Result<(), String> {
    for change in changes {
        fs::write(
            &change.path,
            format!("{}{}", change.original_first_line, change.separator),
        )
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn table_columns(
    db: &rusqlite::Connection,
    table: &str,
) -> Result<HashSet<String>, rusqlite::Error> {
    let mut stmt = db.prepare(&format!(
        "PRAGMA table_info(\"{}\")",
        table.replace('"', "\"\"")
    ))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<HashSet<_>>>()?;
    Ok(columns)
}

fn count_sqlite_updates(
    path: &Path,
    user_event_thread_ids: &HashSet<String>,
    cwd_by_thread_id: &HashMap<String, String>,
) -> Result<usize, rusqlite::Error> {
    if !path.exists() {
        return Ok(0);
    }
    let db = rusqlite::Connection::open(path)?;
    let columns = table_columns(&db, "threads")?;
    if !columns.contains("model_provider") {
        return Ok(0);
    }
    let mut total: usize = db.query_row(
        "SELECT COUNT(*) FROM threads WHERE COALESCE(model_provider, '') <> ?1",
        [TARGET_PROVIDER],
        |row| row.get::<_, i64>(0),
    )? as usize;
    if columns.contains("has_user_event") {
        for thread_id in user_event_thread_ids {
            total += db.query_row(
                "SELECT COUNT(*) FROM threads WHERE id = ?1 AND COALESCE(model_provider, '') <> ?2 AND COALESCE(has_user_event, 0) <> 1",
                (thread_id, TARGET_PROVIDER),
                |row| row.get::<_, i64>(0),
            )? as usize;
        }
    }
    if columns.contains("cwd") {
        for (thread_id, cwd) in cwd_by_thread_id {
            total += db.query_row(
                "SELECT COUNT(*) FROM threads WHERE id = ?1 AND COALESCE(model_provider, '') <> ?2 AND COALESCE(cwd, '') <> ?3",
                (thread_id, TARGET_PROVIDER, cwd),
                |row| row.get::<_, i64>(0),
            )? as usize;
        }
    }
    Ok(total)
}

fn apply_sqlite_update(
    path: &Path,
    user_event_thread_ids: &HashSet<String>,
    cwd_by_thread_id: &HashMap<String, String>,
) -> Result<usize, rusqlite::Error> {
    if !path.exists() {
        return Ok(0);
    }
    let db = rusqlite::Connection::open(path)?;
    let columns = table_columns(&db, "threads")?;
    if !columns.contains("model_provider") {
        return Ok(0);
    }
    let tx = db.transaction()?;
    if columns.contains("has_user_event") {
        for thread_id in user_event_thread_ids {
            tx.execute(
                "UPDATE threads SET has_user_event = 1 WHERE id = ?1 AND COALESCE(model_provider, '') <> ?2 AND COALESCE(has_user_event, 0) <> 1",
                (thread_id, TARGET_PROVIDER),
            )?;
        }
    }
    if columns.contains("cwd") {
        for (thread_id, cwd) in cwd_by_thread_id {
            tx.execute(
                "UPDATE threads SET cwd = ?1 WHERE id = ?2 AND COALESCE(model_provider, '') <> ?3 AND COALESCE(cwd, '') <> ?1",
                (cwd, thread_id, TARGET_PROVIDER),
            )?;
        }
    }
    let provider_rows = tx.execute(
        "UPDATE threads SET model_provider = ?1 WHERE COALESCE(model_provider, '') <> ?1",
        [TARGET_PROVIDER],
    )?;
    tx.commit()?;
    Ok(provider_rows)
}

fn prune_backups(codex_home: &Path) -> Result<(), String> {
    let root = codex_home.join("backups_state").join("provider-sync");
    if !root.exists() {
        return Ok(());
    }
    let mut managed = Vec::new();
    for entry in fs::read_dir(&root).map_err(|err| err.to_string())? {
        let path = entry.map_err(|err| err.to_string())?.path();
        if !path.is_dir() {
            continue;
        }
        let Ok(text) = fs::read_to_string(path.join("metadata.json")) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if value.get("managedBy").and_then(Value::as_str) == Some("CodexManager provider sync") {
            managed.push(path);
        }
    }
    managed.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    for path in managed.into_iter().skip(BACKUP_KEEP_COUNT) {
        let _ = fs::remove_dir_all(path);
    }
    Ok(())
}

fn timestamp_name() -> String {
    chrono::Local::now().format("%Y%m%d%H%M%S").to_string()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use serde_json::Value;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_home() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "codexmanager-provider-sync-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create temp home");
        path
    }

    fn write_rollout(home: &Path, provider: &str, thread_id: &str) -> PathBuf {
        let dir = home.join("sessions").join("2026").join("05").join("21");
        fs::create_dir_all(&dir).expect("create rollout dir");
        let path = dir.join(format!("rollout-{thread_id}.jsonl"));
        let first = serde_json::json!({
            "type": "session_meta",
            "payload": {
                "id": thread_id,
                "cwd": "D:/work/project",
                "model_provider": provider
            }
        });
        fs::write(
            &path,
            format!("{first}\n{}\n", serde_json::json!({"type":"user_message"})),
        )
        .expect("write rollout");
        path
    }

    fn create_state_db(home: &Path, provider: &str) {
        let db = Connection::open(home.join("state_5.sqlite")).expect("open db");
        db.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, model_provider TEXT, has_user_event INTEGER, cwd TEXT)",
            [],
        )
        .expect("create threads");
        db.execute(
            "INSERT INTO threads (id, model_provider, has_user_event, cwd) VALUES (?1, ?2, 0, '')",
            ("thread-1", provider),
        )
        .expect("insert thread");
    }

    #[test]
    fn sync_provider_to_cm_updates_rollout_and_sqlite_rows() {
        let home = temp_home();
        fs::write(home.join("config.toml"), "model_provider = \"cm\"\n").expect("write config");
        let rollout = write_rollout(&home, "openai", "thread-1");
        create_state_db(&home, "openai");

        let result = sync_provider_to_cm(&home).expect("sync provider");

        assert_eq!(result.status, ProviderSyncStatus::Synced);
        assert_eq!(result.target_provider, "cm");
        assert_eq!(result.changed_session_files, 1);
        assert_eq!(result.sqlite_rows_updated, 1);
        assert!(result.backup_dir.as_ref().is_some_and(|path| path.exists()));

        let text = fs::read_to_string(rollout).expect("read rollout");
        let first_line = text.lines().next().expect("first line");
        let value: Value = serde_json::from_str(first_line).expect("parse first line");
        assert_eq!(value["payload"]["model_provider"], "cm");

        let db = Connection::open(home.join("state_5.sqlite")).expect("open db");
        let row: (String, i64, String) = db
            .query_row(
                "SELECT model_provider, has_user_event, cwd FROM threads WHERE id = 'thread-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("query row");
        assert_eq!(row, ("cm".to_string(), 1, "D:/work/project".to_string()));
    }

    #[test]
    fn sync_provider_to_cm_skips_rollout_files_that_are_already_cm() {
        let home = temp_home();
        fs::write(home.join("config.toml"), "model_provider = \"cm\"\n").expect("write config");
        let rollout = write_rollout(&home, "cm", "thread-1");
        let before = fs::read_to_string(&rollout).expect("read rollout before");
        create_state_db(&home, "cm");

        let result = sync_provider_to_cm(&home).expect("sync provider");

        assert_eq!(result.status, ProviderSyncStatus::Synced);
        assert_eq!(result.changed_session_files, 0);
        assert_eq!(result.sqlite_rows_updated, 0);
        assert!(result.backup_dir.is_none());
        assert_eq!(
            fs::read_to_string(&rollout).expect("read rollout after"),
            before
        );
    }
}
