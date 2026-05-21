use std::sync::{Arc, Mutex};

use super::app_paths::CodexInstallKind;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InjectorStatus {
    pub running: bool,
    pub injected: bool,
    pub debug_port: Option<u16>,
    pub codex_path: Option<String>,
    /// 进程 PID（普通 EXE 模式）
    pub pid: Option<u32>,
}

impl Default for InjectorStatus {
    fn default() -> Self {
        Self {
            running: false,
            injected: false,
            debug_port: None,
            codex_path: None,
            pid: None,
        }
    }
}

/// 全局注入器状态（Arc<Mutex> 供跨线程访问）
pub type SharedStatus = Arc<Mutex<InjectorStatus>>;

pub fn new_shared_status() -> SharedStatus {
    Arc::new(Mutex::new(InjectorStatus::default()))
}

/// 在可用范围内找一个空闲 TCP 端口（从 preferred 开始尝试）
pub fn pick_free_port(preferred: u16) -> u16 {
    for port in preferred..preferred + 100 {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    preferred
}

/// 普通启动 Codex 进程，返回 PID（UWP 模式可能返回 None）
pub fn launch_codex_plain(kind: &CodexInstallKind) -> Result<Option<u32>, String> {
    match kind {
        #[cfg(target_os = "macos")]
        CodexInstallKind::Exe(path) if path.extension().map_or(false, |e| e == "app") => {
            log::info!("普通启动 Codex.app: {}", path.display());
            std::process::Command::new("open")
                .arg("-a")
                .arg(path)
                .spawn()
                .map_err(|e| format!("open -a Codex 失败: {e}"))?;
            Ok(None)
        }

        CodexInstallKind::Exe(path) => {
            log::info!("普通启动 Codex EXE: {}", path.display());
            let child = std::process::Command::new(path)
                .spawn()
                .map_err(|e| format!("启动 Codex 失败: {e}"))?;
            Ok(Some(child.id()))
        }

        #[cfg(target_os = "windows")]
        CodexInstallKind::Uwp { app_user_model_id } => launch_uwp_plain(app_user_model_id),
    }
}

/// 启动 Codex 进程并开启调试端口，返回 PID（UWP 模式返回 None）
pub fn launch_codex(kind: &CodexInstallKind, debug_port: u16) -> Result<Option<u32>, String> {
    match kind {
        #[cfg(target_os = "macos")]
        CodexInstallKind::Exe(path) if path.extension().map_or(false, |e| e == "app") => {
            // macOS .app bundle：用 `open -a` 传参
            log::info!("以调试端口 {debug_port} 启动 Codex.app: {}", path.display());
            std::process::Command::new("open")
                .arg("-a")
                .arg(path)
                .arg("--args")
                .arg(format!("--remote-debugging-port={debug_port}"))
                .arg(format!(
                    "--remote-allow-origins=http://127.0.0.1:{debug_port}"
                ))
                .spawn()
                .map_err(|e| format!("open -a Codex 失败: {e}"))?;
            Ok(None) // open 命令启动后无法直接获得 Codex 的 PID
        }

        CodexInstallKind::Exe(path) => {
            log::info!("以调试端口 {debug_port} 启动 Codex EXE: {}", path.display());
            let child = std::process::Command::new(path)
                .arg(format!("--remote-debugging-port={debug_port}"))
                .arg(format!(
                    "--remote-allow-origins=http://127.0.0.1:{debug_port}"
                ))
                .spawn()
                .map_err(|e| format!("启动 Codex 失败: {e}"))?;
            Ok(Some(child.id()))
        }

        #[cfg(target_os = "windows")]
        CodexInstallKind::Uwp { app_user_model_id } => launch_uwp(app_user_model_id, debug_port),
    }
}

/// 停止指定 PID 的进程（仅 EXE 模式下有效）
pub fn stop_process(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
    }
    #[cfg(not(target_os = "windows"))]
    {
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }
}

/// 检查 Codex 进程是否仍在运行（按 PID）
pub fn is_process_alive(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        // SYNCHRONIZE = 0x00100000
        const SYNCHRONIZE: u32 = 0x00100000;
        let handle =
            unsafe { windows_sys::Win32::System::Threading::OpenProcess(SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            return false;
        }
        let still_running =
            unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(handle, 0) }
                == windows_sys::Win32::Foundation::WAIT_TIMEOUT;
        unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
        still_running
    }
    #[cfg(not(target_os = "windows"))]
    {
        // kill(pid, 0) 不发信号，只检查进程是否存在
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }
}

/// 启动前清理已经存在的 Codex 残留进程。
/// 必须这么做的原因：如果之前的 Codex 是在没开 CDP 端口的状态下启动的，
/// 再次 ActivateApplication 只会复用已存在进程（不会用新参数重启），
/// 导致调试端口永远开不起来——表现为「Codex 唤醒但白屏 + 未注入」。
#[cfg(target_os = "windows")]
pub fn kill_existing_codex() {
    let ps_cmd = r#"
Get-CimInstance Win32_Process -Filter "Name = 'Codex.exe'" -ErrorAction SilentlyContinue |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
"#;
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_cmd])
        .output();
}

