use rusqlite::{params, Connection, Row};
use serde_json::json;
use std::path::{Path, PathBuf};

use super::backup::{backup_rollout_file, restore_rollout_file, BackupToken};
use super::types::{DeleteResult, SchemaKind, SessionRef};

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
        return Err(format!(
            "Codex 数据库不存在: {}",
            db_path.display()
        ));
    }
    Connection::open(db_path)
        .map_err(|e| format!("打开 Codex 数据库失败: {e}"))
}

/// 检测 SQLite schema 类型
fn detect_schema(conn: &Connection) -> SchemaKind {
    let has_codex_threads: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='codex_threads'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);

    if has_codex_threads {
        SchemaKind::CodexThreads
    } else {
        SchemaKind::GenericSessions
    }
}

/// 列出所有会话
pub fn list_sessions(db_path: &Path) -> Result<Vec<SessionRef>, String> {
    let conn = open_codex_db(db_path)?;
    match detect_schema(&conn) {
        SchemaKind::CodexThreads => list_codex_threads(&conn),
        SchemaKind::GenericSessions => list_generic_sessions(&conn),
    }
}

fn list_codex_threads(conn: &Connection) -> Result<Vec<SessionRef>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, created_at, updated_at FROM codex_threads ORDER BY updated_at DESC LIMIT 500",
        )
        .map_err(|e| format!("查询 codex_threads 失败: {e}"))?;

    let sessions = stmt
        .query_map([], |row| {
            Ok(SessionRef {
                session_id: row.get::<_, String>(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })
        .map_err(|e| format!("遍历 codex_threads 结果失败: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(sessions)
}

fn list_generic_sessions(conn: &Connection) -> Result<Vec<SessionRef>, String> {
    let mut stmt = conn
        .prepare("SELECT id, title, created_at, updated_at FROM sessions ORDER BY updated_at DESC LIMIT 500")
        .map_err(|e| format!("查询 sessions 失败: {e}"))?;

    let sessions = stmt
        .query_map([], |row| {
            Ok(SessionRef {
                session_id: row.get::<_, String>(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })
        .map_err(|e| format!("遍历 sessions 结果失败: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(sessions)
}

/// 列出归档会话（codex_threads schema 中 is_archived = 1）
pub fn list_archived_sessions(db_path: &Path) -> Result<Vec<SessionRef>, String> {
    let conn = open_codex_db(db_path)?;
    if detect_schema(&conn) != SchemaKind::CodexThreads {
        return Ok(vec![]);
    }

    let has_archived_col: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('codex_threads') WHERE name='is_archived'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);

    if !has_archived_col {
        return Ok(vec![]);
    }

    let mut stmt = conn
        .prepare(
            "SELECT id, title, created_at, updated_at FROM codex_threads WHERE is_archived = 1 ORDER BY updated_at DESC",
        )
        .map_err(|e| format!("查询归档 codex_threads 失败: {e}"))?;

    let sessions = stmt
        .query_map([], |row| {
            Ok(SessionRef {
                session_id: row.get::<_, String>(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
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
        SchemaKind::CodexThreads => delete_codex_thread(&conn, db_path, session_id),
        SchemaKind::GenericSessions => delete_generic_session(&conn, session_id),
    }
}

fn delete_codex_thread(
    conn: &Connection,
    _db_path: &Path,
    session_id: &str,
) -> Result<DeleteResult, String> {
    // 检查字段是否存在 rollout_path
    let has_rollout: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('codex_threads') WHERE name='rollout_path'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);

    // 读取被删行快照
    let row_data: Option<(String, Option<String>)> = if has_rollout {
        conn.query_row(
            "SELECT hex(data), rollout_path FROM codex_threads WHERE id = ?1",
            params![session_id],
            |row: &Row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .ok()
    } else {
        conn.query_row(
            "SELECT hex(data) FROM codex_threads WHERE id = ?1",
            params![session_id],
            |row: &Row| Ok((row.get::<_, String>(0)?, Option::<String>::None)),
        )
        .ok()
    };

    let Some((hex_data, rollout_path)) = row_data else {
        return Ok(DeleteResult::not_found(session_id));
    };

    // 备份 rollout 文件（如果存在）
    let rollout_backup: Option<String> = rollout_path.as_deref().and_then(|p| {
        let path = PathBuf::from(p);
        backup_rollout_file(&path)
    });

    // 创建备份令牌（备份原始行数据）
    let token = BackupToken::new(
        session_id,
        json!({
            "schema": "codex_threads",
            "data_hex": hex_data,
            "rollout_path": rollout_path,
        }),
        rollout_backup,
    );

    match token.save() {
        Err(e) => {
            log::warn!("备份会话 {session_id} 失败，取消删除: {e}");
            return Ok(DeleteResult::backup_failed(session_id));
        }
        Ok(token_id) => {
            // 删除 codex_threads 行（关联表依赖 ON DELETE CASCADE 或手动删）
            conn.execute("DELETE FROM codex_threads WHERE id = ?1", params![session_id])
                .map_err(|e| format!("删除 codex_threads 行失败: {e}"))?;

            // 删除 rollout 原始文件
            if let Some(rp) = &rollout_path {
                let _ = std::fs::remove_file(rp);
            }

            log::info!("会话 {session_id} 已删除，令牌: {token_id}");
            Ok(DeleteResult::deleted(session_id, token_id))
        }
    }
}

fn delete_generic_session(conn: &Connection, session_id: &str) -> Result<DeleteResult, String> {
    // 备份行快照
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

    let token = BackupToken::new(
        session_id,
        json!({ "schema": "generic_sessions" }),
        None,
    );

    match token.save() {
        Err(e) => {
            log::warn!("备份 generic session {session_id} 失败: {e}");
            return Ok(DeleteResult::backup_failed(session_id));
        }
        Ok(token_id) => {
            conn.execute("DELETE FROM messages WHERE session_id = ?1", params![session_id])
                .map_err(|e| format!("删除 messages 失败: {e}"))?;
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
        "codex_threads" => {
            let data_hex = token
                .rows
                .get("data_hex")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let rollout_path = token
                .rows
                .get("rollout_path")
                .and_then(|v| v.as_str());

            conn.execute(
                "INSERT OR REPLACE INTO codex_threads (id, data) SELECT ?1, unhex(?2)",
                params![token.session_id, data_hex],
            )
            .map_err(|e| format!("还原 codex_threads 失败: {e}"))?;

            // 还原 rollout 文件
            if let (Some(backup_path), Some(orig_path)) =
                (token.rollout_backup_path.as_deref(), rollout_path)
            {
                if let Err(e) = restore_rollout_file(backup_path, orig_path) {
                    log::warn!("还原 rollout 文件失败（行已还原）: {e}");
                }
            }
        }
        "generic_sessions" => {
            // generic sessions 暂只移除令牌，不恢复数据（复杂度高，messages 已删）
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
