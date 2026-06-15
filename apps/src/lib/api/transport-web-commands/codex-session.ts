import type { WebCommandDescriptor } from "./shared";

export function createCodexSessionWebCommands(): Record<string, WebCommandDescriptor> {
  return {
    codex_session_list: { rpcMethod: "codexSession/list" },
    codex_session_delete: { rpcMethod: "codexSession/delete" },
    codex_session_delete_many: { rpcMethod: "codexSession/deleteMany" },
    codex_session_move: { rpcMethod: "codexSession/move" },
    codex_session_move_many: { rpcMethod: "codexSession/moveMany" },
    codex_session_delete_all_archived: {
      rpcMethod: "codexSession/deleteAllArchived",
    },
    codex_session_undo: { rpcMethod: "codexSession/undo" },
    codex_session_list_archived: { rpcMethod: "codexSession/listArchived" },
    codex_configure_cm: { rpcMethod: "codexSession/configureCm" },
    codex_provider_sync_cm: { rpcMethod: "codexSession/providerSyncCm" },
    codex_app_bridge_status: { rpcMethod: "codexSession/appBridgeStatus" },
    codex_app_bridge_enable_remote_control: {
      rpcMethod: "codexSession/enableRemoteControl",
    },
  };
}