#[cfg(not(target_os = "windows"))]
pub fn kill_existing_codex() {}

/// Windows UWP 通过 COM `IApplicationActivationManager::ActivateApplication`
/// 激活并传递命令行参数。这是 UWP 唯一能传命令行参数的方式；
/// `Shell.Application.ShellExecute` 不行——参数会被静默丢弃。
#[cfg(target_os = "windows")]
fn launch_uwp(app_user_model_id: &str, debug_port: u16) -> Result<Option<u32>, String> {
    let args = format!(
        "--remote-debugging-port={debug_port} --remote-allow-origins=http://127.0.0.1:{debug_port}"
    );
    log::info!("激活 UWP 包: {app_user_model_id}, 调试端口: {debug_port}");

    // 先清理可能存在的旧 Codex 进程，否则 ActivateApplication 会复用旧实例
    kill_existing_codex();
    std::thread::sleep(std::time::Duration::from_millis(300));

    launch_uwp_with_args(app_user_model_id, Some(args.as_str()))
}

#[cfg(target_os = "windows")]
fn launch_uwp_plain(app_user_model_id: &str) -> Result<Option<u32>, String> {
    log::info!("普通激活 UWP 包: {app_user_model_id}");
    launch_uwp_with_args(app_user_model_id, None)
}

#[cfg(target_os = "windows")]
fn launch_uwp_with_args(
    app_user_model_id: &str,
    args: Option<&str>,
) -> Result<Option<u32>, String> {
    use std::ffi::c_void;
    use windows_sys::core::{GUID, HRESULT};
    use windows_sys::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_LOCAL_SERVER,
        COINIT_APARTMENTTHREADED,
    };

    // IApplicationActivationManager 的 vtable 布局（顺序严格按 shobjidl.h）
    #[repr(C)]
    struct IAamVtbl {
        query_interface:
            unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        activate_application: unsafe extern "system" fn(
            *mut c_void,
            *const u16, // appUserModelId (LPCWSTR)
            *const u16, // arguments (LPCWSTR, 可空)
            u32,        // ACTIVATEOPTIONS
            *mut u32,   // processId
        ) -> HRESULT,
        activate_for_file: *const c_void,
        activate_for_protocol: *const c_void,
    }

    // CLSID_ApplicationActivationManager = {45BA127D-10A8-46EA-8AB7-56EA9078943C}
    const CLSID_AAM: GUID = GUID {
        data1: 0x45BA127D,
        data2: 0x10A8,
        data3: 0x46EA,
        data4: [0x8A, 0xB7, 0x56, 0xEA, 0x90, 0x78, 0x94, 0x3C],
    };
    // IID_IApplicationActivationManager = {2E941141-7F97-4756-BA1D-9DECDE894A3D}
    const IID_AAM: GUID = GUID {
        data1: 0x2E941141,
        data2: 0x7F97,
        data3: 0x4756,
        data4: [0xBA, 0x1D, 0x9D, 0xEC, 0xDE, 0x89, 0x4A, 0x3D],
    };
    const AO_NONE: u32 = 0;

    // 转 UTF-16 宽字符串（末尾 NUL），生命周期必须覆盖 ActivateApplication 调用
    let aumid_w: Vec<u16> = app_user_model_id
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let args_w: Option<Vec<u16>> =
        args.map(|s| s.encode_utf16().chain(std::iter::once(0)).collect());
    let args_ptr = args_w
        .as_ref()
        .map_or(std::ptr::null(), |value| value.as_ptr());

    unsafe {
        // ApplicationActivationManager 必须 STA 单元化
        let init_hr = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        // S_OK = 0, S_FALSE = 1 (已初始化), RPC_E_CHANGED_MODE 等情况都继续尝试
        if init_hr < 0 {
            log::warn!(
                "CoInitializeEx 返回 HRESULT 0x{:08X}（继续尝试激活）",
                init_hr as u32
            );
        }

        let mut mgr: *mut c_void = std::ptr::null_mut();
        let hr = CoCreateInstance(
            &CLSID_AAM,
            std::ptr::null_mut(),
            CLSCTX_LOCAL_SERVER,
            &IID_AAM,
            &mut mgr,
        );
        if hr < 0 || mgr.is_null() {
            CoUninitialize();
            let msg = format!(
                "CoCreateInstance(ApplicationActivationManager) 失败 HRESULT=0x{:08X}",
                hr as u32
            );
            log::error!("{msg}");
            return Err(msg);
        }

        let vtbl_ptr = *(mgr as *mut *const IAamVtbl);
        let activate = (*vtbl_ptr).activate_application;
        let release = (*vtbl_ptr).release;

        let mut pid: u32 = 0;
        let hr = activate(mgr, aumid_w.as_ptr(), args_ptr, AO_NONE, &mut pid);
        release(mgr);
        CoUninitialize();

        if hr < 0 {
            let msg = format!("ActivateApplication 失败 HRESULT=0x{:08X}", hr as u32);
            log::error!("{msg}");
            return Err(msg);
        }

        log::info!("UWP Codex 激活成功 (COM ActivateApplication), PID: {pid}");
        Ok(if pid == 0 { None } else { Some(pid) })
    }
}
