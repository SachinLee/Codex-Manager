"use client";

import { useMemo, useState } from "react";
import {
  Rocket,
  Square,
  RefreshCw,
  Trash2,
  FolderOpen,
  FolderTree,
  MoveRight,
  Archive,
  Zap,
  Circle,
  Search,
  KeyRound,
  ChevronRight,
  CalendarDays,
  Smartphone,
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
  useCodexSessionMove,
  useCodexSessionsMoveMany,
  useCodexArchivedSessionsDeleteAll,
  useCodexAppBridgeStatus,
  useCodexEnableRemoteControl,
} from "@/hooks/useCodexLauncher";
import {
  codexLauncherClient,
  type CodexSession,
  type CodexSessionListOptions,
} from "@/lib/api/codex-launcher";
import { Checkbox } from "@/components/ui/checkbox";
import { useI18n } from "@/lib/i18n/provider";

const ARCHIVE_ALL = "__all__";
const PROJECT_ALL = "__all__";
const PROJECT_NONE = "__none__";
const PROJECT_ARCHIVED = "__archived__";
const DATE_FILTER_ALL = "all";
const DATE_FILTER_CURRENT_MONTH = "currentMonth";
const DATE_FILTER_LAST_MONTH = "lastMonth";
const DATE_FILTER_LAST_3_MONTHS = "last3Months";
const DATE_FILTER_CUSTOM_MONTH = "customMonth";
const DATE_FILTER_CUSTOM_RANGE = "customRange";

type DateFilterMode =
  | typeof DATE_FILTER_ALL
  | typeof DATE_FILTER_CURRENT_MONTH
  | typeof DATE_FILTER_LAST_MONTH
  | typeof DATE_FILTER_LAST_3_MONTHS
  | typeof DATE_FILTER_CUSTOM_MONTH
  | typeof DATE_FILTER_CUSTOM_RANGE;

interface ProjectTreeNode {
  path: string;
  label: string;
  count: number;
  exactCount: number;
  children: ProjectTreeNode[];
}

