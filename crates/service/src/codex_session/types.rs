use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeleteStatus {
    Deleted,
    NotFound,
    BackupFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    pub session_id: String,
    pub title: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    /// 会话所属工作目录（threads 表 `cwd` 列）；generic 模式下为 None
    #[serde(default)]
    pub cwd: Option<String>,
    /// 是否归档（threads 表 `archived` 列）；generic 模式下为 None
    #[serde(default)]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    pub session_id: String,
    pub status: DeleteStatus,
    /// 删除成功时生成的撤销令牌，用于 undo 操作
    pub undo_token: Option<String>,
}

impl DeleteResult {
    pub fn deleted(session_id: impl Into<String>, undo_token: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            status: DeleteStatus::Deleted,
            undo_token: Some(undo_token.into()),
        }
    }

    pub fn not_found(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            status: DeleteStatus::NotFound,
            undo_token: None,
        }
    }

    pub fn backup_failed(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            status: DeleteStatus::BackupFailed,
            undo_token: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SchemaKind {
    /// Codex 原生 schema：codex_threads 表 + rollout_path 字段
    CodexThreads,
    /// 通用 schema：sessions + messages 表
    GenericSessions,
}
