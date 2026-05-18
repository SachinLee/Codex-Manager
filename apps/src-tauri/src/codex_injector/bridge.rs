use serde::Deserialize;
use serde_json::{json, Value};

use codexmanager_service::codex_session::{self, DeleteStatus};

/// CDP binding 名称（与 renderer-inject.js 一致）
pub const BINDING_NAME: &str = "codexSessionDeleteV2";

/// 原始注入脚本通过 path + payload 格式调用 bridge
#[derive(Debug, Deserialize)]
struct BridgeCall {
    id: String,
    path: String,
    #[serde(default)]
    payload: Value,
}

/// 处理来自 JS 的 bridge 调用（格式：{ id, path, payload }），
/// 返回 (call_id, result_json)，供 cdp_client 回调页面
pub fn handle(raw: &str) -> (String, String) {
    let call: BridgeCall = match serde_json::from_str(raw) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("bridge 调用解析失败: {e}，raw: {raw}");
            return ("0".into(), json_error("invalid_request"));
        }
    };

    log::debug!("bridge: id={} path={}", call.id, call.path);
    let result = dispatch(&call.path, &call.payload);
    (call.id, result)
}

fn dispatch(path: &str, payload: &Value) -> String {
    let db_path = codex_session::default_codex_db_path();

    match path {
        "/delete" => {
            let session_id = extract_session_id(payload);
            match codex_session::delete_session(&db_path, &session_id) {
                Ok(r) => {
                    let (status, message, undo_token) = match r.status {
                        DeleteStatus::Deleted => (
                            "server_deleted",
                            "已删除".to_string(),
                            r.undo_token,
                        ),
                        DeleteStatus::NotFound => (
                            "failed",
                            "会话不存在或已被删除".to_string(),
                            None,
                        ),
                        DeleteStatus::BackupFailed => (
                            "failed",
                            "备份失败，未执行删除".to_string(),
                            None,
                        ),
                    };
                    json!({
                        "status": status,
                        "message": message,
                        "undo_token": undo_token,
                        "session_id": r.session_id,
                    })
                    .to_string()
                }
                Err(e) => json!({ "status": "failed", "message": e }).to_string(),
            }
        }
        "/undo" => {
            let token = payload.get("undo_token")
                .or_else(|| payload.get("undoToken"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match codex_session::undo_delete(&db_path, token) {
                Ok(()) => r#"{"ok":true}"#.to_string(),
                Err(e) => json_error(&e),
            }
        }
        "/archived-thread" => {
            let title = payload.get("title").and_then(|v| v.as_str()).unwrap_or("");
            match codex_session::resolve_archived_by_title(&db_path, title) {
                Ok(Some(sid)) => json!({ "session_id": sid }).to_string(),
                Ok(None) => json!({ "session_id": null }).to_string(),
                Err(e) => json_error(&e),
            }
        }
        // 其他路径（export-markdown、thread-sort-keys 等）返回未实现
        other => {
            log::debug!("bridge: 未实现路径 {other}，返回空成功响应");
            r#"{"status":"not_implemented"}"#.to_string()
        }
    }
}

fn extract_session_id(payload: &Value) -> String {
    payload.get("sessionId")
        .or_else(|| payload.get("session_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn json_error(msg: &str) -> String {
    json!({ "status": "failed", "message": msg }).to_string()
}

/// 构建与 renderer-inject.js 协议兼容的桥接初始化脚本
/// 与原始 Python 中 build_bridge_script() 等价
pub fn build_bridge_setup_script() -> String {
    format!(
        r#"(() => {{
  window.__codexSessionDeleteCallbacks = new Map();
  window.__codexSessionDeleteSeq = 0;
  window.__codexSessionDeleteResolve = (id, result) => {{
    const callback = window.__codexSessionDeleteCallbacks.get(String(id));
    if (!callback) return;
    window.__codexSessionDeleteCallbacks.delete(String(id));
    callback.resolve(result);
  }};
  window.__codexSessionDeleteReject = (id, message) => {{
    const callback = window.__codexSessionDeleteCallbacks.get(String(id));
    if (!callback) return;
    window.__codexSessionDeleteCallbacks.delete(String(id));
    callback.resolve({{ status: "failed", message }});
  }};
  window.__codexSessionDeleteBridge = (path, payload) => new Promise((resolve) => {{
    const id = String(++window.__codexSessionDeleteSeq);
    window.__codexSessionDeleteCallbacks.set(id, {{ resolve }});
    window['{binding}'](JSON.stringify({{ id, path, payload }}));
  }});
}})();"#,
        binding = BINDING_NAME
    )
}
