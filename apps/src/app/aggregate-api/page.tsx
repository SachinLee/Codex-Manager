"use client";

import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Activity,
  Copy,
  Database,
  Eye,
  EyeOff,
  Gauge,
  PencilLine,
  Plus,
  RefreshCw,
  ShieldCheck,
  Trash2,
  Unplug,
} from "lucide-react";
import { toast } from "sonner";

import { PageHeader, MetricCard, PageWorkspace } from "@/components/layout/page-workspace";
import { AggregateApiModal } from "@/components/modals/aggregate-api-modal";
import { CapabilityRoutingPanel } from "@/components/aggregate-api/capability-routing-panel";
import { CapabilityDiagnosticsDialog } from "@/components/aggregate-api/capability-diagnostics-dialog";
import { ConfirmDialog } from "@/components/modals/confirm-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useDeferredDesktopActivation } from "@/hooks/useDeferredDesktopActivation";
import { useDesktopPageActive } from "@/hooks/useDesktopPageActive";
import { usePageTransitionReady } from "@/hooks/usePageTransitionReady";
import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";
import { accountClient } from "@/lib/api/account-client";
import { aggregateApiProviderMatchesFilter } from "@/lib/aggregate-api-provider";
import { useI18n } from "@/lib/i18n/provider";
import { useAppStore } from "@/lib/store/useAppStore";
import { copyTextToClipboard } from "@/lib/utils/clipboard";
import { formatTsFromSeconds } from "@/lib/utils/usage";
import type {
  AggregateApi,
  AggregateApiCapabilityDiagnosticsResult,
  AggregateApiBalanceSnapshot,
  AggregateApiSecretResult,
} from "@/types/api-key";

const PROVIDER_LABELS: Record<string, string> = {
  codex: "Codex",
  claude: "Claude",
  gemini: "Gemini",
  compatible: "Codex + Claude",
};

function parseBalanceSnapshot(api: AggregateApi): AggregateApiBalanceSnapshot | null {
  const raw = String(api.lastBalanceJson || "").trim();
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as Partial<AggregateApiBalanceSnapshot>;
    return {
      isValid: parsed.isValid ?? true,
      invalidMessage: parsed.invalidMessage ?? null,
      remaining: typeof parsed.remaining === "number" ? parsed.remaining : null,
      unit: typeof parsed.unit === "string" ? parsed.unit : null,
      planName: typeof parsed.planName === "string" ? parsed.planName : null,
      total: typeof parsed.total === "number" ? parsed.total : null,
      used: typeof parsed.used === "number" ? parsed.used : null,
      extra:
        parsed.extra && typeof parsed.extra === "object"
          ? (parsed.extra as Record<string, unknown>)
          : null,
    };
  } catch {
    return null;
  }
}

function formatBalance(snapshot: AggregateApiBalanceSnapshot | null): string {
  if (!snapshot || typeof snapshot.remaining !== "number") return "-";
  const value = Number.isInteger(snapshot.remaining)
    ? String(snapshot.remaining)
    : snapshot.remaining.toFixed(2);
  const unit = String(snapshot.unit || "").trim();
  return unit.toUpperCase() === "USD" ? `$${value}` : unit ? `${value} ${unit}` : value;
}

function secretPreview(secret: AggregateApiSecretResult): string {
  if (secret.authType === "userpass") {
    return `${secret.username || ""}:${secret.password || ""}`;
  }
  return secret.key;
}

