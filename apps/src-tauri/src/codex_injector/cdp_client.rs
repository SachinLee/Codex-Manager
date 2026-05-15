use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tungstenite::{connect as tungstenite_connect, Message as TungMsg};

static MSG_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
    MSG_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, serde::Deserialize)]
struct CdpTarget {
    #[serde(rename = "type")]
    kind: String,
    url: String,
    title: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    ws_url: Option<String>,
}

/// 轮询 CDP targets，找到 Codex 渲染页面的 WebSocket URL
pub fn find_codex_page(debug_port: u16) -> Option<String> {
    let url = format!("http://127.0.0.1:{debug_port}/json");
    let resp = reqwest::blocking::get(&url).ok()?;
    let targets: Vec<CdpTarget> = resp.json().ok()?;

    targets
        .into_iter()
        .filter(|t| t.kind == "page")
        .find(|t| {
            let lower_url = t.url.to_lowercase();
            let lower_title = t.title.to_lowercase();
            // 优先匹配 codex 相关页面；blank 页面作为兜底
            lower_url.contains("codex")
                || lower_title.contains("codex")
                || lower_url == "about:blank"
        })
        .and_then(|t| t.ws_url)
}

/// 等待 Codex 调试端口就绪（最多 retry_times 次，每次间隔 interval_ms）
pub fn wait_for_debug_port(debug_port: u16, retry_times: u32, interval_ms: u64) -> bool {
    for i in 0..retry_times {
        std::thread::sleep(Duration::from_millis(interval_ms));
        let url = format!("http://127.0.0.1:{debug_port}/json");
        if reqwest::blocking::get(&url).is_ok() {
            log::info!("CDP 端口 {debug_port} 就绪（第 {} 次）", i + 1);
            return true;
        }
    }
    log::warn!("CDP 端口 {debug_port} 超时未就绪（{retry_times} 次）");
    false
}

type WsConn = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>;

/// 完整注入流程：
/// 1. 连接 WebSocket
/// 2. Runtime.enable / addBinding / 桥接初始化脚本 / 主注入脚本
/// 3. 进入消息循环，处理 bindingCalled 事件
pub fn run_injection(
    ws_url: &str,
    binding_name: &str,
    bridge_setup_script: &str,
    inject_script: &str,
    bridge_handler: impl Fn(&str) -> (String, String) + Send + 'static,
) -> Result<(), String> {
    let (mut socket, _) = tungstenite_connect(ws_url)
        .map_err(|e| format!("CDP WebSocket 连接失败: {e}"))?;

    // --- 初始化序列 ---
    let id = cdp_send(&mut socket, "Runtime.enable", json!({}))?;
    cdp_recv_result(&mut socket, id)?;

    // 移除旧 binding 避免重复注入残留
    let id = cdp_send(
        &mut socket,
        "Runtime.removeBinding",
        json!({ "name": binding_name }),
    )?;
    cdp_recv_result(&mut socket, id)?;

    let id = cdp_send(
        &mut socket,
        "Runtime.addBinding",
        json!({ "name": binding_name }),
    )?;
    cdp_recv_result(&mut socket, id)?;

    // 注册到新文档脚本（页面刷新后自动重新注入桥接）
    let id = cdp_send(
        &mut socket,
        "Page.addScriptToEvaluateOnNewDocument",
        json!({ "source": bridge_setup_script }),
    )?;
    cdp_recv_result(&mut socket, id)?;

    // 立即在当前页面执行桥接初始化
    let id = cdp_send(
        &mut socket,
        "Runtime.evaluate",
        json!({
            "expression": bridge_setup_script,
            "awaitPromise": false,
            "allowUnsafeEvalBlockedByCSP": true,
        }),
    )?;
    cdp_recv_result(&mut socket, id)?;

    // 注入主功能脚本
    let id = cdp_send(
        &mut socket,
        "Runtime.evaluate",
        json!({
            "expression": inject_script,
            "awaitPromise": false,
            "allowUnsafeEvalBlockedByCSP": true,
        }),
    )?;
    cdp_recv_result(&mut socket, id)?;

    log::info!("CDP 注入完成，进入消息循环（binding: {binding_name}）");

    // --- 消息循环 ---
    loop {
        let text = match socket.read() {
            Ok(TungMsg::Text(t)) => t,
            Ok(TungMsg::Close(_)) => {
                log::info!("CDP 连接被关闭");
                break;
            }
            Ok(_) => continue,
            Err(e) => {
                log::warn!("CDP 消息循环读取失败: {e}");
                break;
            }
        };

        let Ok(val) = serde_json::from_str::<Value>(&text) else {
            continue;
        };

        if val.get("method").and_then(|v| v.as_str()) != Some("Runtime.bindingCalled") {
            continue;
        }

        let params = val.get("params").cloned().unwrap_or(Value::Null);
        if params.get("name").and_then(|v| v.as_str()) != Some(binding_name) {
            continue;
        }

        let raw_payload = params
            .get("payload")
            .and_then(|v| v.as_str())
            .unwrap_or("{}");

        let (call_id, result_json) = bridge_handler(raw_payload);

        // 回调页面：window.__codexSessionDeleteResolve(id, result)
        let callback = format!(
            "window.__codexSessionDeleteResolve({id}, {result});",
            id = serde_json::to_string(&call_id).unwrap_or_else(|_| "\"0\"".to_string()),
            result = result_json,
        );

        if let Ok(eid) = cdp_send(
            &mut socket,
            "Runtime.evaluate",
            json!({
                "expression": callback,
                "allowUnsafeEvalBlockedByCSP": true,
            }),
        ) {
            // 只消耗响应，不检查结果
            let _ = cdp_recv_result(&mut socket, eid);
        }
    }

    Ok(())
}

fn cdp_send(socket: &mut WsConn, method: &str, params: Value) -> Result<u64, String> {
    let id = next_id();
    let msg = json!({ "id": id, "method": method, "params": params });
    socket
        .send(TungMsg::Text(msg.to_string()))
        .map_err(|e| format!("CDP 发送 {method} 失败: {e}"))?;
    Ok(id)
}

fn cdp_recv_result(socket: &mut WsConn, expected_id: u64) -> Result<Value, String> {
    for _ in 0..200usize {
        match socket.read() {
            Ok(TungMsg::Text(text)) => {
                if let Ok(val) = serde_json::from_str::<Value>(&text) {
                    if val.get("id").and_then(|v| v.as_u64()) == Some(expected_id) {
                        return Ok(val);
                    }
                }
            }
            Ok(TungMsg::Close(_)) => return Err("CDP 连接在等待响应期间关闭".to_string()),
            Err(e) => return Err(format!("CDP 读取失败: {e}")),
            _ => {}
        }
    }
    Err(format!("等待 CDP 响应 id={expected_id} 超时"))
}
