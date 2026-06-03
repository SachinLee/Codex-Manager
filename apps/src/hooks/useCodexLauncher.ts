"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  codexLauncherClient,
  type CodexSessionListOptions,
  LaunchOptions,
} from "@/lib/api/codex-launcher";

const KEYS = {
  status: ["codex-launcher", "status"] as const,
  appBridgeStatus: ["codex-launcher", "app-bridge-status"] as const,
  sessions: ["codex-launcher", "sessions"] as const,
  archived: ["codex-launcher", "archived"] as const,
};

export function useCodexLauncherStatus() {
  return useQuery({
    queryKey: KEYS.status,
    queryFn: () => codexLauncherClient.status(),
    refetchInterval: 3000,
  });
}

export function useCodexSessions(options?: CodexSessionListOptions) {
  return useQuery({
    queryKey: [...KEYS.sessions, options?.updatedFrom ?? null, options?.updatedTo ?? null, options?.limit ?? null],
    queryFn: () => codexLauncherClient.listSessions(options),
  });
}

export function useCodexArchivedSessions() {
  return useQuery({
    queryKey: KEYS.archived,
    queryFn: () => codexLauncherClient.listArchived(),
  });
}

export function useCodexAppBridgeStatus() {
  return useQuery({
    queryKey: KEYS.appBridgeStatus,
    queryFn: () => codexLauncherClient.appBridgeStatus(),
    refetchInterval: 5000,
  });
}

export function useCodexLauncherStart() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (opts?: LaunchOptions) => codexLauncherClient.start(opts),
    onSuccess: () => {
      toast.success("Codex 启动器已启动，正在注入增强功能...");
      qc.invalidateQueries({ queryKey: KEYS.status });
    },
    onError: (e: Error) => toast.error(`启动失败: ${e.message}`),
  });
}

export function useCodexLauncherStartPlain() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (opts?: Pick<LaunchOptions, "customPath">) =>
      codexLauncherClient.startPlain(opts),
    onSuccess: () => {
      toast.success("Codex 已启动");
      qc.invalidateQueries({ queryKey: KEYS.status });
    },
    onError: (e: Error) => toast.error(`启动失败: ${e.message}`),
  });
}

export function useCodexLauncherStop() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => codexLauncherClient.stop(),
    onSuccess: () => {
      toast.success("Codex 注入器已停止");
      qc.invalidateQueries({ queryKey: KEYS.status });
    },
    onError: (e: Error) => toast.error(`停止失败: ${e.message}`),
  });
}

export function useCodexSessionDelete() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (sessionId: string) => codexLauncherClient.deleteSession(sessionId),
    onSuccess: (result) => {
      if (result.status === "deleted") {
        toast.success("会话已删除", {
          action: result.undoToken
            ? {
                label: "撤销",
                onClick: () => codexLauncherClient.undoDelete(result.undoToken!),
              }
            : undefined,
        });
        qc.invalidateQueries({ queryKey: KEYS.sessions });
      } else if (result.status === "notFound") {
        toast.error("会话不存在或已被删除");
      } else if (result.status === "backupFailed") {
        toast.error("备份失败，未执行删除");
      } else {
        toast.error(`删除失败: ${result.status}`);
      }
    },
    onError: (e: Error) => toast.error(`删除失败: ${e.message}`),
  });
}

export function useCodexSessionsDeleteMany() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (sessionIds: string[]) => codexLauncherClient.deleteSessions(sessionIds),
    onSuccess: (results) => {
      const deleted = results.filter((result) => result.status === "deleted").length;
      const failed = results.length - deleted;
      if (failed > 0) {
        toast.warning(`已删除 ${deleted} 个会话，${failed} 个未删除`);
      } else {
        toast.success(`已删除 ${deleted} 个会话`);
      }
      qc.invalidateQueries({ queryKey: KEYS.sessions });
      qc.invalidateQueries({ queryKey: KEYS.archived });
    },
    onError: (e: Error) => toast.error(`批量删除失败: ${e.message}`),
  });
}

export function useCodexSessionMove() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      sessionId,
      targetCwd,
    }: {
      sessionId: string;
      targetCwd: string | null;
    }) => codexLauncherClient.moveSession(sessionId, targetCwd),
    onSuccess: (result) => {
      if (result.status === "moved") {
        toast.success("会话已移动");
      } else if (result.status === "unchanged") {
        toast.info("会话已在目标目录中");
      } else if (result.status === "notFound") {
        toast.error("会话不存在或已被删除");
      } else {
        toast.error("当前 Codex 会话结构不支持移动");
      }
      qc.invalidateQueries({ queryKey: KEYS.sessions });
      qc.invalidateQueries({ queryKey: KEYS.archived });
    },
    onError: (e: Error) => toast.error(`移动失败: ${e.message}`),
  });
}

export function useCodexSessionsMoveMany() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      sessionIds,
      targetCwd,
    }: {
      sessionIds: string[];
      targetCwd: string | null;
    }) => codexLauncherClient.moveSessions(sessionIds, targetCwd),
    onSuccess: (results) => {
      const moved = results.filter((result) => result.status === "moved").length;
      const unchanged = results.filter((result) => result.status === "unchanged").length;
      const failed = results.length - moved - unchanged;
      if (failed > 0) {
        toast.warning(`已移动 ${moved} 个会话，${failed} 个未移动`);
      } else if (moved > 0) {
        toast.success(`已移动 ${moved} 个会话`);
      } else {
        toast.info("选中会话已在目标目录中");
      }
      qc.invalidateQueries({ queryKey: KEYS.sessions });
      qc.invalidateQueries({ queryKey: KEYS.archived });
    },
    onError: (e: Error) => toast.error(`批量移动失败: ${e.message}`),
  });
}

export function useCodexArchivedSessionsDeleteAll() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => codexLauncherClient.deleteAllArchivedSessions(),
    onSuccess: (results) => {
      const deleted = results.filter((result) => result.status === "deleted").length;
      const failed = results.length - deleted;
      if (failed > 0) {
        toast.warning(`已删除 ${deleted} 个已归档会话，${failed} 个未删除`);
      } else {
        toast.success(`已删除 ${deleted} 个已归档会话`);
      }
      qc.invalidateQueries({ queryKey: KEYS.sessions });
      qc.invalidateQueries({ queryKey: KEYS.archived });
    },
    onError: (e: Error) => toast.error(`删除已归档会话失败: ${e.message}`),
  });
}

export function useCodexConfigureCm() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => codexLauncherClient.configureCm(),
    onSuccess: () => {
      toast.success("Codex App 桥接配置已写入");
      qc.invalidateQueries({ queryKey: KEYS.appBridgeStatus });
    },
    onError: (e: Error) => toast.error(`配置失败: ${e.message}`),
  });
}

export function useCodexEnableRemoteControl() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => codexLauncherClient.enableRemoteControl(),
    onSuccess: (result) => {
      if (result.dbUpdated || result.configUpdated) {
        toast.success("手机远程控制已启用");
      } else {
        toast.info("手机远程控制已是启用状态");
      }
      qc.invalidateQueries({ queryKey: KEYS.appBridgeStatus });
    },
    onError: (e: Error) => toast.error(`启用手机远控失败: ${e.message}`),
  });
}