function normalizeCwd(value: string | null | undefined): string {
  return String(value || "")
    .trim()
    .replace(/\\/g, "/")
    .replace(/^\/\/\?\/UNC\//i, "//")
    .replace(/^\/\/\?\//, "")
    .replace(/^\/+([A-Za-z]:\/)/, "$1")
    .replace(/\/+$/, "");
}

function toMonthInputValue(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  return `${year}-${month}`;
}

function toDateInputValue(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function startOfLocalMonth(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

function addLocalMonths(date: Date, months: number): Date {
  return new Date(date.getFullYear(), date.getMonth() + months, 1);
}

function monthInputToRange(value: string): { from: Date; to: Date } | null {
  const match = /^(\d{4})-(\d{2})$/.exec(value);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]);
  if (!Number.isFinite(year) || month < 1 || month > 12) return null;
  const from = new Date(year, month - 1, 1);
  return { from, to: addLocalMonths(from, 1) };
}

function dateInputToStart(value: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(year, month - 1, day);
  if (
    date.getFullYear() !== year ||
    date.getMonth() !== month - 1 ||
    date.getDate() !== day
  ) {
    return null;
  }
  return date;
}

function toUnixSeconds(date: Date): number {
  return Math.floor(date.getTime() / 1000);
}

function buildSessionDateOptions(
  mode: DateFilterMode,
  customMonth: string,
  customFrom: string,
  customTo: string,
): CodexSessionListOptions {
  const now = new Date();
  let range: { from: Date; to: Date } | null = null;

  if (mode === DATE_FILTER_CURRENT_MONTH) {
    const from = startOfLocalMonth(now);
    range = { from, to: addLocalMonths(from, 1) };
  } else if (mode === DATE_FILTER_LAST_MONTH) {
    const to = startOfLocalMonth(now);
    range = { from: addLocalMonths(to, -1), to };
  } else if (mode === DATE_FILTER_LAST_3_MONTHS) {
    const to = addLocalMonths(startOfLocalMonth(now), 1);
    range = { from: addLocalMonths(to, -3), to };
  } else if (mode === DATE_FILTER_CUSTOM_MONTH) {
    range = monthInputToRange(customMonth);
  } else if (mode === DATE_FILTER_CUSTOM_RANGE) {
    const from = dateInputToStart(customFrom);
    const toStart = dateInputToStart(customTo);
    const to = toStart
      ? new Date(toStart.getFullYear(), toStart.getMonth(), toStart.getDate() + 1)
      : null;
    range = from || to ? { from: from ?? new Date(0), to: to ?? new Date(8640000000000000) } : null;
  }

  return {
    updatedFrom: range ? toUnixSeconds(range.from) : null,
    updatedTo: range ? toUnixSeconds(range.to) : null,
    limit: range ? 2000 : 500,
  };
}

function shortCwd(p: string): string {
  // 仅显示最后两段路径，避免占满列宽（hover title 显示全路径）
  const norm = normalizeCwd(p);
  const parts = norm.split("/").filter(Boolean);
  if (parts.length <= 2) return norm;
  return ".../" + parts.slice(-2).join("/");
}

function displayProjectName(path: string): string {
  const parts = normalizeCwd(path).split("/").filter(Boolean);
  return parts.at(-1) || path || "未命名目录";
}

function pathSegments(path: string): string[] {
  return normalizeCwd(path).split("/").filter(Boolean);
}

function joinPathSegments(parts: string[]): string {
  if (parts.length === 0) return "";
  const [first, ...rest] = parts;
  return rest.length === 0 ? first : `${first}/${rest.join("/")}`;
}

function buildProjectTree(sessions: CodexSession[]): ProjectTreeNode[] {
  const roots = new Map<string, ProjectTreeNode>();
  const byPath = new Map<string, ProjectTreeNode>();
  const uniqueCwds = new Map<string, number>();

  sessions.forEach((session) => {
    const cwd = normalizeCwd(session.cwd);
    if (!cwd) return;
    uniqueCwds.set(cwd, (uniqueCwds.get(cwd) || 0) + 1);
  });

  uniqueCwds.forEach((exactCount, cwd) => {
    const parts = pathSegments(cwd);
    parts.forEach((_, index) => {
      const currentParts = parts.slice(0, index + 1);
      const path = joinPathSegments(currentParts);
      const parentPath = joinPathSegments(parts.slice(0, index));
      let node = byPath.get(path);
      if (!node) {
        node = {
          path,
          label: currentParts.at(-1) || path,
          count: 0,
          exactCount: 0,
          children: [],
        };
        byPath.set(path, node);
        if (parentPath) {
          byPath.get(parentPath)?.children.push(node);
        } else {
          roots.set(path, node);
        }
      }
      node.count += exactCount;
      if (path === cwd) {
        node.exactCount += exactCount;
      }
    });
  });

  const sortNodes = (nodes: ProjectTreeNode[]) => {
    nodes.sort((a, b) => a.label.localeCompare(b.label));
    nodes.forEach((node) => sortNodes(node.children));
    return nodes;
  };

  return sortNodes(Array.from(roots.values()));
}

function flattenProjectTree(nodes: ProjectTreeNode[]): ProjectTreeNode[] {
  return nodes.flatMap((node) => [node, ...flattenProjectTree(node.children)]);
}

function actualProjectTargets(nodes: ProjectTreeNode[]): ProjectTreeNode[] {
  return flattenProjectTree(nodes).filter((node) => node.exactCount > 0);
}

function projectMatchesSelection(session: CodexSession, selectedProject: string): boolean {
  if (selectedProject === PROJECT_ALL) return true;
  if (selectedProject === PROJECT_NONE) return !normalizeCwd(session.cwd);
  if (selectedProject === PROJECT_ARCHIVED) return !!session.archived;
  const cwd = normalizeCwd(session.cwd);
  return cwd === selectedProject || cwd.startsWith(`${selectedProject}/`);
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

function ProjectNodeButton({
  node,
  depth,
  selectedProject,
  onSelect,
}: {
  node: ProjectTreeNode;
  depth: number;
  selectedProject: string;
  onSelect: (path: string) => void;
}) {
  const selected = selectedProject === node.path;

  return (
    <div>
      <button
        type="button"
        title={node.path}
        onClick={() => onSelect(node.path)}
        className={`flex h-8 w-full items-center gap-1.5 rounded-md px-2 text-left text-sm transition ${
          selected ? "bg-primary text-primary-foreground" : "hover:bg-muted"
        }`}
        style={{ paddingLeft: `${8 + depth * 14}px` }}
      >
        {node.children.length > 0 ? (
          <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        ) : (
          <span className="h-3.5 w-3.5 shrink-0" />
        )}
        <FolderOpen className="h-3.5 w-3.5 shrink-0" />
        <span className="min-w-0 flex-1 truncate">{node.label}</span>
        <Badge variant={selected ? "secondary" : "outline"} className="h-5 px-1.5 text-[10px]">
          {node.count}
        </Badge>
      </button>
      {node.children.map((child) => (
        <ProjectNodeButton
          key={child.path}
          node={child}
          depth={depth + 1}
          selectedProject={selectedProject}
          onSelect={onSelect}
        />
      ))}
    </div>
  );
}

function ProjectTreePanel({
  sessions,
  projectTree,
  selectedProject,
  onSelect,
}: {
  sessions: CodexSession[];
  projectTree: ProjectTreeNode[];
  selectedProject: string;
  onSelect: (path: string) => void;
}) {
  const { t } = useI18n();
  const [treeKeyword, setTreeKeyword] = useState("");
  const normalizedKeyword = treeKeyword.trim().toLowerCase();
  const noneCount = sessions.filter((session) => !normalizeCwd(session.cwd)).length;
  const archivedCount = sessions.filter((session) => session.archived).length;
  const visibleTree = useMemo(() => {
    if (!normalizedKeyword) return projectTree;
    const filterNode = (node: ProjectTreeNode): ProjectTreeNode | null => {
      const children = node.children
        .map(filterNode)
        .filter((child): child is ProjectTreeNode => !!child);
      const matches =
        node.path.toLowerCase().includes(normalizedKeyword) ||
        node.label.toLowerCase().includes(normalizedKeyword);
      if (!matches && children.length === 0) return null;
      return { ...node, children };
    };
    return projectTree
      .map(filterNode)
      .filter((node): node is ProjectTreeNode => !!node);
  }, [normalizedKeyword, projectTree]);

  const quickItems = [
    { id: PROJECT_ALL, label: t("全部会话"), count: sessions.length, icon: Circle },
    { id: PROJECT_NONE, label: t("无项目"), count: noneCount, icon: FolderOpen },
    { id: PROJECT_ARCHIVED, label: t("已归档"), count: archivedCount, icon: Archive },
  ];

  return (
    <div className="border-b border-border/60 p-3 lg:w-80 lg:border-b-0 lg:border-r">
      <div className="mb-3 flex items-center gap-2 text-sm font-medium">
        <FolderTree className="h-4 w-4 text-primary" />
        {t("项目目录")}
      </div>
      <div className="relative mb-3">
        <Search className="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
        <Input
          placeholder={t("搜索目录")}
          value={treeKeyword}
          onChange={(event) => setTreeKeyword(event.target.value)}
          className="h-8 pl-8 text-sm"
        />
      </div>
      <div className="space-y-1">
        {quickItems.map(({ id, label, count, icon: Icon }) => {
          const selected = selectedProject === id;
          return (
            <button
              key={id}
              type="button"
              onClick={() => onSelect(id)}
              className={`flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-sm transition ${
                selected ? "bg-primary text-primary-foreground" : "hover:bg-muted"
              }`}
            >
              <Icon className="h-3.5 w-3.5 shrink-0" />
              <span className="flex-1">{label}</span>
              <Badge variant={selected ? "secondary" : "outline"} className="h-5 px-1.5 text-[10px]">
                {count}
              </Badge>
            </button>
          );
        })}
      </div>
      <div className="mt-3 max-h-[520px] space-y-1 overflow-y-auto pr-1">
        {visibleTree.length === 0 ? (
          <div className="py-6 text-center text-sm text-muted-foreground">
            {t("没有匹配的项目目录")}
          </div>
        ) : (
          visibleTree.map((node) => (
            <ProjectNodeButton
              key={node.path}
              node={node}
              depth={0}
              selectedProject={selectedProject}
              onSelect={onSelect}
            />
          ))
        )}
      </div>
    </div>
  );
}

function LauncherStatusCard() {
  const { t } = useI18n();
  const { data: status, isLoading } = useCodexLauncherStatus();
  const {
    data: bridgeStatus,
    isLoading: bridgeLoading,
    refetch: refetchBridgeStatus,
  } = useCodexAppBridgeStatus();
  const injectStartMutation = useCodexLauncherStart();
  const plainStartMutation = useCodexLauncherStartPlain();
  const stopMutation = useCodexLauncherStop();
  const enableRemoteControlMutation = useCodexEnableRemoteControl();

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
      refetchBridgeStatus();
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
          {t("Codex 启动状态")}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid grid-cols-2 gap-3 text-sm">
          <div className="flex items-center gap-2 text-muted-foreground">
            <StatusDot active={!!status?.running} />
            {status?.running ? t("运行中") : t("未运行")}
          </div>
          <div className="flex items-center gap-2 text-muted-foreground">
            <StatusDot active={!!status?.running || !!status?.injected} />
            {status?.injected ? t("已注入") : status?.running ? t("普通启动") : t("未注入")}
          </div>
          {status?.debugPort && (
            <div className="text-muted-foreground col-span-2">
              {t("调试端口：")}<span className="font-mono text-foreground">{status.debugPort}</span>
            </div>
          )}
          {status?.codexPath && (
            <div className="text-muted-foreground col-span-2 truncate">
              {t("路径：")}<span className="font-mono text-foreground text-xs">{status.codexPath}</span>
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
                {plainStartMutation.isPending ? t("启动中...") : t("普通启动")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={handleInjectStart}
                disabled={plainStartMutation.isPending || injectStartMutation.isPending}
              >
                <Zap className="h-3.5 w-3.5 mr-1.5" />
                {injectStartMutation.isPending ? t("注入中...") : t("启动并注入")}
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
              {t("停止")}
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
            {t("探测路径")}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={handleConfigureCm}
            disabled={configuringCm}
          >
            <KeyRound className="h-3.5 w-3.5 mr-1.5" />
            {configuringCm ? t("配置中...") : t("配置桥接")}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => enableRemoteControlMutation.mutate()}
            disabled={enableRemoteControlMutation.isPending}
          >
            <Smartphone className="h-3.5 w-3.5 mr-1.5" />
            {enableRemoteControlMutation.isPending ? t("启用中...") : t("启用手机远控")}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={handleSyncProvider}
            disabled={syncingProvider}
          >
            <RefreshCw className={`h-3.5 w-3.5 mr-1.5 ${syncingProvider ? "animate-spin" : ""}`} />
            {t("同步 Provider")}
          </Button>
        </div>

        <div className="rounded-md border border-border/70 bg-muted/20 p-3">
          <div className="mb-2 flex items-center justify-between gap-2">
            <div className="text-sm font-medium">{t("Codex App 桥接")}</div>
            <Badge variant={bridgeStatus?.issues.length ? "secondary" : "default"}>
              {bridgeLoading
                ? t("检查中")
                : bridgeStatus?.issues.length
                  ? t("待处理")
                  : t("就绪")}
            </Badge>
          </div>
          <div className="flex flex-wrap gap-2 text-xs">
            <BridgeStateBadge
              label={t("登录态")}
              active={
                !!bridgeStatus?.authModeChatgpt &&
                !!bridgeStatus?.hasAccessToken &&
                !!bridgeStatus?.hasIdToken &&
                !!bridgeStatus?.hasRefreshToken
              }
            />
            <BridgeStateBadge label={t("网关")} active={!!bridgeStatus?.providerIsCm} />
            <BridgeStateBadge
              label={t("远程连接")}
              active={!!bridgeStatus?.remoteConnectionsEnabled}
            />
            <BridgeStateBadge
              label={t("手机控制")}
              active={!!bridgeStatus?.dbRemoteControlEnabled}
            />
            <BridgeStateBadge
              label={t("桌面授权")}
              active={!!bridgeStatus && !bridgeStatus.desktopSignInRequired}
            />
            <BridgeStateBadge
              label={t("App 确认")}
              active={!!bridgeStatus?.logEnablementSeen}
              muted
            />
          </div>
          {bridgeStatus?.issues?.[0] && (
            <div className="mt-2 truncate text-xs text-muted-foreground" title={bridgeStatus.issues[0]}>
              {bridgeStatus.issues[0]}
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

function BridgeStateBadge({
  label,
  active,
  muted = false,
}: {
  label: string;
  active: boolean;
  muted?: boolean;
}) {
  return (
    <Badge
      variant={active ? "default" : muted ? "outline" : "secondary"}
      className="h-6 rounded-md"
    >
      <StatusDot active={active} />
      <span className="ml-1">{label}</span>
    </Badge>
  );
}

function SessionTable() {
  const { t } = useI18n();
  const [keyword, setKeyword] = useState("");
  const [selectedProject, setSelectedProject] = useState<string>(PROJECT_ALL);
  const [archiveFilter, setArchiveFilter] = useState<string>(ARCHIVE_ALL);
  const [dateFilterMode, setDateFilterMode] = useState<DateFilterMode>(DATE_FILTER_ALL);
  const [customMonth, setCustomMonth] = useState(() => toMonthInputValue(new Date()));
  const [customFromDate, setCustomFromDate] = useState("");
  const [customToDate, setCustomToDate] = useState(() => toDateInputValue(new Date()));
  const [pendingDelete, setPendingDelete] = useState<CodexSession | null>(null);
  const [pendingBulkDelete, setPendingBulkDelete] = useState<CodexSession[]>([]);
  const [pendingMove, setPendingMove] = useState<CodexSession[]>([]);
  const [moveTarget, setMoveTarget] = useState<string | null>(null);
  const [pendingDeleteArchived, setPendingDeleteArchived] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());

  const sessionListOptions = useMemo(
    () =>
      buildSessionDateOptions(
        dateFilterMode,
        customMonth,
        customFromDate,
        customToDate,
      ),
    [customFromDate, customMonth, customToDate, dateFilterMode],
  );

  const { data: sessions, isLoading, refetch } = useCodexSessions(sessionListOptions);
  const deleteMutation = useCodexSessionDelete();
  const deleteManyMutation = useCodexSessionsDeleteMany();
  const moveMutation = useCodexSessionMove();
  const moveManyMutation = useCodexSessionsMoveMany();
  const deleteAllArchivedMutation = useCodexArchivedSessionsDeleteAll();

  const allSessions = sessions || [];
  const projectTree = useMemo(() => buildProjectTree(allSessions), [allSessions]);
  const projectTargets = useMemo(() => actualProjectTargets(projectTree), [projectTree]);
  const availableMoveTargets = useMemo(
    () =>
      projectTargets.filter((target) =>
        pendingMove.some((session) => normalizeCwd(session.cwd) !== target.path)
      ),
    [pendingMove, projectTargets]
  );
  const selectedProjectLabel =
    selectedProject === PROJECT_ALL
      ? t("全部会话")
      : selectedProject === PROJECT_NONE
        ? t("无项目")
        : selectedProject === PROJECT_ARCHIVED
          ? t("已归档")
          : selectedProject;

  const filtered = useMemo(() => {
    if (!sessions) return [] as CodexSession[];
    const kw = keyword.trim().toLowerCase();
    return sessions.filter((s) => {
      if (kw) {
        const hay = `${s.title || ""} ${s.cwd || ""}`.toLowerCase();
          if (!hay.includes(kw)) return false;
      }
      if (!projectMatchesSelection(s, selectedProject)) return false;
      if (archiveFilter !== ARCHIVE_ALL) {
        const wantArchived = archiveFilter === "archived";
        if (!!s.archived !== wantArchived) return false;
      }
      return true;
    });
  }, [sessions, keyword, selectedProject, archiveFilter]);

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

  const openMoveDialog = (targetSessions: CodexSession[]) => {
    setPendingMove(targetSessions);
    setMoveTarget(null);
  };

  const handleConfirmMove = () => {
    const ids = pendingMove.map((session) => session.sessionId);
    if (ids.length === 0 || !moveTarget) return;
    const targetCwd = moveTarget;
    if (ids.length === 1) {
      moveMutation.mutate(
        { sessionId: ids[0], targetCwd },
        { onSettled: () => setPendingMove([]) }
      );
      return;
    }
    moveManyMutation.mutate(
      { sessionIds: ids, targetCwd },
      {
        onSettled: () => {
          setPendingMove([]);
          setSelectedIds((current) => {
            const next = new Set(current);
            ids.forEach((id) => next.delete(id));
            return next;
          });
        },
      }
    );
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
            {t("会话管理")}
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

        {sessions && (
          <div className="flex items-center gap-2 flex-wrap mt-3">
            <div className="relative flex-1 min-w-[180px] max-w-xs">
              <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
              <Input
                placeholder={t("搜索会话标题或路径")}
                value={keyword}
                onChange={(e) => setKeyword(e.target.value)}
                className="h-8 pl-8 text-sm"
              />
            </div>
            <Select
              value={archiveFilter}
              onValueChange={(v) => setArchiveFilter(v ?? ARCHIVE_ALL)}
            >
              <SelectTrigger className="h-8 w-[120px] text-sm">
                <SelectValue placeholder={t("归档状态")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={ARCHIVE_ALL}>{t("全部")}</SelectItem>
                <SelectItem value="active">{t("未归档")}</SelectItem>
                <SelectItem value="archived">{t("已归档")}</SelectItem>
              </SelectContent>
            </Select>
            <Select
              value={dateFilterMode}
              onValueChange={(value) => setDateFilterMode((value ?? DATE_FILTER_ALL) as DateFilterMode)}
            >
              <SelectTrigger className="h-8 w-[150px] text-sm">
                <CalendarDays className="mr-1.5 h-3.5 w-3.5" />
                <SelectValue placeholder={t("更新时间")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={DATE_FILTER_ALL}>{t("全部时间")}</SelectItem>
                <SelectItem value={DATE_FILTER_CURRENT_MONTH}>{t("本月")}</SelectItem>
                <SelectItem value={DATE_FILTER_LAST_MONTH}>{t("上月")}</SelectItem>
                <SelectItem value={DATE_FILTER_LAST_3_MONTHS}>{t("近 3 个月")}</SelectItem>
                <SelectItem value={DATE_FILTER_CUSTOM_MONTH}>{t("指定月份")}</SelectItem>
                <SelectItem value={DATE_FILTER_CUSTOM_RANGE}>{t("日期范围")}</SelectItem>
              </SelectContent>
            </Select>
            {dateFilterMode === DATE_FILTER_CUSTOM_MONTH && (
              <Input
                type="month"
                value={customMonth}
                onChange={(event) => setCustomMonth(event.target.value)}
                className="h-8 w-[140px] text-sm"
                aria-label={t("选择会话更新时间月份")}
              />
            )}
            {dateFilterMode === DATE_FILTER_CUSTOM_RANGE && (
              <>
                <Input
                  type="date"
                  value={customFromDate}
                  onChange={(event) => setCustomFromDate(event.target.value)}
                  className="h-8 w-[145px] text-sm"
                  aria-label={t("会话更新时间开始日期")}
                />
                <span className="text-xs text-muted-foreground">{t("至")}</span>
                <Input
                  type="date"
                  value={customToDate}
                  onChange={(event) => setCustomToDate(event.target.value)}
                  className="h-8 w-[145px] text-sm"
                  aria-label={t("会话更新时间结束日期")}
                />
              </>
            )}
            {(keyword ||
              selectedProject !== PROJECT_ALL ||
              archiveFilter !== ARCHIVE_ALL ||
              dateFilterMode !== DATE_FILTER_ALL) && (
              <Button
                size="sm"
                variant="ghost"
                className="h-8 text-xs"
                onClick={() => {
                  setKeyword("");
                  setSelectedProject(PROJECT_ALL);
                  setArchiveFilter(ARCHIVE_ALL);
                  setDateFilterMode(DATE_FILTER_ALL);
                }}
              >
                {t("重置")}
              </Button>
            )}
            {selectedInFiltered.length > 0 && (
              <>
                <Button
                  size="sm"
                  variant="outline"
                  className="h-8 text-xs"
                  disabled={moveManyMutation.isPending}
                  onClick={() => openMoveDialog(selectedInFiltered)}
                >
                  <MoveRight className="h-3.5 w-3.5 mr-1.5" />
                  {t("移动选中")} {selectedInFiltered.length}
                </Button>
                <Button
                  size="sm"
                  variant="destructive"
                  className="h-8 text-xs"
                  disabled={deleteManyMutation.isPending}
                  onClick={() => setPendingBulkDelete(selectedInFiltered)}
                >
                  <Trash2 className="h-3.5 w-3.5 mr-1.5" />
                  {t("删除选中")} {selectedInFiltered.length}
                </Button>
              </>
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
                {t("删除已归档")} {archivedCount}
              </Button>
            )}
          </div>
        )}
      </CardHeader>
      <CardContent className="p-0">
        {!sessions || sessions.length === 0 ? (
          <p className="text-sm text-muted-foreground text-center py-8">
            {t("暂无会话数据（需先启动 Codex）")}
          </p>
        ) : (
          <div className="flex flex-col lg:flex-row">
            <ProjectTreePanel
              sessions={allSessions}
              projectTree={projectTree}
              selectedProject={selectedProject}
              onSelect={setSelectedProject}
            />
            <div className="min-w-0 flex-1">
              <div className="flex items-center justify-between gap-3 border-b border-border/60 px-4 py-3">
                <div className="min-w-0">
                  <div className="text-sm font-medium truncate">{selectedProjectLabel}</div>
                  <div className="text-xs text-muted-foreground">
                    {filtered.length} {t("个会话")}
                  </div>
                </div>
              </div>
              {filtered.length === 0 ? (
                <p className="text-sm text-muted-foreground text-center py-8">
                  {t("没有匹配当前筛选条件的会话")}
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
                          aria-label={t("选择当前筛选会话")}
                        />
                      </TableHead>
                      <TableHead>{t("标题")}</TableHead>
                      {selectedProject === PROJECT_ALL && (
                        <TableHead className="w-56">{t("所属项目")}</TableHead>
                      )}
                      <TableHead className="w-20 text-center">{t("归档")}</TableHead>
                      <TableHead className="w-32">{t("更新时间")}</TableHead>
                      <TableHead className="w-28 text-right">{t("操作")}</TableHead>
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
                              aria-label={`${t("选择会话")} ${s.title || s.sessionId}`}
                          />
                        </TableCell>
                        <TableCell className="font-medium max-w-xs truncate">
                          {s.title || <span className="text-muted-foreground italic">{t("无标题")}</span>}
                        </TableCell>
                        {selectedProject === PROJECT_ALL && (
                          <TableCell
                            className="text-xs text-muted-foreground font-mono truncate max-w-[14rem]"
                            title={s.cwd || ""}
                          >
                            {s.cwd ? shortCwd(s.cwd) : <span className="italic">—</span>}
                          </TableCell>
                        )}
                        <TableCell className="text-center">
                          {s.archived ? (
                            <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
                              {t("已归档")}
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
                          <div className="flex justify-end gap-1">
                            <Button
                              size="sm"
                              variant="ghost"
                              className="h-7 w-7 p-0"
                              disabled={moveMutation.isPending || moveManyMutation.isPending}
                              onClick={() => openMoveDialog([s])}
                              title={t("移动会话")}
                            >
                              <MoveRight className="h-3.5 w-3.5" />
                            </Button>
                            <Button
                              size="sm"
                              variant="ghost"
                              className="h-7 w-7 p-0 text-destructive hover:text-destructive"
                              disabled={deleteMutation.isPending}
                              onClick={() => setPendingDelete(s)}
                              title={t("删除会话")}
                            >
                              <Trash2 className="h-3.5 w-3.5" />
                            </Button>
                          </div>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </div>
          </div>
        )}
      </CardContent>

      <Dialog
        open={pendingMove.length > 0}
        onOpenChange={(open) => {
          if (!open) setPendingMove([]);
        }}
      >
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("移动会话")}</DialogTitle>
            <DialogDescription>
              {t("选择目标工作目录，确认后会更新 Codex 会话所属项目。")}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="rounded-md border border-border/70 p-3 text-sm">
              <div className="text-muted-foreground">{t("待移动")}</div>
              <div className="mt-1 font-medium">
                {pendingMove.length === 1
                  ? pendingMove[0]?.title || pendingMove[0]?.sessionId
                  : `${pendingMove.length} ${t("个会话")}`}
              </div>
              {pendingMove.length === 1 && (
                <div className="mt-1 break-all font-mono text-xs text-muted-foreground">
                  {pendingMove[0]?.cwd || t("无项目")}
                </div>
              )}
            </div>
            <div className="space-y-2">
              <div className="text-sm font-medium">{t("目标目录")}</div>
              <Select
                value={moveTarget}
                onValueChange={(value) => setMoveTarget(value)}
              >
                <SelectTrigger className="h-9 w-full text-sm">
                  <SelectValue placeholder={t("请选择目标目录")} />
                </SelectTrigger>
                <SelectContent>
                  {availableMoveTargets.map((target) => (
                    <SelectItem key={target.path} value={target.path}>
                      <span className="font-mono text-xs">{shortCwd(target.path)}</span>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {availableMoveTargets.length === 0 && (
                <div className="text-xs text-muted-foreground">
                  {t("暂无可用目标目录")}
                </div>
              )}
            </div>
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setPendingMove([])}
              disabled={moveMutation.isPending || moveManyMutation.isPending}
            >
              {t("取消")}
            </Button>
            <Button
              onClick={handleConfirmMove}
              disabled={!moveTarget || moveMutation.isPending || moveManyMutation.isPending}
            >
              <MoveRight className="h-3.5 w-3.5 mr-1.5" />
              {moveMutation.isPending || moveManyMutation.isPending ? t("移动中...") : t("确认移动")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={!!pendingDelete}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null);
        }}
      >
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{t("确认删除会话")}</DialogTitle>
            <DialogDescription>
              {t("即将删除以下会话及其本地数据，删除后可在 30 天内通过备份令牌撤销。")}
            </DialogDescription>
          </DialogHeader>
          {pendingDelete && (
            <div className="space-y-2 text-sm">
              <div>
                <span className="text-muted-foreground">{t("标题：")}</span>
                <span className="font-medium">
                  {pendingDelete.title || <span className="italic">{t("无标题")}</span>}
                </span>
              </div>
              {pendingDelete.cwd && (
                <div className="break-all">
                  <span className="text-muted-foreground">{t("所属项目：")}</span>
                  <span className="font-mono text-xs">{pendingDelete.cwd}</span>
                </div>
              )}
              {pendingDelete.archived && (
                <div>
                  <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
                    {t("已归档")}
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
              {t("取消")}
            </Button>
            <Button
              variant="destructive"
              onClick={handleConfirmDelete}
              disabled={deleteMutation.isPending}
            >
              <Trash2 className="h-3.5 w-3.5 mr-1.5" />
              {deleteMutation.isPending ? t("删除中...") : t("确认删除")}
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
            <DialogTitle>{t("确认批量删除会话")}</DialogTitle>
            <DialogDescription>
              {t("即将删除")} {pendingBulkDelete.length} {t("个会话及其本地数据，删除后可在 30 天内通过备份令牌撤销。")}
            </DialogDescription>
          </DialogHeader>
          <div className="max-h-48 space-y-2 overflow-y-auto text-sm">
            {pendingBulkDelete.slice(0, 12).map((session) => (
              <div key={session.sessionId} className="truncate">
                {session.title || <span className="text-muted-foreground italic">{t("无标题")}</span>}
              </div>
            ))}
            {pendingBulkDelete.length > 12 && (
              <div className="text-xs text-muted-foreground">
                {t("还有")} {pendingBulkDelete.length - 12} {t("个会话")}
              </div>
            )}
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setPendingBulkDelete([])}
              disabled={deleteManyMutation.isPending}
            >
              {t("取消")}
            </Button>
            <Button
              variant="destructive"
              onClick={handleConfirmBulkDelete}
              disabled={deleteManyMutation.isPending}
            >
              <Trash2 className="h-3.5 w-3.5 mr-1.5" />
              {deleteManyMutation.isPending ? t("删除中...") : t("确认删除")}
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
            <DialogTitle>{t("确认删除已归档会话")}</DialogTitle>
            <DialogDescription>
              {t("即将删除全部")} {archivedCount} {t("个已归档会话及其本地数据。")}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setPendingDeleteArchived(false)}
              disabled={deleteAllArchivedMutation.isPending}
            >
              {t("取消")}
            </Button>
            <Button
              variant="destructive"
              onClick={handleConfirmDeleteArchived}
              disabled={deleteAllArchivedMutation.isPending}
            >
              <Trash2 className="h-3.5 w-3.5 mr-1.5" />
              {deleteAllArchivedMutation.isPending ? t("删除中...") : t("确认删除")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}

export default function CodexLauncherPage() {
  const { t } = useI18n();
  return (
    <div className="space-y-4 p-4">
      <div>
        <h1 className="text-xl font-semibold">{t("Codex 启动器")}</h1>
        <p className="text-sm text-muted-foreground mt-1">
          {t("普通启动 Codex 并使用 Codex-Manager 会话管理；注入模式仅作为旧增强能力保留")}
        </p>
      </div>

      <LauncherStatusCard />
      <SessionTable />
    </div>
  );
}
