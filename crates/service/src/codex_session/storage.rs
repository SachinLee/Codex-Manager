use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row, ToSql};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

use super::backup::{backup_rollout_file, restore_rollout_file, BackupToken};
use super::types::{DeleteResult, MoveResult, SchemaKind, SessionListOptions, SessionRef};

/// 默认 Codex 数据库路径：~/.codex/state_5.sqlite
pub fn default_codex_db_path() -> PathBuf {
    if let Ok(custom) = std::env::var("CODEXMANAGER_CODEX_DB_PATH") {
        return PathBuf::from(custom);
    }
    let home = {
        #[cfg(windows)]
        {
            std::env::var("USERPROFILE")
                .ok()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
        }
        #[cfg(not(windows))]
        {
            std::env::var("HOME")
                .ok()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
        }
    };
    home.join(".codex").join("state_5.sqlite")
}

fn open_codex_db(db_path: &Path) -> Result<Connection, String> {
    if !db_path.exists() {
        return Err(format!("Codex 数据库不存在: {}", db_path.display()));
    }
    Connection::open(db_path).map_err(|e| format!("打开 Codex 数据库失败: {e}"))
}

/// 检测 Codex 实际 schema：以 `threads` 表 + `rollout_path` 列为特征。
/// 兼容旧版（generic sessions）作为兜底。
fn detect_schema(conn: &Connection) -> SchemaKind {
    if table_has_columns(conn, "threads", &["id", "title", "rollout_path"]) {
        SchemaKind::CodexThreads
    } else if table_has_columns(conn, "sessions", &["id", "title"]) {
        SchemaKind::GenericSessions
    } else {
        // 默认走 CodexThreads，保证错误信息更明确
        SchemaKind::CodexThreads
    }
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        params![table],
        |row| row.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .unwrap_or(false)
}

fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
    let sql = format!("PRAGMA table_info(\"{table}\")");
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return vec![];
    };
    stmt.query_map([], |row| row.get::<_, String>(1))
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

fn table_has_columns(conn: &Connection, table: &str, required: &[&str]) -> bool {
    if !table_exists(conn, table) {
        return false;
    }
    let cols = table_columns(conn, table);
    required.iter().all(|c| cols.iter().any(|x| x == c))
}

/// 列出所有会话
pub fn list_sessions(db_path: &Path) -> Result<Vec<SessionRef>, String> {
    list_sessions_with_options(db_path, &SessionListOptions::default())
}

pub fn list_sessions_with_options(
    db_path: &Path,
    options: &SessionListOptions,
) -> Result<Vec<SessionRef>, String> {
    let conn = open_codex_db(db_path)?;
    match detect_schema(&conn) {
        SchemaKind::CodexThreads => list_codex_threads(&conn, options),
        SchemaKind::GenericSessions => list_generic_sessions(&conn, options),
    }
}

fn normalize_session_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(500).clamp(1, 2000)
}

fn append_session_filters(
    sql: &mut String,
    params: &mut Vec<Box<dyn ToSql>>,
    options: &SessionListOptions,
) {
    let mut clauses = Vec::new();
    if let Some(updated_from) = options.updated_from {
        clauses.push("updated_at >= ?");
        params.push(Box::new(updated_from));
    }
    if let Some(updated_to) = options.updated_to {
        clauses.push("updated_at < ?");
        params.push(Box::new(updated_to));
    }
    if !clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(clauses.join(" AND ").as_str());
    }
}

