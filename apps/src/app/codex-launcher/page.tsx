"use client";

import { useState } from "react";
import {
  Rocket,
  Square,
  RefreshCw,
  Trash2,
  Undo2,
  FolderOpen,
  Zap,
  Circle,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  useCodexLauncherStatus,
  useCodexLauncherStart,
  useCodexLauncherStop,
  useCodexSessions,
  useCodexSessionDelete,
} from "@/hooks/useCodexLauncher";
import { codexLauncherClient } from "@/lib/api/codex-launcher";

function StatusDot({ active }: { active: boolean }) {
  return (
    <span
      className={`inline-block w-2 h-2 rounded-full ${
        active ? "bg-green-500 animate-pulse" : "bg-muted-foreground"
      }`}
    />
  );
}

function LauncherStatusCard() {
  const { data: status, isLoading } = useCodexLauncherStatus();
  const startMutation = useCodexLauncherStart();
  const stopMutation = useCodexLauncherStop();

  const [customPort, setCustomPort] = useState<string>("");

  const handleStart = () => {
    const port = customPort ? parseInt(customPort, 10) : undefined;
    startMutation.mutate(port ? { debugPort: port } : undefined);
  };

  if (isLoading) {
    return (
      <Card className="glass-card">
        <CardHeader>
          <Skeleton className="h-6 w-40" />
        </CardHeader>
        <CardContent>
          <Skeleton className="h-20 w-full" />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className="glass-card">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Zap className="h-4 w-4 text-primary" />
          Codex 注入器状态
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid grid-cols-2 gap-3 text-sm">
          <div className="flex items-center gap-2 text-muted-foreground">
            <StatusDot active={!!status?.running} />
            {status?.running ? "运行中" : "未运行"}
          </div>
          <div className="flex items-center gap-2 text-muted-foreground">
            <StatusDot active={!!status?.injected} />
            {status?.injected ? "已注入" : "未注入"}
          </div>
          {status?.debugPort && (
            <div className="text-muted-foreground col-span-2">
              调试端口：<span className="font-mono text-foreground">{status.debugPort}</span>
            </div>
          )}
          {status?.codexPath && (
            <div className="text-muted-foreground col-span-2 truncate">
              路径：<span className="font-mono text-foreground text-xs">{status.codexPath}</span>
            </div>
          )}
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          {!status?.running ? (
            <Button
              size="sm"
              onClick={handleStart}
              disabled={startMutation.isPending}
            >
              <Rocket className="h-3.5 w-3.5 mr-1.5" />
              {startMutation.isPending ? "启动中..." : "启动 Codex"}
            </Button>
          ) : (
            <Button
              size="sm"
              variant="destructive"
              onClick={() => stopMutation.mutate()}
              disabled={stopMutation.isPending}
            >
              <Square className="h-3.5 w-3.5 mr-1.5" />
              停止
            </Button>
          )}
          <Button
            size="sm"
            variant="outline"
            onClick={async () => {
              const r = await codexLauncherClient.resolvePath();
              if (r.found) {
                toast.info(`Codex 路径: ${r.path}`);
              } else {
                toast.error(`未找到 Codex: ${r.error}`);
              }
            }}
          >
            <FolderOpen className="h-3.5 w-3.5 mr-1.5" />
            探测路径
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

function SessionTable() {
  const { data: sessions, isLoading, refetch } = useCodexSessions();
  const deleteMutation = useCodexSessionDelete();

  if (isLoading) {
    return (
      <Card className="glass-card">
        <CardHeader>
          <Skeleton className="h-6 w-40" />
        </CardHeader>
        <CardContent>
          <Skeleton className="h-40 w-full" />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className="glass-card">
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle className="text-base flex items-center gap-2">
            <Circle className="h-4 w-4 text-primary" />
            会话管理
            {sessions && (
              <Badge variant="secondary" className="ml-1">
                {sessions.length}
              </Badge>
            )}
          </CardTitle>
          <Button size="sm" variant="ghost" onClick={() => refetch()}>
            <RefreshCw className="h-3.5 w-3.5" />
          </Button>
        </div>
      </CardHeader>
      <CardContent className="p-0">
        {!sessions || sessions.length === 0 ? (
          <p className="text-sm text-muted-foreground text-center py-8">
            暂无会话数据（需先启动 Codex）
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>标题</TableHead>
                <TableHead className="w-40">更新时间</TableHead>
                <TableHead className="w-24 text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {sessions.map((s) => (
                <TableRow key={s.sessionId}>
                  <TableCell className="font-medium max-w-xs truncate">
                    {s.title || <span className="text-muted-foreground italic">无标题</span>}
                  </TableCell>
                  <TableCell className="text-sm text-muted-foreground">
                    {s.updatedAt
                      ? new Date(s.updatedAt * 1000).toLocaleDateString("zh-CN")
                      : "—"}
                  </TableCell>
                  <TableCell className="text-right">
                    <Button
                      size="sm"
                      variant="ghost"
                      className="h-7 w-7 p-0 text-destructive hover:text-destructive"
                      disabled={deleteMutation.isPending}
                      onClick={() => deleteMutation.mutate(s.sessionId)}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

export default function CodexLauncherPage() {
  return (
    <div className="space-y-4 p-4">
      <div>
        <h1 className="text-xl font-semibold">Codex 启动器</h1>
        <p className="text-sm text-muted-foreground mt-1">
          注入式启动 Codex，实现会话删除、插件入口解锁等增强功能
        </p>
      </div>

      <LauncherStatusCard />
      <SessionTable />
    </div>
  );
}
