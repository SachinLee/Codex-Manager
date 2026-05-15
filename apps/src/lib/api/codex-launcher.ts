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
}

export interface DeleteResult {
  sessionId: string;
  status: "Deleted" | "NotFound" | "BackupFailed";
  undoToken: string | null;
}

export const codexLauncherClient = {
  async start(opts?: LaunchOptions): Promise<{ ok: boolean; debugPort: number }> {
    return invoke("codex_launcher_start", withAddr({ opts }));
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

  async undoDelete(undoToken: string): Promise<{ ok: boolean }> {
    return invoke("codex_session_undo", withAddr({ undoToken }));
  },

  async listArchived(): Promise<CodexSession[]> {
    return invoke("codex_session_list_archived", withAddr({}));
  },
};
