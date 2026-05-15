use codexmanager_core::rpc::types::{JsonRpcRequest, JsonRpcResponse};

use crate::codex_session;

fn db_path() -> std::path::PathBuf {
    codex_session::default_codex_db_path()
}

pub(super) fn try_handle(req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    let result = match req.method.as_str() {
        "codexSession/list" => {
            super::value_or_error(codex_session::list_sessions(&db_path()))
        }
        "codexSession/listArchived" => {
            super::value_or_error(codex_session::list_archived_sessions(&db_path()))
        }
        "codexSession/delete" => {
            let session_id = super::str_param(req, "sessionId").unwrap_or("");
            super::value_or_error(codex_session::delete_session(&db_path(), session_id))
        }
        "codexSession/undo" => {
            let token = super::str_param(req, "undoToken").unwrap_or("");
            super::ok_or_error(codex_session::undo_delete(&db_path(), token))
        }
        "codexSession/resolveArchivedTitle" => {
            let title = super::str_param(req, "title").unwrap_or("");
            super::value_or_error(codex_session::resolve_archived_by_title(&db_path(), title))
        }
        "codexSession/deleteAllArchived" => {
            super::value_or_error(codex_session::delete_all_archived(&db_path()))
        }
        "codexSession/listBackupTokens" => {
            super::as_json(codex_session::list_backup_tokens()).into()
        }
        _ => return None,
    };

    Some(super::response(req, result))
}