export default function AggregateApiPage() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const serviceStatus = useAppStore((state) => state.serviceStatus);
  const { canAccessManagementRpc } = useRuntimeCapabilities();
  const isServiceReady = canAccessManagementRpc && serviceStatus.connected;
  const isPageActive = useDesktopPageActive("/aggregate-api/");
  const isQueryEnabled = useDeferredDesktopActivation(
    isServiceReady && isPageActive,
  );

  const [modalOpen, setModalOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const [providerFilter, setProviderFilter] = useState("all");
  const [revealedSecrets, setRevealedSecrets] = useState<
    Record<string, AggregateApiSecretResult>
  >({});
  const [loadingSecretId, setLoadingSecretId] = useState<string | null>(null);
  const [testingApiId, setTestingApiId] = useState<string | null>(null);
  const [refreshingBalanceId, setRefreshingBalanceId] = useState<string | null>(
    null,
  );
  const [togglingApiId, setTogglingApiId] = useState<string | null>(null);
  const [capabilityApiId, setCapabilityApiId] = useState<string | null>(null);
  const [diagnosingApiId, setDiagnosingApiId] = useState<string | null>(null);
  const [diagnosticsResult, setDiagnosticsResult] =
    useState<AggregateApiCapabilityDiagnosticsResult | null>(null);

  const { data: aggregateApis = [], isLoading } = useQuery({
    queryKey: ["aggregate-apis"],
    queryFn: () => accountClient.listAggregateApis(),
    enabled: isQueryEnabled,
    staleTime: 60_000,
    retry: 1,
  });
  usePageTransitionReady("/aggregate-api/", !isServiceReady || !isLoading);

  useEffect(() => {
    if (isPageActive) return;
    const frameId = window.requestAnimationFrame(() => {
      setModalOpen(false);
      setEditingId(null);
      setDeleteId(null);
      setRevealedSecrets({});
    });
    return () => window.cancelAnimationFrame(frameId);
  }, [isPageActive]);

  const editingApi = useMemo(
    () => aggregateApis.find((api) => api.id === editingId) || null,
    [aggregateApis, editingId],
  );
  const filteredApis = useMemo(
    () =>
      providerFilter === "all"
        ? aggregateApis
        : aggregateApis.filter((api) =>
            aggregateApiProviderMatchesFilter(api.providerType, providerFilter),
          ),
    [aggregateApis, providerFilter],
  );
  const selectedCapabilityApi = useMemo(
    () => aggregateApis.find((api) => api.id === capabilityApiId) ?? aggregateApis[0] ?? null,
    [aggregateApis, capabilityApiId]
  );
  const defaultCreateSort = useMemo(
    () =>
      aggregateApis.reduce(
        (largest, api) => Math.max(largest, Number(api.sort) || 0),
        0,
      ) + 5,
    [aggregateApis],
  );
  const activeCount = aggregateApis.filter((api) => api.status === "active").length;
  const routedCount = aggregateApis.filter((api) => api.modelSlugs.length > 0).length;
  const failedCount = aggregateApis.filter((api) => api.lastTestStatus === "failed").length;

  const deleteMutation = useMutation({
    mutationFn: (apiId: string) => accountClient.deleteAggregateApi(apiId),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["aggregate-apis"] }),
        queryClient.invalidateQueries({ queryKey: ["managed-models-v2"] }),
        queryClient.invalidateQueries({ queryKey: ["apikeys"] }),
        queryClient.invalidateQueries({ queryKey: ["startup-snapshot"] }),
      ]);
      toast.success(t("聚合 API 已删除"));
    },
    onError: (error: unknown) => {
      toast.error(`${t("删除失败")}: ${error instanceof Error ? error.message : String(error)}`);
    },
  });

  const testMutation = useMutation({
    mutationFn: (apiId: string) => accountClient.testAggregateApiConnection(apiId),
    onMutate: (apiId) => setTestingApiId(apiId),
    onSuccess: (result) => {
      if (result.ok) {
        toast.success(t("连通性测试成功"));
      } else {
        toast.error(result.message || t("连通性测试失败"));
      }
    },
    onSettled: async (_result, _error, apiId) => {
      setTestingApiId((current) => (current === apiId ? null : current));
      await queryClient.invalidateQueries({ queryKey: ["aggregate-apis"] });
    },
  });

  const balanceMutation = useMutation({
    mutationFn: (apiId: string) => accountClient.refreshAggregateApiBalance(apiId),
    onMutate: (apiId) => setRefreshingBalanceId(apiId),
    onSuccess: (result) => {
      if (result.ok) toast.success(t("余额已刷新"));
      else toast.error(result.message || t("余额查询失败"));
    },
    onSettled: async (_result, _error, apiId) => {
      setRefreshingBalanceId((current) => (current === apiId ? null : current));
      await queryClient.invalidateQueries({ queryKey: ["aggregate-apis"] });
    },
  });

  const toggleMutation = useMutation({
    mutationFn: ({ api, enabled }: { api: AggregateApi; enabled: boolean }) =>
      accountClient.updateAggregateApi(api.id, {
        supplierName: api.supplierName || api.url,
        status: enabled ? "active" : "disabled",
      }),
    onMutate: ({ api }) => setTogglingApiId(api.id),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["aggregate-apis"] }),
        queryClient.invalidateQueries({ queryKey: ["apikeys"] }),
        queryClient.invalidateQueries({ queryKey: ["startup-snapshot"] }),
      ]);
      toast.success(t("状态已更新"));
    },
    onError: (error: unknown) => {
      toast.error(`${t("更新状态失败")}: ${error instanceof Error ? error.message : String(error)}`);
    },
    onSettled: () => setTogglingApiId(null),
  });

  const diagnosticsMutation = useMutation({
    mutationFn: (apiId: string) =>
      accountClient.diagnoseAggregateApiCapabilities(apiId, { liveSmoke: false }),
    onMutate: (apiId) => {
      setDiagnosingApiId(apiId);
      setDiagnosticsResult(null);
    },
    onSuccess: setDiagnosticsResult,
    onError: (error: unknown) => {
      toast.error(`${t("能力诊断失败")}: ${error instanceof Error ? error.message : String(error)}`);
    },
    onSettled: (_result, _error, apiId) => {
      setDiagnosingApiId((current) => (current === apiId ? null : current));
    },
  });

  const toggleSecret = async (apiId: string) => {
    if (revealedSecrets[apiId]) {
      setRevealedSecrets((current) => {
        const next = { ...current };
        delete next[apiId];
        return next;
      });
      return;
    }
    setLoadingSecretId(apiId);
    try {
      const secret = await accountClient.readAggregateApiSecret(apiId);
      setRevealedSecrets((current) => ({ ...current, [apiId]: secret }));
    } catch (error) {
      toast.error(`${t("读取密钥失败")}: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setLoadingSecretId(null);
    }
  };

  return (
    <>
      <PageWorkspace>
        <PageHeader
          eyebrow={t("显式路由")}
          title={t("聚合 API")}
          description={t("这里只管理上游连接；模型路由在“模型管理”中显式配置，页面不会访问供应商 `/models`。")}
          actions={
            <Button
              size="sm"
              disabled={!isServiceReady}
              onClick={() => {
                setEditingId(null);
                setModalOpen(true);
              }}
            >
              <Plus className="mr-1.5 h-4 w-4" />
              {t("新建聚合 API")}
            </Button>
          }
        />

        <section className="grid grid-cols-2 gap-2 lg:grid-cols-4">
          <MetricCard title={t("总数")} value={aggregateApis.length} icon={Database} tone="blue" />
          <MetricCard title={t("已启用")} value={activeCount} icon={ShieldCheck} tone="emerald" />
          <MetricCard title={t("已有模型路由")} value={routedCount} icon={Gauge} tone="violet" />
          <MetricCard title={t("测试失败")} value={failedCount} icon={Unplug} tone="rose" />
        </section>

        <Card className="glass-card overflow-hidden py-0">
          <CardHeader className="border-b border-border/50 px-4 py-3">
            <div className="flex items-center justify-between gap-3">
              <div>
                <CardTitle>{t("上游连接")}</CardTitle>
                <p className="mt-1 text-xs text-muted-foreground">
                  {t("连通性测试只使用已配置路由对应的模型。")}
                </p>
              </div>
              <Select value={providerFilter} onValueChange={(value) => setProviderFilter(value || "all")}>
                <SelectTrigger className="h-9 w-[150px]"><SelectValue /></SelectTrigger>
                <SelectContent><SelectGroup>
                  <SelectItem value="all">{t("全部类型")}</SelectItem>
                  <SelectItem value="codex">Codex</SelectItem>
                  <SelectItem value="claude">Claude</SelectItem>
                  <SelectItem value="gemini">Gemini</SelectItem>
                  <SelectItem value="compatible">
                    {t("通用兼容（Codex + Claude）")}
                  </SelectItem>
                </SelectGroup></SelectContent>
              </Select>
            </div>
          </CardHeader>
          <CardContent className="p-0">
            <div className="overflow-x-auto">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t("供应商")}</TableHead>
                    <TableHead>{t("类型")}</TableHead>
                    <TableHead>{t("密钥")}</TableHead>
                    <TableHead>{t("模型路由")}</TableHead>
                    <TableHead>{t("余额")}</TableHead>
                    <TableHead>{t("连通性")}</TableHead>
                    <TableHead>{t("启用")}</TableHead>
                    <TableHead className="text-right">{t("操作")}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {isLoading ? (
                    Array.from({ length: 4 }).map((_, index) => (
                      <TableRow key={index}>
                        {Array.from({ length: 8 }).map((__, cell) => (
                          <TableCell key={cell}><Skeleton className="h-7 w-full" /></TableCell>
                        ))}
                      </TableRow>
                    ))
                  ) : filteredApis.length === 0 ? (
                    <TableRow>
                      <TableCell colSpan={8} className="h-48 text-center text-muted-foreground">
                        {t("暂无聚合 API，点击右上角新建")}
                      </TableCell>
                    </TableRow>
                  ) : (
                    filteredApis.map((api) => {
                      const revealed = revealedSecrets[api.id];
                      const balance = parseBalanceSnapshot(api);
                      const testError = String(api.lastTestError || "").trim();
                      return (
                        <TableRow key={api.id}>
                          <TableCell className="min-w-[240px]">
                            <div className="font-medium">{api.supplierName || api.id}</div>
                            <div className="max-w-[360px] truncate font-mono text-[11px] text-muted-foreground">{api.url}</div>
                            <div className="mt-1 text-[10px] text-muted-foreground">
                              {t("创建时间")}: {formatTsFromSeconds(api.createdAt, "-")}
                            </div>
                          </TableCell>
                          <TableCell>
                            <Badge variant="secondary">
                              {api.providerType === "compatible"
                                ? t("通用兼容（Codex + Claude）")
                                : PROVIDER_LABELS[api.providerType] || api.providerType}
                            </Badge>
                          </TableCell>
                          <TableCell>
                            <div className="flex items-center gap-1">
                              <code className="max-w-[160px] truncate rounded border bg-muted/40 px-2 py-1 text-[10px]">
                                {revealed ? secretPreview(revealed) : loadingSecretId === api.id ? t("读取中...") : api.id}
                              </code>
                              <Button type="button" variant="ghost" size="icon" aria-label={revealed ? t("隐藏密钥") : t("显示密钥")} onClick={() => void toggleSecret(api.id)}>
                                {revealed ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                              </Button>
                              {revealed ? (
                                <Button type="button" variant="ghost" size="icon" aria-label={t("复制密钥")} onClick={() => void copyTextToClipboard(secretPreview(revealed)).then(() => toast.success(t("密钥已复制")))}>
                                  <Copy className="h-4 w-4" />
                                </Button>
                              ) : null}
                            </div>
                          </TableCell>
                          <TableCell className="max-w-[240px]">
                            {api.modelSlugs.length > 0 ? (
                              <div className="flex flex-wrap gap-1">
                                {api.modelSlugs.slice(0, 3).map((slug) => <Badge key={slug} variant="outline">{slug}</Badge>)}
                                {api.modelSlugs.length > 3 ? <Badge variant="secondary">+{api.modelSlugs.length - 3}</Badge> : null}
                              </div>
                            ) : (
                              <Badge variant="destructive">missing route</Badge>
                            )}
                          </TableCell>
                          <TableCell>
                            <div className="flex items-center gap-1">
                              <span className="font-mono text-xs">{formatBalance(balance)}</span>
                              {api.balanceQueryEnabled ? (
                                <Button type="button" variant="ghost" size="icon" aria-label={t("刷新余额")} disabled={refreshingBalanceId === api.id} onClick={() => balanceMutation.mutate(api.id)}>
                                  <RefreshCw className={`h-4 w-4 ${refreshingBalanceId === api.id ? "animate-spin" : ""}`} />
                                </Button>
                              ) : null}
                            </div>
                          </TableCell>
                          <TableCell>
                            <div className="space-y-1">
                              {api.lastTestStatus === "failed" && testError ? (
                                <Tooltip>
                                  <TooltipTrigger
                                    render={<span />}
                                    className="inline-flex cursor-help"
                                  >
                                    <Badge variant="destructive">{t("失败")}</Badge>
                                  </TooltipTrigger>
                                  <TooltipContent className="max-w-sm whitespace-pre-wrap break-words">
                                    {testError}
                                  </TooltipContent>
                                </Tooltip>
                              ) : (
                                <Badge variant={api.lastTestStatus === "success" ? "default" : api.lastTestStatus === "failed" ? "destructive" : "secondary"}>
                                  {api.lastTestStatus === "success" ? t("已连通") : api.lastTestStatus === "failed" ? t("失败") : t("未测试")}
                                </Badge>
                              )}
                              <Button type="button" size="sm" variant="ghost" className="h-7 px-2 text-xs" disabled={testingApiId === api.id || api.modelSlugs.length === 0} onClick={() => testMutation.mutate(api.id)}>
                                {testingApiId === api.id ? t("测试中...") : t("测试 route")}
                              </Button>
                            </div>
                          </TableCell>
                          <TableCell>
                            <Switch
                              checked={api.status === "active"}
                              disabled={togglingApiId === api.id}
                              onCheckedChange={(enabled) => toggleMutation.mutate({ api, enabled })}
                            />
                          </TableCell>
                          <TableCell>
                            <div className="flex justify-end gap-1">
                              <Button
                                type="button"
                                variant="ghost"
                                size="icon"
                                aria-label={t("能力诊断")}
                                disabled={!isServiceReady || diagnosingApiId === api.id}
                                onClick={() => diagnosticsMutation.mutate(api.id)}
                              >
                                <Activity className={`h-4 w-4 ${diagnosingApiId === api.id ? "animate-pulse" : ""}`} />
                              </Button>
                              <Button type="button" variant="ghost" size="icon" aria-label={t("编辑聚合 API")} onClick={() => { setEditingId(api.id); setModalOpen(true); }}>
                                <PencilLine className="h-4 w-4" />
                              </Button>
                              <Button type="button" variant="ghost" size="icon" aria-label={t("删除聚合 API")} onClick={() => setDeleteId(api.id)}>
                                <Trash2 className="h-4 w-4" />
                              </Button>
                            </div>
                          </TableCell>
                        </TableRow>
                      );
                    })
                  )}
                </TableBody>
              </Table>
            </div>
          </CardContent>
        </Card>
        {selectedCapabilityApi ? (
          <Card className="glass-card">
            <CardHeader className="flex-row items-center justify-between gap-3 py-4">
              <div>
                <CardTitle>{t("能力感知路由")}</CardTitle>
                <p className="mt-1 text-xs text-muted-foreground">
                  {t("查看与调整单个上游的能力学习和路由恢复策略。")}
                </p>
              </div>
              <Select
                value={selectedCapabilityApi.id}
                onValueChange={(value) => setCapabilityApiId(value || null)}
              >
                <SelectTrigger className="w-[220px]"><SelectValue /></SelectTrigger>
                <SelectContent>
                  {aggregateApis.map((api) => (
                    <SelectItem key={api.id} value={api.id}>
                      {api.supplierName || api.id}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </CardHeader>
            <CardContent className="pt-0">
              <CapabilityRoutingPanel apiId={selectedCapabilityApi.id} />
            </CardContent>
          </Card>
        ) : null}
      </PageWorkspace>

      <AggregateApiModal
        open={modalOpen}
        onOpenChange={setModalOpen}
        aggregateApi={editingApi}
        defaultSort={defaultCreateSort}
      />

      <CapabilityDiagnosticsDialog
        result={diagnosticsResult}
        onOpenChange={(open) => {
          if (!open) setDiagnosticsResult(null);
        }}
      />

      <ConfirmDialog
        open={Boolean(deleteId)}
        onOpenChange={(open) => {
          if (!open) setDeleteId(null);
        }}
        title={t("删除聚合 API")}
        description={t("删除连接时会同时删除引用它的模型路由。")}
        confirmText={t("删除")}
        confirmVariant="destructive"
        onConfirm={() => {
          if (!deleteId) return;
          deleteMutation.mutate(deleteId);
          setDeleteId(null);
        }}
      />
    </>
  );
}
