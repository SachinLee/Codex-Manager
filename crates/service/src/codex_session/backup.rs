use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 备份存储目录：~/.codexmanager/codex-session-backups/
pub(super) fn backup_dir() -> PathBuf {
    let base = dirs_or_home();
    base.join(".codexmanager").join("codex-session-backups")
}

fn dirs_or_home() -> PathBuf {
    std::env::var("CODEXMANAGER_CODEX_BACKUP_DIR")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs_sys_home())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn dirs_sys_home() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

/// 单条删除操作的备份令牌
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BackupToken {
    pub token_id: String,
    pub session_id: String,
    pub created_at_ms: u64,
    /// 被删除行的 JSON 快照（按 schema 存入不同键）
    pub rows: serde_json::Value,
    /// rollout_path 对应的文件备份路径（如果有）
    pub rollout_backup_path: Option<String>,
}

impl BackupToken {
    pub fn new(session_id: &str, rows: serde_json::Value, rollout_backup: Option<String>) -> Self {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let token_id = format!("{}-{:x}", now_ms, pseudo_random_suffix());
        Self {
            token_id,
            session_id: session_id.to_string(),
            created_at_ms: now_ms,
            rows,
            rollout_backup_path: rollout_backup,
        }
    }

    pub fn save(&self) -> Result<String, String> {
        let dir = backup_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("创建备份目录失败: {e}"))?;
        let path = dir.join(format!("{}.json", self.token_id));
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("序列化备份令牌失败: {e}"))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("写入备份文件失败: {e}"))?;
        Ok(self.token_id.clone())
    }

    pub fn load(token_id: &str) -> Result<Self, String> {
        let path = backup_dir().join(format!("{token_id}.json"));
        let json = std::fs::read_to_string(&path)
            .map_err(|e| format!("读取备份文件失败: {e}"))?;
        serde_json::from_str(&json)
            .map_err(|e| format!("解析备份令牌失败: {e}"))
    }

    pub fn delete_file(&self) {
        let path = backup_dir().join(format!("{}.json", self.token_id));
        let _ = std::fs::remove_file(path);
    }
}

/// 列出所有备份令牌（按创建时间排序）
pub fn list_backup_tokens() -> Vec<String> {
    let dir = backup_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return vec![];
    };
    let mut tokens: Vec<(u64, String)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".json") {
                let token_id = name.trim_end_matches(".json").to_string();
                let mtime = e.metadata().ok()?.modified().ok()?;
                let ms = mtime.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64;
                Some((ms, token_id))
            } else {
                None
            }
        })
        .collect();
    tokens.sort_by(|a, b| b.0.cmp(&a.0));
    tokens.into_iter().map(|(_, id)| id).collect()
}

/// 清理 30 天前的备份文件
pub fn cleanup_old_backups(max_age_days: u64) {
    let dir = backup_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let cutoff_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
        .saturating_sub(max_age_days * 24 * 3600 * 1000);

    for entry in entries.filter_map(|e| e.ok()) {
        if let Ok(meta) = entry.metadata() {
            if let Ok(mtime) = meta.modified() {
                let ms = mtime
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(u64::MAX);
                if ms < cutoff_ms {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// 将 rollout_path 文件移动到备份目录，返回备份后的路径
pub(super) fn backup_rollout_file(rollout_path: &Path) -> Option<String> {
    if !rollout_path.exists() {
        return None;
    }
    let dir = backup_dir().join("rollout-files");
    std::fs::create_dir_all(&dir).ok()?;
    let file_name = rollout_path.file_name()?.to_string_lossy().to_string();
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let backup_name = format!("{now_ms}_{file_name}");
    let dest = dir.join(&backup_name);
    std::fs::copy(rollout_path, &dest).ok()?;
    Some(dest.to_string_lossy().to_string())
}

/// 还原 rollout_path 文件（从备份覆盖到原始位置）
pub(super) fn restore_rollout_file(backup_path: &str, original_path: &str) -> Result<(), String> {
    std::fs::copy(backup_path, original_path)
        .map(|_| ())
        .map_err(|e| format!("还原 rollout 文件失败: {e}"))
}

fn pseudo_random_suffix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
}
