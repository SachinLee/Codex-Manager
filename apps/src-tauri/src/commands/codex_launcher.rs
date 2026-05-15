use std::sync::{Mutex, OnceLock};

use crate::codex_injector::{
    new_shared_status, start_and_inject, InjectorStatus, SharedStatus,
};

/// 全局注入器状态（单例）
fn global_status() -> &'static Mutex<SharedStatus> {
    static STATUS: OnceLock<Mutex<SharedStatus>> = OnceLock::new();
    STATUS.get_or_init(|| Mutex::new(new_shared_status()))
}

fn get_status() -> InjectorStatus {
    let guard = global_status().lock().unwrap();
    let inner = guard.lock().unwrap().clone();
    inner
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
        let result = start_and_inject(
            custom_path.as_deref(),
            debug_port,
            status_arc,
        );
        if let Err(e) = result {
            log::warn!("Codex 注入器退出: {e}");
        }
    });

    Ok(serde_json::json!({ "ok": true, "debug_port": debug_port }))
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
pub async fn codex_session_list() -> Result<serde_json::Value, String> {
    let db = codexmanager_service::codex_session::default_codex_db_path();
    codexmanager_service::codex_session::list_sessions(&db)
        .map(|v| serde_json::to_value(v).unwrap_or(serde_json::Value::Array(vec![])))
}

/// 删除单条 Codex 会话
#[tauri::command]
pub async fn codex_session_delete(session_id: String) -> Result<serde_json::Value, String> {
    let db = codexmanager_service::codex_session::default_codex_db_path();
    codexmanager_service::codex_session::delete_session(&db, &session_id)
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
