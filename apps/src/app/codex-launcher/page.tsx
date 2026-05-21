"use client";

import { useMemo, useState } from "react";
import {
  Rocket,
  Square,
  RefreshCw,
  Trash2,
  FolderOpen,
  Zap,
  Circle,
  Search,
  KeyRound,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
  useCodexLauncherStartPlain,
  useCodexLauncherStop,
  useCodexSessions,
  useCodexSessionDelete,
  useCodexSessionsDeleteMany,
  useCodexArchivedSessionsDeleteAll,
} from "@/hooks/useCodexLauncher";
import { codexLauncherClient, type CodexSession } from "@/lib/api/codex-launcher";
import { Checkbox } from "@/components/ui/checkbox";

const ARCHIVE_ALL = "__all__";
const PROJECT_ALL = "__all__";
const PROJECT_NONE = "__none__";

function shortCwd(p: string): string {
  // 仅显示最后两段路径，避免占满列宽（hover title 显示全路径）
  const norm = p.replace(/\\/g, "/").replace(/\/+$/, "");
  const parts = norm.split("/").filter(Boolean);
  if (parts.length <= 2) return norm;
  return ".../" + parts.slice(-2).join("/");
}

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
  const injectStartMutation = useCodexLauncherStart();
  const plainStartMutation = useCodexLauncherStartPlain();
  const stopMutation = useCodexLauncherStop();

  const [configuringCm, setConfiguringCm] = useState(false);
  const [syncingProvider, setSyncingProvider] = useState(false);

  const handlePlainStart = () => {
    plainStartMutation.mutate(undefined);
  };

  const handleInjectStart = () => {
    injectStartMutation.mutate(undefined);
  };

  const handleConfigureCm = async () => {
    setConfiguringCm(true);
    try {
      const result = await codexLauncherClient.configureCm();
      const sync = result.providerSync;
      toast.success(
        `已配置 cm：账号 ${result.selectedAccountLabel || result.selectedAccountId}，同步 ${sync.changedSessionFiles} 个会话文件 / ${sync.sqliteRowsUpdated} 行`
      );
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setConfiguringCm(false);
    }
  };

  const handleSyncProvider = async () => {
    setSyncingProvider(true);
    try {
      const result = await codexLauncherClient.syncProviderCm();
      toast.success(`Provider 已同步：${result.changedSessionFiles} 个会话文件 / ${result.sqliteRowsUpdated} 行`);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setSyncingProvider(false);
    }
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
          Codex 启动状态
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid grid-cols-2 gap-3 text-sm">
          <div className="flex items-center gap-2 text-muted-foreground">
            <StatusDot active={!!status?.running} />
            {status?.running ? "运行中" : "未运行"}
          </div>
          <div className="flex items-center gap-2 text-muted-foreground">
            <StatusDot active={!!status?.running || !!status?.injected} />
            {status?.injected ? "已注入" : status?.running ? "普通启动" : "未注入"}
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
            <>
              <Button
                size="sm"
                onClick={handlePlainStart}
                disabled={plainStartMutation.isPending || injectStartMutation.isPending}
              >
                <Rocket className="h-3.5 w-3.5 mr-1.5" />
                {plainStartMutation.isPending ? "启动中..." : "普通启动"}
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={handleInjectStart}
                disabled={plainStartMutation.isPending || injectStartMutation.isPending}
              >
                <Zap className="h-3.5 w-3.5 mr-1.5" />
                {injectStartMutation.isPending ? "注入中..." : "启动并注入"}
              </Button>
            </>
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
          <Button
            size="sm"
            variant="outline"
            onClick={handleConfigureCm}
            disabled={configuringCm}
          >
            <KeyRound className="h-3.5 w-3.5 mr-1.5" />
            {configuringCm ? "配置中..." : "配置 cm"}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={handleSyncProvider}
            disabled={syncingProvider}
          >
            <RefreshCw className={`h-3.5 w-3.5 mr-1.5 ${syncingProvider ? "animate-spin" : ""}`} />
            同步 Provider
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

function SessionTable() {
  const { data: sessions, isLoading, refetch } = useCodexSessions();
  const deleteMutation = useCodexSessionDelete();
  const deleteManyMutation = useCodexSessionsDeleteMany();
  const deleteAllArchivedMutation = useCodexArchivedSessionsDeleteAll();

  const [keyword, setKeyword] = useState("");
  const [projectFilter, setProjectFilter] = useState<string>(PROJECT_ALL);
  const [archiveFilter, setArchiveFilter] = useState<string>(ARCHIVE_ALL);
  const [pendingDelete, setPendingDelete] = useState<CodexSession | null>(null);
  const [pendingBulkDelete, setPendingBulkDelete] = useState<CodexSession[]>([]);
  const [pendingDeleteArchived, setPendingDeleteArchived] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());

  // 项目下拉：从会话 cwd 去重；保留全路径作为 value，用 shortCwd 展示
  const projectOptions = useMemo(() => {
    if (!sessions) return [] as string[];
    const set = new Set<string>();
    sessions.forEach((s) => {
      if (s.cwd) set.add(s.cwd);
    });
    return Array.from(set).sort((a, b) => a.localeCompare(b));
  }, [sessions]);

  const filtered = useMemo(() => {
    if (!sessions) return [] as CodexSession[];
    const kw = keyword.trim().toLowerCase();
    return sessions.filter((s) => {
      if (kw) {
        const hay = `${s.title || ""} ${s.cwd || ""}`.toLowerCase();
        if (!hay.includes(kw)) return false;
      }
      if (projectFilter !== PROJECT_ALL) {
        if (projectFilter === PROJECT_NONE) {
          if (s.cwd) return false;
        } else if (s.cwd !== projectFilter) {
          return false;
        }
      }
      if (archiveFilter !== ARCHIVE_ALL) {
        const wantArchived = archiveFilter === "archived";
        if (!!s.archived !== wantArchived) return false;
      }
      return true;
    });
  }, [sessions, keyword, projectFilter, archiveFilter]);

  const filteredIds = useMemo(
    () => filtered.map((session) => session.sessionId),
    [filtered]
  );
  const selectedInFiltered = useMemo(
    () => filtered.filter((session) => selectedIds.has(session.sessionId)),
    [filtered, selectedIds]
  );
  const allFilteredSelected =
    filteredIds.length > 0 && filteredIds.every((id) => selectedIds.has(id));
  const someFilteredSelected =
    filteredIds.some((id) => selectedIds.has(id)) && !allFilteredSelected;
  const archivedCount = useMemo(
    () => (sessions || []).filter((session) => session.archived).length,
    [sessions]
  );

  const handleConfirmDelete = () => {
    if (!pendingDelete) return;
    deleteMutation.mutate(pendingDelete.sessionId, {
      onSettled: () => setPendingDelete(null),
    });
  };

  const toggleSelected = (sessionId: string, checked: boolean) => {
    setSelectedIds((current) => {
      const next = new Set(current);
      if (checked) {
        next.add(sessionId);
      } else {
        next.delete(sessionId);
      }
      return next;
    });
  };

  const toggleFilteredSelected = (checked: boolean) => {
    setSelectedIds((current) => {
      const next = new Set(current);
      filteredIds.forEach((id) => {
        if (checked) {
          next.add(id);
        } else {
          next.delete(id);
        }
      });
      return next;
    });
  };

  const handleConfirmBulkDelete = () => {
    const ids = pendingBulkDelete.map((session) => session.sessionId);
    if (ids.length === 0) return;
    deleteManyMutation.mutate(ids, {
      onSettled: () => {
        setSelectedIds((current) => {
          const next = new Set(current);
          ids.forEach((id) => next.delete(id));
          return next;
        });
        setPendingBulkDelete([]);
      },
    });
  };

  const handleConfirmDeleteArchived = () => {
    deleteAllArchivedMutation.mutate(undefined, {
      onSettled: () => {
        setSelectedIds((current) => {
          const next = new Set(current);
          (sessions || [])
            .filter((session) => session.archived)
            .forEach((session) => next.delete(session.sessionId));
          return next;
        });
        setPendingDeleteArchived(false);
      },
    });
  };

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
        <div className="flex items-center justify-between gap-3 flex-wrap">
          <CardTitle className="text-base flex items-center gap-2">
            <Circle className="h-4 w-4 text-primary" />
            会话管理
            {sessions && (
              <Badge variant="secondary" className="ml-1">
                {filtered.length}/{sessions.length}
              </Badge>
            )}
          </CardTitle>
          <Button size="sm" variant="ghost" onClick={() => refetch()}>
            <RefreshCw className="h-3.5 w-3.5" />
          </Button>
        </div>

        {sessions && sessions.length > 0 && (
          <div className="flex items-center gap-2 flex-wrap mt-3">
            <div className="relative flex-1 min-w-[180px] max-w-xs">
              <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
              <Input
                placeholder="搜索标题或路径"
                value={keyword}
                onChange={(e) => setKeyword(e.target.value)}
                className="h-8 pl-8 text-sm"
              />
            </div>
            <Select
              value={projectFilter}
              onValueChange={(v) => setProjectFilter(v ?? PROJECT_ALL)}
            >
              <SelectTrigger className="h-8 w-[220px] text-sm">
                <SelectValue placeholder="所属项目" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={PROJECT_ALL}>全部项目</SelectItem>
                <SelectItem value={PROJECT_NONE}>无项目</SelectItem>
                {projectOptions.map((p) => (
                  <SelectItem key={p} value={p}>
                    <span className="font-mono text-xs">{shortCwd(p)}</span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Select
              value={archiveFilter}
              onValueChange={(v) => setArchiveFilter(v ?? ARCHIVE_ALL)}
            >
              <SelectTrigger className="h-8 w-[120px] text-sm">
                <SelectValue placeholder="归档状态" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={ARCHIVE_ALL}>全部</SelectItem>
                <SelectItem value="active">未归档</SelectItem>
                <SelectItem value="archived">已归档</SelectItem>
              </SelectContent>
            </Select>
            {(keyword || projectFilter !== PROJECT_ALL || archiveFilter !== ARCHIVE_ALL) && (
              <Button
                size="sm"
                variant="ghost"
                className="h-8 text-xs"
                onClick={() => {
                  setKeyword("");
                  setProjectFilter(PROJECT_ALL);
                  setArchiveFilter(ARCHIVE_ALL);
                }}
              >
                重置
              </Button>
            )}
            {selectedInFiltered.length > 0 && (
              <Button
                size="sm"
                variant="destructive"
                className="h-8 text-xs"
                disabled={deleteManyMutation.isPending}
                onClick={() => setPendingBulkDelete(selectedInFiltered)}
              >
                <Trash2 className="h-3.5 w-3.5 mr-1.5" />
                删除选中 {selectedInFiltered.length}
              </Button>
            )}
            {archivedCount > 0 && (
              <Button
                size="sm"
                variant="outline"
                className="h-8 text-xs text-destructive hover:text-destructive"
                disabled={deleteAllArchivedMutation.isPending}
                onClick={() => setPendingDeleteArchived(true)}
              >
                <Trash2 className="h-3.5 w-3.5 mr-1.5" />
                删除已归档 {archivedCount}
              </Button>
            )}
          </div>
        )}
      </CardHeader>
      <CardContent className="p-0">
        {!sessions || sessions.length === 0 ? (
          <p className="text-sm text-muted-foreground text-center py-8">
            暂无会话数据（需先启动 Codex）
          </p>
        ) : filtered.length === 0 ? (
          <p className="text-sm text-muted-foreground text-center py-8">
            没有匹配当前筛选条件的会话
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-10">
                  <Checkbox
                    checked={allFilteredSelected}
                    aria-checked={someFilteredSelected ? "mixed" : allFilteredSelected}
                    onCheckedChange={(checked) => toggleFilteredSelected(checked === true)}
                    aria-label="选择当前筛选会话"
                  />
                </TableHead>
                <TableHead>标题</TableHead>
                <TableHead className="w-56">所属项目</TableHead>
                <TableHead className="w-20 text-center">归档</TableHead>
                <TableHead className="w-32">更新时间</TableHead>
                <TableHead className="w-24 text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filtered.map((s) => (
                <TableRow key={s.sessionId}>
                  <TableCell>
                    <Checkbox
                      checked={selectedIds.has(s.sessionId)}
                      onCheckedChange={(checked) =>
                        toggleSelected(s.sessionId, checked === true)
                      }
                      aria-label={`选择会话 ${s.title || s.sessionId}`}
                    />
                  </TableCell>
                  <TableCell className="font-medium max-w-xs truncate">
                    {s.title || <span className="text-muted-foreground italic">无标题</span>}
                  </TableCell>
                  <TableCell
                    className="text-xs text-muted-foreground font-mono truncate max-w-[14rem]"
                    title={s.cwd || ""}
                  >
                    {s.cwd ? shortCwd(s.cwd) : <span className="italic">—</span>}
                  </TableCell>
                  <TableCell className="text-center">
                    {s.archived ? (
                      <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
                        已归档
                      </Badge>
                    ) : (
                      <span className="text-xs text-muted-foreground">—</span>
                    )}
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
                      onClick={() => setPendingDelete(s)}
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

      <Dialog
        open={!!pendingDelete}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null);
        }}
      >
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>确认删除会话</DialogTitle>
            <DialogDescription>
              即将删除以下会话及其本地数据，删除后可在 30 天内通过备份令牌撤销。
            </DialogDescription>
          </DialogHeader>
          {pendingDelete && (
            <div className="space-y-2 text-sm">
              <div>
                <span className="text-muted-foreground">标题：</span>
                <span className="font-medium">
                  {pendingDelete.title || <span className="italic">无标题</span>}
                </span>
              </div>
              {pendingDelete.cwd && (
                <div className="break-all">
                  <span className="text-muted-foreground">所属项目：</span>
                  <span className="font-mono text-xs">{pendingDelete.cwd}</span>
                </div>
              )}
              {pendingDelete.archived && (
                <div>
                  <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
                    已归档
                  </Badge>
                </div>
              )}
            </div>
          )}
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setPendingDelete(null)}
              disabled={deleteMutation.isPending}
            >
              取消
            </Button>
            <Button
              variant="destructive"
              onClick={handleConfirmDelete}
              disabled={deleteMutation.isPending}
            >
              <Trash2 className="h-3.5 w-3.5 mr-1.5" />
              {deleteMutation.isPending ? "删除中..." : "确认删除"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={pendingBulkDelete.length > 0}
        onOpenChange={(open) => {
          if (!open) setPendingBulkDelete([]);
        }}
      >
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>确认批量删除会话</DialogTitle>
            <DialogDescription>
              即将删除 {pendingBulkDelete.length} 个会话及其本地数据，删除后可在 30 天内通过备份令牌撤销。
            </DialogDescription>
          </DialogHeader>
          <div className="max-h-48 space-y-2 overflow-y-auto text-sm">
            {pendingBulkDelete.slice(0, 12).map((session) => (
              <div key={session.sessionId} className="truncate">
                {session.title || <span className="text-muted-foreground italic">无标题</span>}
              </div>
            ))}
            {pendingBulkDelete.length > 12 && (
              <div className="text-xs text-muted-foreground">
                还有 {pendingBulkDelete.length - 12} 个会话
              </div>
            )}
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setPendingBulkDelete([])}
              disabled={deleteManyMutation.isPending}
            >
              取消
            </Button>
            <Button
              variant="destructive"
              onClick={handleConfirmBulkDelete}
              disabled={deleteManyMutation.isPending}
            >
              <Trash2 className="h-3.5 w-3.5 mr-1.5" />
              {deleteManyMutation.isPending ? "删除中..." : "确认删除"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={pendingDeleteArchived}
        onOpenChange={(open) => setPendingDeleteArchived(open)}
      >
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>确认删除已归档会话</DialogTitle>
            <DialogDescription>
              即将删除全部 {archivedCount} 个已归档会话及其本地数据。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setPendingDeleteArchived(false)}
              disabled={deleteAllArchivedMutation.isPending}
            >
              取消
            </Button>
            <Button
              variant="destructive"
              onClick={handleConfirmDeleteArchived}
              disabled={deleteAllArchivedMutation.isPending}
            >
              <Trash2 className="h-3.5 w-3.5 mr-1.5" />
              {deleteAllArchivedMutation.isPending ? "删除中..." : "确认删除"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}

export default function CodexLauncherPage() {
  return (
    <div className="space-y-4 p-4">
      <div>
        <h1 className="text-xl font-semibold">Codex 启动器</h1>
        <p className="text-sm text-muted-foreground mt-1">
          普通启动 Codex 并使用 Codex-Manager 会话管理；注入模式仅作为旧增强能力保留
        </p>
      </div>

      <LauncherStatusCard />
      <SessionTable />
    </div>
  );
}
