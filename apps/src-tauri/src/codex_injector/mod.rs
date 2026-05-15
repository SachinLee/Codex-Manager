mod app_paths;
mod bridge;
mod cdp_client;
mod process;

pub use app_paths::{describe_codex_path, find_codex};
pub use bridge::build_bridge_setup_script;
pub use cdp_client::{find_codex_page, run_injection, wait_for_debug_port};
pub use process::{new_shared_status, stop_process, InjectorStatus, SharedStatus};

/// 注入脚本（编译时内嵌，确保资产不丢失）
const INJECT_SCRIPT: &str = include_str!("../../assets/renderer-inject.js");

/// 主入口：启动 Codex 并注入增强功能
///
/// 设计：在独立线程中运行，通过 SharedStatus 向外汇报状态。
/// Codex 关闭或 CDP 断连后函数返回，调用方可选择重新注入。
pub fn start_and_inject(
    custom_path: Option<&str>,
    debug_port: u16,
    status: SharedStatus,
) -> Result<(), String> {
    // 1. 找到 Codex 安装路径
    let install_kind = find_codex(custom_path)?;
    let path_desc = describe_codex_path(&install_kind);
    log::info!("Codex 路径: {path_desc}");

    {
        let mut s = status.lock().unwrap();
        s.codex_path = Some(path_desc.clone());
    }

    // 2. 启动 Codex 进程
    let pid = process::launch_codex(&install_kind, debug_port)?;
    log::info!("Codex 进程已启动，PID: {pid:?}，调试端口: {debug_port}");

    {
        let mut s = status.lock().unwrap();
        s.running = true;
        s.debug_port = Some(debug_port);
        s.pid = pid;
    }

    // 3. 等待调试端口就绪（最多 30 次 × 500ms = 15s）
    if !wait_for_debug_port(debug_port, 30, 500) {
        let mut s = status.lock().unwrap();
        s.running = false;
        return Err(format!("Codex 调试端口 {debug_port} 15 秒内未就绪"));
    }

    // 4. 找到渲染页面的 WebSocket URL（最多重试 10 次）
    let ws_url = retry_find_page(debug_port, 10, 800)?;
    log::info!("找到 Codex 渲染页面 WS: {ws_url}");

    {
        let mut s = status.lock().unwrap();
        s.injected = true;
    }

    // 5. 执行注入（阻塞直到 CDP 断连）
    let bridge_setup = build_bridge_setup_script();

    let result = run_injection(
        &ws_url,
        bridge::BINDING_NAME,
        &bridge_setup,
        INJECT_SCRIPT,
        bridge::handle,
    );

    {
        let mut s = status.lock().unwrap();
        s.running = false;
        s.injected = false;
        s.pid = None;
    }

    result
}

fn retry_find_page(debug_port: u16, retries: u32, interval_ms: u64) -> Result<String, String> {
    for i in 0..retries {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(interval_ms));
        }
        if let Some(ws_url) = find_codex_page(debug_port) {
            return Ok(ws_url);
        }
        log::debug!("未找到 Codex 页面，第 {} 次重试", i + 1);
    }
    Err(format!(
        "未找到 Codex 渲染页面（重试 {retries} 次），请确认 Codex 已完全启动"
    ))
}