fn list_codex_threads(
    conn: &Connection,
    options: &SessionListOptions,
) -> Result<Vec<SessionRef>, String> {
    let mut sql =
        "SELECT id, title, created_at, updated_at, cwd, archived FROM threads".to_string();
    let mut values: Vec<Box<dyn ToSql>> = Vec::new();
    append_session_filters(&mut sql, &mut values, options);
    sql.push_str(" ORDER BY updated_at DESC LIMIT ?");
    values.push(Box::new(normalize_session_limit(options.limit)));

    let mut stmt = conn
        .prepare(sql.as_str())
        .map_err(|e| format!("查询 threads 失败: {e}"))?;
    let params = params_from_iter(values.iter().map(|value| value.as_ref()));

    let sessions = stmt
        .query_map(params, |row| {
            Ok(SessionRef {
                session_id: row.get::<_, String>(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                cwd: row.get(4).ok(),
                archived: row.get::<_, Option<i64>>(5).ok().flatten().map(|n| n != 0),
            })
        })
        .map_err(|e| format!("遍历 threads 结果失败: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(sessions)
}

fn list_generic_sessions(
    conn: &Connection,
    options: &SessionListOptions,
) -> Result<Vec<SessionRef>, String> {
    let mut sql = "SELECT id, title, created_at, updated_at FROM sessions".to_string();
    let mut values: Vec<Box<dyn ToSql>> = Vec::new();
    append_session_filters(&mut sql, &mut values, options);
    sql.push_str(" ORDER BY updated_at DESC LIMIT ?");
    values.push(Box::new(normalize_session_limit(options.limit)));

    let mut stmt = conn
        .prepare(sql.as_str())
        .map_err(|e| format!("查询 sessions 失败: {e}"))?;
    let params = params_from_iter(values.iter().map(|value| value.as_ref()));

    let sessions = stmt
        .query_map(params, |row| {
            Ok(SessionRef {
                session_id: row.get::<_, String>(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                cwd: None,
                archived: None,
            })
        })
        .map_err(|e| format!("遍历 sessions 结果失败: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(sessions)
}

/// 列出归档会话（threads 表 `archived` 列）
pub fn list_archived_sessions(db_path: &Path) -> Result<Vec<SessionRef>, String> {
    let conn = open_codex_db(db_path)?;
    if detect_schema(&conn) != SchemaKind::CodexThreads {
        return Ok(vec![]);
    }
    if !table_columns(&conn, "threads")
        .iter()
        .any(|c| c == "archived")
    {
        return Ok(vec![]);
    }

    let mut stmt = conn
        .prepare(
            "SELECT id, title, created_at, updated_at, cwd FROM threads WHERE archived = 1 ORDER BY updated_at DESC",
        )
        .map_err(|e| format!("查询归档 threads 失败: {e}"))?;

    let sessions = stmt
        .query_map([], |row| {
            Ok(SessionRef {
                session_id: row.get::<_, String>(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                cwd: row.get(4).ok(),
                archived: Some(true),
            })
        })
        .map_err(|e| format!("遍历归档结果失败: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(sessions)
}

/// 通过标题查找归档会话的 session_id（用于注入脚本中按标题删除归档）
pub fn resolve_archived_by_title(db_path: &Path, title: &str) -> Result<Option<String>, String> {
    let sessions = list_archived_sessions(db_path)?;
    Ok(sessions
        .into_iter()
        .find(|s| s.title.as_deref() == Some(title))
        .map(|s| s.session_id))
}

/// 删除单条会话（含备份）
pub fn delete_session(db_path: &Path, session_id: &str) -> Result<DeleteResult, String> {
    let conn = open_codex_db(db_path)?;
    let schema = detect_schema(&conn);

    match schema {
        SchemaKind::CodexThreads => delete_codex_thread(&conn, session_id),
        SchemaKind::GenericSessions => delete_generic_session(&conn, session_id),
    }
}

/// 将单条会话移动到目标工作目录。`target_cwd = None` 表示移出项目目录。
pub fn move_session(
    db_path: &Path,
    session_id: &str,
    target_cwd: Option<&str>,
) -> Result<MoveResult, String> {
    let conn = open_codex_db(db_path)?;
    let schema = detect_schema(&conn);
    let normalized_target = normalize_cwd(target_cwd);

    match schema {
        SchemaKind::CodexThreads => move_codex_thread(&conn, session_id, normalized_target),
        SchemaKind::GenericSessions => Ok(MoveResult::unsupported(session_id, normalized_target)),
    }
}

pub fn move_sessions(
    db_path: &Path,
    session_ids: &[String],
    target_cwd: Option<&str>,
) -> Result<Vec<MoveResult>, String> {
    let mut results = Vec::with_capacity(session_ids.len());
    for session_id in session_ids {
        let trimmed = session_id.trim();
        if trimmed.is_empty() {
            continue;
        }
        results.push(move_session(db_path, trimmed, target_cwd)?);
    }
    Ok(results)
}

fn normalize_cwd(target_cwd: Option<&str>) -> Option<String> {
    target_cwd
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
        .map(|cwd| cwd.replace('\\', "/").trim_end_matches('/').to_string())
        .filter(|cwd| !cwd.is_empty())
}

fn move_codex_thread(
    conn: &Connection,
    session_id: &str,
    target_cwd: Option<String>,
) -> Result<MoveResult, String> {
    if !table_has_columns(conn, "threads", &["id", "cwd"]) {
        return Ok(MoveResult::unsupported(session_id, target_cwd));
    }

    let normalized_id = session_id.strip_prefix("local:").unwrap_or(session_id);
    let Some(previous_cwd) = conn
        .query_row(
            "SELECT cwd FROM \"threads\" WHERE id = ?1",
            params![normalized_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|e| format!("查询 threads.cwd 失败: {e}"))?
    else {
        return Ok(MoveResult::not_found(session_id, target_cwd));
    };

    let normalized_previous = normalize_cwd(previous_cwd.as_deref());
    if normalized_previous == target_cwd {
        return Ok(MoveResult::unchanged(
            session_id,
            normalized_previous,
            target_cwd,
        ));
    }

    conn.execute(
        "UPDATE \"threads\" SET cwd = ?1 WHERE id = ?2",
        params![target_cwd.as_deref(), normalized_id],
    )
    .map_err(|e| format!("更新 threads.cwd 失败: {e}"))?;

    Ok(MoveResult::moved(
        session_id,
        normalized_previous,
        target_cwd,
    ))
}

/// 与 CodexPlusPlus 等价：备份 threads + 关联表行 + rollout 文件，
/// 然后清理 threads + thread_dynamic_tools + thread_goals + thread_spawn_edges
/// + stage1_outputs，并把 agent_job_items.assigned_thread_id 置 NULL。
fn delete_codex_thread(conn: &Connection, session_id: &str) -> Result<DeleteResult, String> {
    let normalized_id = session_id.strip_prefix("local:").unwrap_or(session_id);

    // 主表行（必须存在）
    let thread_rows = select_rows_json(
        conn,
        "SELECT * FROM \"threads\" WHERE id = ?1",
        params![normalized_id],
    )?;
    if thread_rows.is_empty() {
        return Ok(DeleteResult::not_found(session_id));
    }

    // 关联表行备份
    let mut tables = Map::new();
    tables.insert("threads".into(), Value::Array(thread_rows.clone()));
    backup_related(
        conn,
        &mut tables,
        "thread_dynamic_tools",
        "thread_id = ?1",
        params![normalized_id],
    )?;
    backup_related(
        conn,
        &mut tables,
        "thread_goals",
        "thread_id = ?1",
        params![normalized_id],
    )?;
    backup_related(
        conn,
        &mut tables,
        "thread_spawn_edges",
        "parent_thread_id = ?1 OR child_thread_id = ?2",
        params![normalized_id, normalized_id],
    )?;
    backup_related(
        conn,
        &mut tables,
        "stage1_outputs",
        "thread_id = ?1",
        params![normalized_id],
    )?;

    // rollout 文件备份（每行一个）
    let mut rollout_backups: Vec<(String, String)> = Vec::new();
    for row in &thread_rows {
        if let Some(p) = row.get("rollout_path").and_then(|v| v.as_str()) {
            if !p.is_empty() {
                if let Some(backup) = backup_rollout_file(Path::new(p)) {
                    rollout_backups.push((p.to_string(), backup));
                }
            }
        }
    }
    let primary_rollout_backup = rollout_backups.first().map(|(_, b)| b.clone());

    let token = BackupToken::new(
        normalized_id,
        json!({
            "schema": "codex_threads_v2",
            "tables": Value::Object(tables),
            "rollout_files": rollout_backups
                .iter()
                .map(|(orig, backup)| json!({"original": orig, "backup": backup}))
                .collect::<Vec<_>>(),
        }),
        primary_rollout_backup,
    );

    let token_id = match token.save() {
        Ok(id) => id,
        Err(e) => {
            log::warn!("备份会话 {normalized_id} 失败，取消删除: {e}");
            return Ok(DeleteResult::backup_failed(session_id));
        }
    };

    // 执行删除（关联表 → 主表）
    delete_related(
        conn,
        "thread_dynamic_tools",
        "thread_id = ?1",
        params![normalized_id],
    )?;
    delete_related(
        conn,
        "thread_goals",
        "thread_id = ?1",
        params![normalized_id],
    )?;
    delete_related(
        conn,
        "thread_spawn_edges",
        "parent_thread_id = ?1 OR child_thread_id = ?2",
        params![normalized_id, normalized_id],
    )?;
    delete_related(
        conn,
        "stage1_outputs",
        "thread_id = ?1",
        params![normalized_id],
    )?;
    if table_has_columns(conn, "agent_job_items", &["assigned_thread_id"]) {
        conn.execute(
            "UPDATE \"agent_job_items\" SET assigned_thread_id = NULL WHERE assigned_thread_id = ?1",
            params![normalized_id],
        )
        .map_err(|e| format!("更新 agent_job_items 失败: {e}"))?;
    }
    conn.execute(
        "DELETE FROM \"threads\" WHERE id = ?1",
        params![normalized_id],
    )
    .map_err(|e| format!("删除 threads 行失败: {e}"))?;

    // 删除 rollout 原始文件
    for (orig, _) in &rollout_backups {
        let _ = std::fs::remove_file(orig);
    }

    log::info!("会话 {normalized_id} 已删除，令牌: {token_id}");
    Ok(DeleteResult::deleted(session_id, token_id))
}

fn delete_generic_session(conn: &Connection, session_id: &str) -> Result<DeleteResult, String> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE id = ?1",
            params![session_id],
            |row: &Row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);

    if !exists {
        return Ok(DeleteResult::not_found(session_id));
    }

    let token = BackupToken::new(session_id, json!({ "schema": "generic_sessions" }), None);

    match token.save() {
        Err(e) => {
            log::warn!("备份 generic session {session_id} 失败: {e}");
            Ok(DeleteResult::backup_failed(session_id))
        }
        Ok(token_id) => {
            if table_exists(conn, "messages") {
                conn.execute(
                    "DELETE FROM messages WHERE session_id = ?1",
                    params![session_id],
                )
                .map_err(|e| format!("删除 messages 失败: {e}"))?;
            }
            conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])
                .map_err(|e| format!("删除 sessions 失败: {e}"))?;
            Ok(DeleteResult::deleted(session_id, token_id))
        }
    }
}

/// 撤销删除（通过 undo_token 还原）
pub fn undo_delete(db_path: &Path, token_id: &str) -> Result<(), String> {
    let token = BackupToken::load(token_id)?;
    let conn = open_codex_db(db_path)?;

    let schema = token
        .rows
        .get("schema")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match schema {
        // 新版（v2）：tables 按列恢复
        "codex_threads_v2" => {
            let tables = token
                .rows
                .get("tables")
                .and_then(|v| v.as_object())
                .ok_or_else(|| "备份令牌缺少 tables".to_string())?;

            // threads 先恢复
            if let Some(threads) = tables.get("threads").and_then(|v| v.as_array()) {
                restore_table_rows(&conn, "threads", threads)?;
            }
            for (name, value) in tables {
                if name == "threads" {
                    continue;
                }
                if let Some(rows) = value.as_array() {
                    restore_table_rows(&conn, name, rows)?;
                }
            }

            // rollout 文件恢复
            if let Some(files) = token.rows.get("rollout_files").and_then(|v| v.as_array()) {
                for file in files {
                    let orig = file.get("original").and_then(|v| v.as_str());
                    let backup = file.get("backup").and_then(|v| v.as_str());
                    if let (Some(orig), Some(backup)) = (orig, backup) {
                        if let Err(e) = restore_rollout_file(backup, orig) {
                            log::warn!("还原 rollout 文件失败: {e}");
                        }
                    }
                }
            }
        }
        // 旧版（v1，已废弃但兼容）
        "codex_threads" => {
            log::warn!("旧版 codex_threads 备份，仅尝试 hex 复原（可能不完整）");
            let data_hex = token
                .rows
                .get("data_hex")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !data_hex.is_empty() {
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO threads (id, data) SELECT ?1, unhex(?2)",
                    params![token.session_id, data_hex],
                );
            }
        }
        "generic_sessions" => {
            log::warn!("generic_sessions 暂不支持完整 undo，仅清除令牌");
        }
        _ => {
            return Err(format!("未知 schema 类型: {schema}"));
        }
    }

    token.delete_file();
    Ok(())
}

/// 删除所有归档会话
pub fn delete_all_archived(db_path: &Path) -> Result<Vec<DeleteResult>, String> {
    let sessions = list_archived_sessions(db_path)?;
    let mut results = Vec::with_capacity(sessions.len());
    for session in sessions {
        match delete_session(db_path, &session.session_id) {
            Ok(r) => results.push(r),
            Err(e) => {
                log::warn!("删除归档会话 {} 失败: {e}", session.session_id);
                results.push(DeleteResult::backup_failed(session.session_id));
            }
        }
    }
    Ok(results)
}

// ---------- 通用 row <-> JSON 工具 ----------

fn select_rows_json(
    conn: &Connection,
    sql: &str,
    params: &[&dyn ToSql],
) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("准备 SQL 失败 ({sql}): {e}"))?;
    let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let column_count = column_names.len();

    let rows_iter = stmt
        .query_map(params, |row| {
            let mut obj = Map::with_capacity(column_count);
            for (i, name) in column_names.iter().enumerate() {
                let v = sqlite_value_to_json(row.get_ref(i)?);
                obj.insert(name.clone(), v);
            }
            Ok(Value::Object(obj))
        })
        .map_err(|e| format!("执行查询失败 ({sql}): {e}"))?;

    let mut out = Vec::new();
    for r in rows_iter {
        out.push(r.map_err(|e| format!("读取行失败: {e}"))?);
    }
    Ok(out)
}

fn sqlite_value_to_json(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ValueRef::Text(t) => match std::str::from_utf8(t) {
            Ok(s) => Value::String(s.to_string()),
            Err(_) => Value::String(format!("__b64:{}", base64_encode(t))),
        },
        ValueRef::Blob(b) => Value::String(format!("__b64:{}", base64_encode(b))),
    }
}

fn json_to_sqlite_value(v: &Value) -> SqlValue {
    match v {
        Value::Null => SqlValue::Null,
        Value::Bool(b) => SqlValue::Integer(if *b { 1 } else { 0 }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                SqlValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                SqlValue::Real(f)
            } else {
                SqlValue::Null
            }
        }
        Value::String(s) => {
            if let Some(b64) = s.strip_prefix("__b64:") {
                match base64_decode(b64) {
                    Some(bytes) => SqlValue::Blob(bytes),
                    None => SqlValue::Text(s.clone()),
                }
            } else {
                SqlValue::Text(s.clone())
            }
        }
        Value::Array(_) | Value::Object(_) => SqlValue::Text(v.to_string()),
    }
}

fn backup_related(
    conn: &Connection,
    tables: &mut Map<String, Value>,
    table: &str,
    where_clause: &str,
    params: &[&dyn ToSql],
) -> Result<(), String> {
    if !table_exists(conn, table) {
        return Ok(());
    }
    let sql = format!("SELECT * FROM \"{table}\" WHERE {where_clause}");
    let rows = select_rows_json(conn, &sql, params)?;
    if !rows.is_empty() {
        tables.insert(table.to_string(), Value::Array(rows));
    }
    Ok(())
}

fn delete_related(
    conn: &Connection,
    table: &str,
    where_clause: &str,
    params: &[&dyn ToSql],
) -> Result<(), String> {
    if !table_exists(conn, table) {
        return Ok(());
    }
    let sql = format!("DELETE FROM \"{table}\" WHERE {where_clause}");
    conn.execute(&sql, params)
        .map_err(|e| format!("删除 {table} 失败: {e}"))?;
    Ok(())
}

fn restore_table_rows(conn: &Connection, table: &str, rows: &[Value]) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    if !table_exists(conn, table) {
        log::warn!("undo 时表 {table} 不存在，跳过");
        return Ok(());
    }
    let existing_cols = table_columns(conn, table);

    for row in rows {
        let Some(obj) = row.as_object() else { continue };
        let cols: Vec<&String> = obj
            .keys()
            .filter(|k| existing_cols.iter().any(|c| c == *k))
            .collect();
        if cols.is_empty() {
            continue;
        }
        let placeholders: Vec<String> = (1..=cols.len()).map(|i| format!("?{i}")).collect();
        let col_list = cols
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "INSERT OR REPLACE INTO \"{table}\" ({col_list}) VALUES ({})",
            placeholders.join(", ")
        );
        let values: Vec<SqlValue> = cols
            .iter()
            .map(|c| json_to_sqlite_value(obj.get(*c).unwrap_or(&Value::Null)))
            .collect();
        conn.execute(&sql, params_from_iter(values.iter()))
            .map_err(|e| format!("还原 {table} 行失败: {e}"))?;
    }
    Ok(())
}

