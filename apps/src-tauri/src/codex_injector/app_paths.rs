use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum CodexInstallKind {
    /// 普通 EXE / .app，直接 spawn
    Exe(PathBuf),
    /// Windows UWP Store 包，需要 COM 激活
    #[cfg(target_os = "windows")]
    Uwp { app_user_model_id: String },
}

/// 探测 Codex 安装位置，优先级：
/// 1. 用户在设置里手动指定的路径
/// 2. Windows UWP (Get-AppxPackage)
/// 3. Windows 传统 EXE (~\AppData\Local\Programs\Codex)
/// 4. macOS /Applications/Codex.app
pub fn find_codex(custom_path: Option<&str>) -> Result<CodexInstallKind, String> {
    // 用户自定义路径优先
    if let Some(p) = custom_path.filter(|s| !s.is_empty()) {
        let path = PathBuf::from(p);
        if path.exists() {
            return Ok(CodexInstallKind::Exe(path));
        }
        return Err(format!("指定的 Codex 路径不存在: {p}"));
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(kind) = find_codex_windows() {
            return Ok(kind);
        }
        return Err(
            "未找到 Codex 安装（尝试了 UWP 包和 %LOCALAPPDATA%\\Programs\\Codex）".to_string(),
        );
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(path) = find_codex_macos() {
            return Ok(CodexInstallKind::Exe(path));
        }
        return Err("未找到 Codex.app（尝试了 /Applications 等目录）".to_string());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Err("当前平台暂不支持自动探测 Codex 路径".to_string())
    }
}

#[cfg(target_os = "windows")]
fn find_codex_windows() -> Option<CodexInstallKind> {
    // 先尝试 UWP
    if let Some(model_id) = find_uwp_app_user_model_id() {
        log::info!("找到 Codex UWP 包，AppUserModelId: {model_id}");
        return Some(CodexInstallKind::Uwp {
            app_user_model_id: model_id,
        });
    }

    // 再尝试传统 EXE
    if let Some(exe) = find_exe_install() {
        log::info!("找到 Codex EXE 安装: {}", exe.display());
        return Some(CodexInstallKind::Exe(exe));
    }

    None
}

/// 通过 PowerShell 查找 UWP 包，返回 AppUserModelId
#[cfg(target_os = "windows")]
fn find_uwp_app_user_model_id() -> Option<String> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            r#"
$pkg = Get-AppxPackage -Name 'OpenAI.Codex' -ErrorAction SilentlyContinue |
       Sort-Object Version -Descending | Select-Object -First 1
if ($pkg) { Write-Output ($pkg.PackageFamilyName + '!App') }
"#,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// 查找传统 EXE 安装（%LOCALAPPDATA%\Programs\Codex\Codex.exe）
#[cfg(target_os = "windows")]
fn find_exe_install() -> Option<PathBuf> {
    let local_app_data = std::env::var("LOCALAPPDATA").ok()?;
    let candidate = PathBuf::from(local_app_data)
        .join("Programs")
        .join("Codex")
        .join("Codex.exe");
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn find_codex_macos() -> Option<PathBuf> {
    let candidates = [
        "/Applications/Codex.app/Contents/MacOS/Codex",
        "/Applications/Codex.app",
    ];
    for c in &candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return Some(p);
        }
    }
    // 尝试用户目录
    if let Ok(home) = std::env::var("HOME") {
        let user_app = PathBuf::from(&home)
            .join("Applications")
            .join("Codex.app");
        if user_app.exists() {
            return Some(user_app);
        }
    }
    None
}

/// 返回当前平台下 Codex 可执行文件的人类可读路径描述（用于 UI 展示）
pub fn describe_codex_path(kind: &CodexInstallKind) -> String {
    match kind {
        CodexInstallKind::Exe(p) => p.to_string_lossy().to_string(),
        #[cfg(target_os = "windows")]
        CodexInstallKind::Uwp { app_user_model_id } => {
            format!("UWP: {app_user_model_id}")
        }
    }
}
