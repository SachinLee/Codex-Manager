use std::sync::{Mutex, OnceLock};

use crate::codex_injector::{
    new_shared_status, start_and_inject, start_plain, InjectorStatus, SharedStatus,
};

/// 全局注入器状态（单例）
fn global_status() -> &'static Mutex<SharedStatus> {
    static STATUS: OnceLock<Mutex<SharedStatus>> = OnceLock::new();
    STATUS.get_or_init(|| Mutex::new(new_shared_status()))
}

fn get_status() -> InjectorStatus {
    let guard = global_status().lock().unwrap();
    let mut inner = guard.lock().unwrap();
    if inner.running {
        if let Some(pid) = inner.pid {
            if !crate::codex_injector::is_process_alive(pid) {
                inner.running = false;
                inner.injected = false;
                inner.debug_port = None;
                inner.pid = None;
            }
        }
    }
    inner.clone()
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    pub custom_path: Option<String>,
    pub debug_port: Option<u16>,
}

/// 启动 Codex 并注入增强功能（异步：在后台线程中执行）
#[tauri::command]
pub async fn codex_launcher_start(
    opts: Option<LaunchOptions>,
) -> Result<serde_json::Value, String> {
    let status_snapshot = get_status();
    if status_snapshot.running {
        return Err("Codex 注入器已在运行中".to_string());
    }

    let custom_path = opts.as_ref().and_then(|o| o.custom_path.clone());
    let debug_port = opts.as_ref().and_then(|o| o.debug_port).unwrap_or(57320);

    let status_arc = {
        let guard = global_status().lock().unwrap();
        guard.clone()
    };

    std::thread::spawn(move || {
        log::info!("Codex 注入器启动，端口: {debug_port}");
        let result = start_and_inject(custom_path.as_deref(), debug_port, status_arc);
        if let Err(e) = result {
            log::warn!("Codex 注入器退出: {e}");
        }
    });

    Ok(serde_json::json!({ "ok": true, "debug_port": debug_port }))
}

/// 普通启动 Codex（不打开调试端口，不注入增强脚本）
#[tauri::command]
pub async fn codex_launcher_start_plain(
    opts: Option<LaunchOptions>,
) -> Result<serde_json::Value, String> {
    let status_snapshot = get_status();
    if status_snapshot.running {
        return Err("Codex 已在运行中".to_string());
    }

    let custom_path = opts.as_ref().and_then(|o| o.custom_path.clone());
    let status_arc = {
        let guard = global_status().lock().unwrap();
        guard.clone()
    };

    start_plain(custom_path.as_deref(), status_arc)?;

    Ok(serde_json::json!({ "ok": true }))
}

/// 停止 Codex 进程（如果由我们启动）
#[tauri::command]
pub async fn codex_launcher_stop() -> Result<serde_json::Value, String> {
    let status = get_status();
    if let Some(pid) = status.pid {
        log::info!("停止 Codex 进程 PID: {pid}");
        crate::codex_injector::stop_process(pid);
    }
    let outer = global_status().lock().unwrap();
    let mut s = outer.lock().unwrap();
    s.running = false;
    s.injected = false;
    s.pid = None;
    Ok(serde_json::json!({ "ok": true }))
}

/// 获取注入器当前状态
#[tauri::command]
pub async fn codex_launcher_status() -> Result<InjectorStatus, String> {
    Ok(get_status())
}

/// 探测 Codex 安装路径（不启动）
#[tauri::command]
pub async fn codex_launcher_resolve_path(
    custom_path: Option<String>,
) -> Result<serde_json::Value, String> {
    match crate::codex_injector::find_codex(custom_path.as_deref()) {
        Ok(kind) => {
            let desc = crate::codex_injector::describe_codex_path(&kind);
            Ok(serde_json::json!({ "found": true, "path": desc }))
        }
        Err(e) => Ok(serde_json::json!({ "found": false, "error": e })),
    }
}

/// 列出 Codex 会话（直接调用 Service 层，不经过 RPC）
#[tauri::command]
pub async fn codex_session_list(
    updated_from: Option<i64>,
    updated_to: Option<i64>,
    limit: Option<i64>,
) -> Result<serde_json::Value, String> {
    let db = codexmanager_service::codex_session::default_codex_db_path();
    let options = codexmanager_service::codex_session::SessionListOptions {
        updated_from,
        updated_to,
        limit,
    };
    codexmanager_service::codex_session::list_sessions_with_options(&db, &options)
        .map(|v| serde_json::to_value(v).unwrap_or(serde_json::Value::Array(vec![])))
}