// ---------- 轻量 base64（避免新增依赖） ----------

fn base64_encode(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .collect();
    if bytes.is_empty() {
        return Some(vec![]);
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut i = 0;
    while i + 4 <= bytes.len() {
        let a = val(bytes[i])?;
        let b = val(bytes[i + 1])?;
        let c = val(bytes[i + 2])?;
        let d = val(bytes[i + 3])?;
        let n = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6) | (d as u32);
        out.push(((n >> 16) & 0xff) as u8);
        out.push(((n >> 8) & 0xff) as u8);
        out.push((n & 0xff) as u8);
        i += 4;
    }
    let rem = bytes.len() - i;
    if rem == 2 {
        let a = val(bytes[i])?;
        let b = val(bytes[i + 1])?;
        let n = ((a as u32) << 18) | ((b as u32) << 12);
        out.push(((n >> 16) & 0xff) as u8);
    } else if rem == 3 {
        let a = val(bytes[i])?;
        let b = val(bytes[i + 1])?;
        let c = val(bytes[i + 2])?;
        let n = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6);
        out.push(((n >> 16) & 0xff) as u8);
        out.push(((n >> 8) & 0xff) as u8);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("codexmanager-{name}-{nanos}.sqlite"))
    }

    fn create_codex_threads_db() -> PathBuf {
        let db_path = temp_db_path("codex-session-move");
        let conn = Connection::open(&db_path).expect("open temp db");
        conn.execute(
            "CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                title TEXT,
                created_at INTEGER,
                updated_at INTEGER,
                cwd TEXT,
                archived INTEGER,
                rollout_path TEXT
            )",
            [],
        )
        .expect("create threads");
        conn.execute(
            "INSERT INTO threads (id, title, created_at, updated_at, cwd, archived, rollout_path)
             VALUES ('thread-1', 'One', 1, 2, 'D:\\work\\old\\', 0, '')",
            [],
        )
        .expect("insert thread one");
        conn.execute(
            "INSERT INTO threads (id, title, created_at, updated_at, cwd, archived, rollout_path)
             VALUES ('thread-2', 'Two', 1, 3, NULL, 0, '')",
            [],
        )
        .expect("insert thread two");
        db_path
    }

    #[test]
    fn move_session_updates_codex_thread_cwd() {
        let db_path = create_codex_threads_db();
        let result = move_session(&db_path, "local:thread-1", Some("D:\\work\\new\\"))
            .expect("move session");

        assert_eq!(result.status, super::super::types::MoveStatus::Moved);
        assert_eq!(result.previous_cwd.as_deref(), Some("D:/work/old"));
        assert_eq!(result.target_cwd.as_deref(), Some("D:/work/new"));

        let sessions = list_sessions(&db_path).expect("list sessions");
        let moved = sessions
            .iter()
            .find(|session| session.session_id == "thread-1")
            .expect("moved session");
        assert_eq!(moved.cwd.as_deref(), Some("D:/work/new"));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn move_session_reports_unchanged_and_not_found() {
        let db_path = create_codex_threads_db();
        let unchanged =
            move_session(&db_path, "thread-1", Some("D:/work/old")).expect("move unchanged");
        let missing = move_session(&db_path, "missing", Some("D:/work/new")).expect("move missing");

        assert_eq!(unchanged.status, super::super::types::MoveStatus::Unchanged);
        assert_eq!(missing.status, super::super::types::MoveStatus::NotFound);

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn move_sessions_moves_multiple_threads() {
        let db_path = create_codex_threads_db();
        let ids = vec!["thread-1".to_string(), "thread-2".to_string()];
        let results = move_sessions(&db_path, &ids, Some("D:/work/target")).expect("move sessions");

        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .all(|result| result.status == super::super::types::MoveStatus::Moved));

        let sessions = list_sessions(&db_path).expect("list sessions");
        assert!(sessions
            .iter()
            .all(|session| session.cwd.as_deref() == Some("D:/work/target")));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn list_sessions_filters_by_updated_range() {
        let db_path = create_codex_threads_db();
        let options = SessionListOptions {
            updated_from: Some(3),
            updated_to: Some(4),
            limit: Some(500),
        };
        let sessions = list_sessions_with_options(&db_path, &options).expect("list sessions");

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "thread-2");

        let _ = std::fs::remove_file(db_path);
    }
}
