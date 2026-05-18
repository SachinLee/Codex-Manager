"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { codexLauncherClient, LaunchOptions } from "@/lib/api/codex-launcher";

const KEYS = {
  status: ["codex-launcher", "status"] as const,
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

export function useCodexSessions() {
  return useQuery({
    queryKey: KEYS.sessions,
    queryFn: () => codexLauncherClient.listSessions(),
  });
}

export function useCodexArchivedSessions() {
  return useQuery({
    queryKey: KEYS.archived,
    queryFn: () => codexLauncherClient.listArchived(),
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
