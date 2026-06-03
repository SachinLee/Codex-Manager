use codexmanager_core::rpc::types::{JsonRpcRequest, JsonRpcResponse};

use crate::codex_session;

fn db_path() -> std::path::PathBuf {
    codex_session::default_codex_db_path()
}

fn codex_home() -> std::path::PathBuf {
    db_path()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

pub(super) fn try_handle(req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    let result = match req.method.as_str() {
        "codexSession/list" => {
            let options = codex_session::SessionListOptions {
                updated_from: super::i64_param(req, "updatedFrom"),
                updated_to: super::i64_param(req, "updatedTo"),
                limit: super::i64_param(req, "limit"),
            };
            super::value_or_error(codex_session::list_sessions_with_options(
                &db_path(),
                &options,
            ))
        }
        "codexSession/listArchived" => {
            super::value_or_error(codex_session::list_archived_sessions(&db_path()))
        }
        "codexSession/delete" => {
            let session_id = super::str_param(req, "sessionId").unwrap_or("");
            super::value_or_error(codex_session::delete_session(&db_path(), session_id))
        }
        "codexSession/deleteMany" => {
            let session_ids = req
                .params
                .as_ref()
                .and_then(|params| params.get("sessionIds"))
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let db = db_path();
            let mut results = Vec::with_capacity(session_ids.len());
            for session_id in session_ids {
                match codex_session::delete_session(&db, session_id.as_str()) {
                    Ok(result) => results.push(result),
                    Err(err) => {
                        log::warn!("批量删除会话 {} 失败: {err}", session_id);
                        results.push(codex_session::DeleteResult::backup_failed(session_id));
                    }
                }
            }
            super::as_json(results).into()
        }
        "codexSession/move" => {
            let session_id = super::str_param(req, "sessionId").unwrap_or("");
            let target_cwd = super::str_param(req, "targetCwd");
            super::value_or_error(codex_session::move_session(
                &db_path(),
                session_id,
                target_cwd,
            ))
        }
        "codexSession/moveMany" => {
            let session_ids = req
                .params
                .as_ref()
                .and_then(|params| params.get("sessionIds"))
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let target_cwd = super::str_param(req, "targetCwd");
            super::value_or_error(codex_session::move_sessions(
                &db_path(),
                &session_ids,
                target_cwd,
            ))
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
        "codexSession/providerSyncCm" => {
            super::value_or_error(codex_session::sync_provider_to_cm(&codex_home()))
        }
        "codexSession/configureCm" => {
            super::value_or_error(codex_session::configure_cm_for_codex_app())
        }
        "codexSession/appBridgeStatus" => {
            super::value_or_error(codex_session::codex_app_bridge_status(&codex_home()))
        }
        "codexSession/enableRemoteControl" => super::value_or_error(
            codex_session::enable_codex_mobile_remote_control(&codex_home()),
        ),
        _ => return None,
    };

    Some(super::response(req, result))
}
