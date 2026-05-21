import { invoke, withAddr } from "./transport";

export interface InjectorStatus {
  running: boolean;
  injected: boolean;
  debugPort: number | null;
  codexPath: string | null;
  pid: number | null;
}

export interface LaunchOptions {
  customPath?: string;
  debugPort?: number;
}

export interface CodexSession {
  sessionId: string;
  title: string | null;
  createdAt: number | null;
  updatedAt: number | null;
  cwd: string | null;
  archived: boolean | null;
}

export type ProviderSyncStatus = "skipped" | "synced";

export interface ProviderSyncResult {
  status: ProviderSyncStatus;
  targetProvider: string;
  changedSessionFiles: number;
  sqliteRowsUpdated: number;
  backupDir: string | null;
  message: string;
}

export interface CmConfigResult {
  codexHome: string;
  authPath: string;
  configPath: string;
  selectedAccountId: string;
  selectedAccountLabel: string;
  apiKeyId: string;
  apiKeyCreated: boolean;
  authUpdated: boolean;
  configUpdated: boolean;
  backupDir: string | null;
  providerSync: ProviderSyncResult;
}

/** 后端 serde camelCase 序列化：deleted / notFound / backupFailed */
export type DeleteStatus = "deleted" | "notFound" | "backupFailed";

export interface DeleteResult {
  sessionId: string;
  status: DeleteStatus;
  undoToken: string | null;
}

export const codexLauncherClient = {
  async start(opts?: LaunchOptions): Promise<{ ok: boolean; debugPort: number }> {
    return invoke("codex_launcher_start", withAddr({ opts }));
  },

  async startPlain(opts?: Pick<LaunchOptions, "customPath">): Promise<{ ok: boolean }> {
    return invoke("codex_launcher_start_plain", withAddr({ opts }));
  },

  async stop(): Promise<{ ok: boolean }> {
    return invoke("codex_launcher_stop", withAddr({}));
  },

  async status(): Promise<InjectorStatus> {
    return invoke("codex_launcher_status", withAddr({}));
  },

  async resolvePath(customPath?: string): Promise<{ found: boolean; path?: string; error?: string }> {
    return invoke("codex_launcher_resolve_path", withAddr({ customPath }));
  },

  async listSessions(): Promise<CodexSession[]> {
    return invoke("codex_session_list", withAddr({}));
  },

  async deleteSession(sessionId: string): Promise<DeleteResult> {
    return invoke("codex_session_delete", withAddr({ sessionId }));
  },

  async deleteSessions(sessionIds: string[]): Promise<DeleteResult[]> {
    return invoke("codex_session_delete_many", withAddr({ sessionIds }));
  },

  async deleteAllArchivedSessions(): Promise<DeleteResult[]> {
    return invoke("codex_session_delete_all_archived", withAddr({}));
  },

  async undoDelete(undoToken: string): Promise<{ ok: boolean }> {
    return invoke("codex_session_undo", withAddr({ undoToken }));
  },

  async listArchived(): Promise<CodexSession[]> {
    return invoke("codex_session_list_archived", withAddr({}));
  },

  async syncProviderCm(): Promise<ProviderSyncResult> {
    return invoke("codex_provider_sync_cm", withAddr({}));
  },

  async configureCm(): Promise<CmConfigResult> {
    return invoke("codex_configure_cm", withAddr({}));
  },
};