/// 删除单条 Codex 会话
#[tauri::command]
pub async fn codex_session_delete(session_id: String) -> Result<serde_json::Value, String> {
    let db = codexmanager_service::codex_session::default_codex_db_path();
    codexmanager_service::codex_session::delete_session(&db, &session_id)
        .map(|r| serde_json::to_value(r).unwrap_or_default())
}

/// 批量删除 Codex 会话
#[tauri::command]
pub async fn codex_session_delete_many(
    session_ids: Vec<String>,
) -> Result<serde_json::Value, String> {
    let db = codexmanager_service::codex_session::default_codex_db_path();
    let mut results = Vec::with_capacity(session_ids.len());
    for session_id in session_ids {
        let trimmed = session_id.trim();
        if trimmed.is_empty() {
            continue;
        }
        match codexmanager_service::codex_session::delete_session(&db, trimmed) {
            Ok(result) => results.push(result),
            Err(err) => {
                log::warn!("批量删除会话 {trimmed} 失败: {err}");
                results.push(
                    codexmanager_service::codex_session::DeleteResult::backup_failed(trimmed),
                );
            }
        }
    }
    Ok(serde_json::to_value(results).unwrap_or_default())
}

/// 移动单条 Codex 会话到目标工作目录；target_cwd 为空表示移出项目目录
#[tauri::command]
pub async fn codex_session_move(
    session_id: String,
    target_cwd: Option<String>,
) -> Result<serde_json::Value, String> {
    let db = codexmanager_service::codex_session::default_codex_db_path();
    codexmanager_service::codex_session::move_session(&db, &session_id, target_cwd.as_deref())
        .map(|r| serde_json::to_value(r).unwrap_or_default())
}

/// 批量移动 Codex 会话到目标工作目录
#[tauri::command]
pub async fn codex_session_move_many(
    session_ids: Vec<String>,
    target_cwd: Option<String>,
) -> Result<serde_json::Value, String> {
    let db = codexmanager_service::codex_session::default_codex_db_path();
    codexmanager_service::codex_session::move_sessions(&db, &session_ids, target_cwd.as_deref())
        .map(|r| serde_json::to_value(r).unwrap_or_default())
}

/// 删除全部已归档 Codex 会话
#[tauri::command]
pub async fn codex_session_delete_all_archived() -> Result<serde_json::Value, String> {
    let db = codexmanager_service::codex_session::default_codex_db_path();
    codexmanager_service::codex_session::delete_all_archived(&db)
        .map(|r| serde_json::to_value(r).unwrap_or_default())
}

/// 撤销删除
#[tauri::command]
pub async fn codex_session_undo(undo_token: String) -> Result<serde_json::Value, String> {
    let db = codexmanager_service::codex_session::default_codex_db_path();
    codexmanager_service::codex_session::undo_delete(&db, &undo_token)
        .map(|_| serde_json::json!({ "ok": true }))
}

/// 列出归档会话
#[tauri::command]
pub async fn codex_session_list_archived() -> Result<serde_json::Value, String> {
    let db = codexmanager_service::codex_session::default_codex_db_path();
    codexmanager_service::codex_session::list_archived_sessions(&db)
        .map(|v| serde_json::to_value(v).unwrap_or_default())
}

/// 将 Codex 会话元数据的 provider 同步为 CodexManager provider(cm)
#[tauri::command]
pub async fn codex_provider_sync_cm() -> Result<serde_json::Value, String> {
    let db = codexmanager_service::codex_session::default_codex_db_path();
    let home = db
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    codexmanager_service::codex_session::sync_provider_to_cm(&home)
        .map(|v| serde_json::to_value(v).unwrap_or_default())
}

/// 一键写入 ChatGPT 登录态和 CodexManager provider 配置，并同步旧会话 provider
#[tauri::command]
pub async fn codex_configure_cm() -> Result<serde_json::Value, String> {
    codexmanager_service::codex_session::configure_cm_for_codex_app()
        .map(|v| serde_json::to_value(v).unwrap_or_default())
}

/// 检查 Codex App 桥接状态：ChatGPT 登录态、CM provider、手机远程控制开关
#[tauri::command]
pub async fn codex_app_bridge_status() -> Result<serde_json::Value, String> {
    let db = codexmanager_service::codex_session::default_codex_db_path();
    let home = db
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    codexmanager_service::codex_session::codex_app_bridge_status(&home)
        .map(|v| serde_json::to_value(v).unwrap_or_default())
}

/// 启用 Codex App 手机远程控制特性，保留实际模型请求走 CodexManager 网关
#[tauri::command]
pub async fn codex_app_bridge_enable_remote_control() -> Result<serde_json::Value, String> {
    let db = codexmanager_service::codex_session::default_codex_db_path();
    let home = db
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    codexmanager_service::codex_session::enable_codex_mobile_remote_control(&home)
        .map(|v| serde_json::to_value(v).unwrap_or_default())
}
