"use client";

import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Activity,
  ChevronDown,
  CircleDollarSign,
  Coins,
  Copy,
  Database,
  Eye,
  EyeOff,
  Gauge,
  PencilLine,
  Percent,
  Plus,
  RefreshCw,
  RotateCcw,
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
import { useLocalDayRange } from "@/hooks/useLocalDayRange";
import { usePageTransitionReady } from "@/hooks/usePageTransitionReady";
import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";
import { useAggregateApiRuntimeStatuses } from "@/hooks/useAggregateApiRuntimeStatuses";
import { accountClient } from "@/lib/api/account-client";
import { aggregateApiProviderMatchesFilter } from "@/lib/aggregate-api-provider";
import { getAppErrorMessage } from "@/lib/api/transport";
import { useI18n } from "@/lib/i18n/provider";
import { useAppStore } from "@/lib/store/useAppStore";
import {
  formatCacheRateValue,
  formatMillionTokenAmount,
  formatUsdAmount,
} from "@/lib/utils/billing";
import { copyTextToClipboard } from "@/lib/utils/clipboard";
import { formatTsFromSeconds } from "@/lib/utils/usage";
import type {
  AggregateApi,
  AggregateApiCapabilityDiagnosticsResult,
  AggregateApiBalanceSnapshot,
  AggregateApiDailyUsageStat,
  AggregateApiSecretResult,
  AggregateApiHealthState,
} from "@/types/api-key";
import type { ModelDailyUsageStat } from "@/types/request-log";

const PROVIDER_LABELS: Record<string, string> = {
  codex: "Codex",
  claude: "Claude",
  gemini: "Gemini",
  compatible: "Codex + Claude",
};

const AGGREGATE_API_TABLE_COLUMNS = 12;

const HEALTH_STATE_META: Record<AggregateApiHealthState, { label: string; className: string }> = {
  healthy: { label: "推荐使用", className: "border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300" },
  unknown: { label: "可用", className: "border-slate-500/20 bg-slate-500/10 text-slate-700 dark:text-slate-300" },
  degraded: { label: "需注意", className: "border-amber-500/20 bg-amber-500/10 text-amber-700 dark:text-amber-300" },
  unhealthy: { label: "不可用", className: "border-rose-500/20 bg-rose-500/10 text-rose-700 dark:text-rose-300" },
  cooldown: { label: "冷却中", className: "border-orange-500/20 bg-orange-500/10 text-orange-700 dark:text-orange-300" },
  recovering: { label: "冷却中", className: "border-orange-500/20 bg-orange-500/10 text-orange-700 dark:text-orange-300" },
};

function buildAggregateApiDailyUsageMap(
  items: AggregateApiDailyUsageStat[],
): Map<string, AggregateApiDailyUsageStat> {
  return new Map(items.map((item) => [item.aggregateApiId, item]));
}

function buildDailyUsageTooltip(
  usage: AggregateApiDailyUsageStat,
  t: (key: string) => string,
): string {
  return [
    `${t("请求")} ${usage.requestCount}`,
    `${t("输入")} ${formatMillionTokenAmount(usage.inputTokens)} / ${t("缓存")} ${formatMillionTokenAmount(usage.cachedInputTokens)} / ${t("缓存写入")} ${formatMillionTokenAmount(usage.cacheWriteInputTokens)} / ${t("计费输入")} ${formatMillionTokenAmount(usage.billableInputTokens)}`,
    `${t("输出")} ${formatMillionTokenAmount(usage.outputTokens)} / ${t("推理输出")} ${formatMillionTokenAmount(usage.reasoningOutputTokens)}`,
    `${t("Guard 重试")} ${formatMillionTokenAmount(usage.guardRetryTotalTokens)} tok / ${formatUsdAmount(usage.guardRetryEstimatedCostUsd)}`,
    `${t("计费合计")} ${formatMillionTokenAmount(usage.billableTotalTokens)} tok / ${formatUsdAmount(usage.billableEstimatedCostUsd)}`,
    t("含 Guard 重试"),
  ].join("\n");
}

function buildModelDailyUsageTooltip(
  usage: ModelDailyUsageStat,
  t: (key: string) => string,
): string {
  return [
    `${t("请求")} ${usage.requestCount}`,
    `${t("输入")} ${formatMillionTokenAmount(usage.inputTokens)} / ${t("缓存")} ${formatMillionTokenAmount(usage.cachedInputTokens)} / ${t("缓存写入")} ${formatMillionTokenAmount(usage.cacheWriteInputTokens)} / ${t("计费输入")} ${formatMillionTokenAmount(usage.billableInputTokens)}`,
    `${t("输出")} ${formatMillionTokenAmount(usage.outputTokens)} / ${t("推理输出")} ${formatMillionTokenAmount(usage.reasoningOutputTokens)}`,
    `${t("合计")} ${formatMillionTokenAmount(usage.totalTokens)} tok / ${formatUsdAmount(usage.estimatedCostUsd)}`,
    `${t("缓存率")} ${formatCacheRateValue(usage.cacheHitRate)}`,
  ].join("\n");
}

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

function formatCooldownRemaining(cooldownUntil: number | null | undefined, nowSeconds: number): string {
  const remaining = Math.max(0, Number(cooldownUntil || 0) - nowSeconds);
  const minutes = Math.floor(remaining / 60);
  const seconds = remaining % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
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
  const localDayRange = useLocalDayRange();

  const [modalOpen, setModalOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const [providerFilter, setProviderFilter] = useState("all");
  const [modelDailyUsageExpanded, setModelDailyUsageExpanded] = useState(false);
  const [revealedSecrets, setRevealedSecrets] = useState<
    Record<string, AggregateApiSecretResult>
  >({});
  const [loadingSecretId, setLoadingSecretId] = useState<string | null>(null);
  const [testingApiId, setTestingApiId] = useState<string | null>(null);
  const [refreshingBalanceId, setRefreshingBalanceId] = useState<string | null>(
    null,
  );
  const [refreshingBalances, setRefreshingBalances] = useState(false);
  const [togglingApiId, setTogglingApiId] = useState<string | null>(null);
  const [capabilityApiId, setCapabilityApiId] = useState<string | null>(null);
  const [diagnosingApiId, setDiagnosingApiId] = useState<string | null>(null);
  const [diagnosticsResult, setDiagnosticsResult] =
    useState<AggregateApiCapabilityDiagnosticsResult | null>(null);
  const [resetCooldownApi, setResetCooldownApi] = useState<AggregateApi | null>(null);

  const {
    byApiId: aggregateApiRuntimeStatusById,
    nowSeconds: runtimeStatusNowSeconds,
  } = useAggregateApiRuntimeStatuses(isQueryEnabled);

  const { data: aggregateApis = [], isLoading } = useQuery({
    queryKey: ["aggregate-apis"],
    queryFn: () => accountClient.listAggregateApis(),
    enabled: isQueryEnabled,
    staleTime: 60_000,
    retry: 1,
  });
  const healthQuery = useQuery({
    queryKey: ["aggregate-api-health"],
    queryFn: () => accountClient.listAggregateApiHealth(),
    enabled: isQueryEnabled,
    staleTime: 10_000,
    refetchInterval: isQueryEnabled ? 15_000 : false,
  });
  const healthByApiId = useMemo(
    () => new Map((healthQuery.data || []).map((item) => [item.aggregateApiId, item])),
    [healthQuery.data],
  );
  const probeCostRange = useMemo(() => {
    const now = new Date();
    const start = new Date(now.getFullYear(), now.getMonth(), 1);
    const end = new Date(now.getFullYear(), now.getMonth() + 1, 1);
    return {
      startTs: Math.floor(start.getTime() / 1_000),
      endTs: Math.floor(end.getTime() / 1_000),
    };
  }, []);
  const probeCostQuery = useQuery({
    queryKey: ["aggregate-api-health-costs", probeCostRange.startTs, probeCostRange.endTs],
    queryFn: () => accountClient.listAggregateApiProbeCosts(probeCostRange.startTs, probeCostRange.endTs),
    enabled: isQueryEnabled,
    staleTime: 15_000,
    refetchInterval: isQueryEnabled ? 60_000 : false,
  });
  const probeCostByApiId = useMemo(
    () => new Map((probeCostQuery.data || []).map((item) => [item.aggregateApiId, item])),
    [probeCostQuery.data],
  );

  const dailyUsageQuery = useQuery({
    queryKey: [
      "requestlog",
      "aggregate-api-daily-usage",
      localDayRange.dayStartTs,
      localDayRange.dayEndTs,
    ],
    queryFn: () =>
      accountClient.listAggregateApiDailyUsageStats({
        dayStartTs: localDayRange.dayStartTs,
        dayEndTs: localDayRange.dayEndTs,
      }),
    enabled: isQueryEnabled,
    retry: 1,
    staleTime: 0,
    refetchInterval: isQueryEnabled ? 5_000 : false,
    refetchIntervalInBackground: true,
    refetchOnMount: "always",
    refetchOnWindowFocus: "always",
    refetchOnReconnect: "always",
  });

  const modelDailyUsageQuery = useQuery({
    queryKey: [
      "requestlog",
      "model-daily-usage",
      localDayRange.dayStartTs,
      localDayRange.dayEndTs,
    ],
    queryFn: () =>
      accountClient.listModelDailyUsageStats({
        dayStartTs: localDayRange.dayStartTs,
        dayEndTs: localDayRange.dayEndTs,
      }),
    enabled: isQueryEnabled,
    retry: 1,
    staleTime: 0,
    refetchInterval: isQueryEnabled ? 5_000 : false,
    refetchIntervalInBackground: true,
    refetchOnMount: "always",
    refetchOnWindowFocus: "always",
    refetchOnReconnect: "always",
  });

  usePageTransitionReady("/aggregate-api/", !isServiceReady || !isLoading);

  useEffect(() => {
    if (isPageActive) return;
    const frameId = window.requestAnimationFrame(() => {
      setModalOpen(false);
      setEditingId(null);
      setDeleteId(null);
      setResetCooldownApi(null);
      setRevealedSecrets({});
    });
    return () => window.cancelAnimationFrame(frameId);
  }, [isPageActive]);

  const editingApi = useMemo(
    () => aggregateApis.find((api) => api.id === editingId) || null,
    [aggregateApis, editingId],
  );
  const sortedAggregateApis = useMemo(
    () =>
      [...aggregateApis].sort((left, right) => {
        const sortDifference = left.sort - right.sort;
        return sortDifference !== 0 ? sortDifference : left.id.localeCompare(right.id);
      }),
    [aggregateApis],
  );
  const filteredApis = useMemo(
    () =>
      providerFilter === "all"
        ? sortedAggregateApis
        : sortedAggregateApis.filter((api) =>
            aggregateApiProviderMatchesFilter(api.providerType, providerFilter),
          ),
    [sortedAggregateApis, providerFilter],
  );
  const balanceEnabledApiIds = useMemo(
    () => filteredApis.filter((api) => api.balanceQueryEnabled).map((api) => api.id),
    [filteredApis],
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
  const dailyUsageById = useMemo(
    () => buildAggregateApiDailyUsageMap(dailyUsageQuery.data || []),
    [dailyUsageQuery.data],
  );
  const filteredDailyUsageSummary = useMemo(() => {
    let requestCount = 0;
    let inputTokens = 0;
    let cachedInputTokens = 0;
    let billableTotalTokens = 0;
    let billableEstimatedCostUsd = 0;

    for (const api of filteredApis) {
      const usage = dailyUsageById.get(api.id);
      if (!usage || usage.requestCount <= 0) continue;
      requestCount += usage.requestCount;
      inputTokens += usage.inputTokens;
      cachedInputTokens += usage.cachedInputTokens;
      billableTotalTokens += usage.billableTotalTokens;
      billableEstimatedCostUsd += usage.billableEstimatedCostUsd;
    }

    return {
      requestCount,
      inputTokens,
      cachedInputTokens,
      billableTotalTokens,
      billableEstimatedCostUsd,
      cacheHitRate:
        inputTokens > 0
          ? Math.min(1, Math.max(0, cachedInputTokens / inputTokens))
          : null,
    };
  }, [dailyUsageById, filteredApis]);
  const activeCount = aggregateApis.filter((api) => api.status === "active").length;
  const routedCount = aggregateApis.filter((api) => api.modelSlugs.length > 0).length;
  const failedCount = aggregateApis.filter((api) => api.lastTestStatus === "failed").length;
  const coolingCount = aggregateApis.filter((api) => {
    return (aggregateApiRuntimeStatusById.get(api.id) || []).some(
      (runtime) =>
        runtime.isCoolingDown && Number(runtime.cooldownUntil || 0) > runtimeStatusNowSeconds,
    );
  }).length;

  const deleteMutation = useMutation({
    mutationFn: (apiId: string) => accountClient.deleteAggregateApi(apiId),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["aggregate-apis"] }),
        queryClient.invalidateQueries({ queryKey: ["managed-models-v2"] }),
        queryClient.invalidateQueries({ queryKey: ["apikeys"] }),
        queryClient.invalidateQueries({ queryKey: ["startup-snapshot"] }),
        queryClient.invalidateQueries({
          queryKey: ["requestlog", "aggregate-api-daily-usage"],
        }),
      ]);
      toast.success(t("聚合 API 已删除"));
    },
    onError: (error: unknown) => {
      toast.error(`${t("删除失败")}: ${error instanceof Error ? error.message : String(error)}`);
    },
  });

  const testMutation = useMutation({
    mutationFn: ({ apiId, model }: { apiId: string; model?: string | null }) =>
      accountClient.probeAggregateApiHealth(apiId, model),
    onMutate: ({ apiId }) => setTestingApiId(apiId),
    onSuccess: (result) => {
      if (result.ok) {
        toast.success(t("连通性测试成功"));
      } else {
        toast.error(result.message || t("连通性测试失败"));
      }
    },
    onSettled: async (_result, _error, variables) => {
      const apiId = variables?.apiId;
      setTestingApiId((current) => (current === apiId ? null : current));
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["aggregate-apis"] }),
        queryClient.invalidateQueries({ queryKey: ["aggregate-api-health"] }),
        queryClient.invalidateQueries({ queryKey: ["aggregate-api-health-costs"] }),
      ]);
    },
  });

  const activeProbeMutation = useMutation({
    mutationFn: async ({ apiId, enabled, probeModel }: { apiId: string; enabled: boolean; probeModel?: string | null }) => {
      const detail = await accountClient.getAggregateApiHealth(apiId);
      return accountClient.updateAggregateApiHealthConfig(apiId, {
        enabled,
        probeIntervalSecs: detail.config.probeIntervalSecs || 900,
        probeTimeoutMs: detail.config.probeTimeoutMs || 30_000,
        probeModel: probeModel === undefined ? detail.config.probeModel : probeModel,
      });
    },
    onSuccess: async (_result, variables) => {
      await queryClient.invalidateQueries({ queryKey: ["aggregate-api-health"] });
      toast.success(variables.enabled ? t("主动监测已开启") : t("主动监测已关闭"));
    },
    onError: (error: unknown) => toast.error(`${t("更新监测设置失败")}: ${getAppErrorMessage(error)}`),
  });

  const resetCooldownMutation = useMutation({
    mutationFn: (apiId: string) => accountClient.resetAggregateApiRuntimeStatus(apiId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["aggregate-api-runtime-status"] });
      toast.success(t("已解除冷却，API 已重新加入路由候选"));
      setResetCooldownApi(null);
    },
    onError: (error: unknown) => {
      toast.error(`${t("解除冷却失败")}: ${getAppErrorMessage(error)}`);
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

  const refreshAllBalancesMutation = useMutation({
    mutationFn: async ({ apiIds }: { apiIds: string[] }) =>
      Promise.allSettled(
        apiIds.map((apiId) => accountClient.refreshAggregateApiBalance(apiId)),
      ),
    onMutate: () => setRefreshingBalances(true),
    onSuccess: (results) => {
      const successCount = results.filter(
        (result) => result.status === "fulfilled" && result.value.ok,
      ).length;
      const failCount = results.length - successCount;
      if (failCount === 0) {
        toast.success(t("余额刷新完成：{count} 个成功", { count: successCount }));
        return;
      }
      toast.warning(
        t("余额刷新完成：{success} 个成功，{fail} 个失败", {
          success: successCount,
          fail: failCount,
        }),
      );
    },
    onError: (error: unknown) => {
      toast.error(`${t("批量刷新余额失败")}: ${getAppErrorMessage(error)}`);
    },
    onSettled: async () => {
      setRefreshingBalances(false);
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
      <PageWorkspace className="gap-3">
        <PageHeader
          eyebrow={t("显式路由")}
          title={t("聚合 API")}
          description={t("这里只管理上游连接；模型路由在“模型管理”中显式配置，页面不会访问供应商 `/models`。")}
          actions={
            <div className="flex flex-wrap items-center justify-end gap-2">
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={!isServiceReady || refreshingBalances}
                onClick={() => {
                  if (balanceEnabledApiIds.length === 0) {
                    toast.info(t("暂无已启用余额检测的聚合 API"));
                    return;
                  }
                  refreshAllBalancesMutation.mutate({ apiIds: balanceEnabledApiIds });
                }}
              >
                <RefreshCw
                  className={`mr-1.5 h-4 w-4 ${refreshingBalances ? "animate-spin" : ""}`}
                />
                {t("刷新余额")}
              </Button>
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
            </div>
          }
        />

        <section className="grid grid-cols-2 gap-1.5 sm:grid-cols-3 lg:grid-cols-5">
          <MetricCard title={t("总数")} value={aggregateApis.length} icon={Database} tone="blue" />
          <MetricCard title={t("已启用")} value={activeCount} icon={ShieldCheck} tone="emerald" />
          <MetricCard title={t("已有模型路由")} value={routedCount} icon={Gauge} tone="violet" />
          <MetricCard title={t("测试失败")} value={failedCount} icon={Unplug} tone="rose" />
          <MetricCard title={t("冷却中")} value={coolingCount} icon={RotateCcw} tone="amber" />
        </section>

        <section className="grid grid-cols-1 gap-1.5 sm:grid-cols-3">
          <MetricCard
            title={t("今日 Token")}
            value={
              dailyUsageQuery.isLoading
                ? "..."
                : formatMillionTokenAmount(filteredDailyUsageSummary.billableTotalTokens)
            }
            detail={
              filteredDailyUsageSummary.requestCount > 0
                ? `${t("请求")} ${filteredDailyUsageSummary.requestCount} · ${t("含 Guard 重试")}`
                : t("今日无请求")
            }
            icon={Coins}
            tone="blue"
          />
          <MetricCard
            title={t("今日费用")}
            value={
              dailyUsageQuery.isLoading
                ? "..."
                : formatUsdAmount(filteredDailyUsageSummary.billableEstimatedCostUsd)
            }
            detail={t("含 Guard 重试")}
            icon={CircleDollarSign}
            tone="emerald"
          />
          <MetricCard
            title={t("平均缓存率")}
            value={
              dailyUsageQuery.isLoading
                ? "..."
                : formatCacheRateValue(filteredDailyUsageSummary.cacheHitRate)
            }
            detail={
              filteredDailyUsageSummary.inputTokens > 0
                ? `${t("缓存")} ${formatMillionTokenAmount(filteredDailyUsageSummary.cachedInputTokens)} / ${t("输入")} ${formatMillionTokenAmount(filteredDailyUsageSummary.inputTokens)}`
                : t("跟随当前筛选")
            }
            icon={Percent}
            tone="violet"
          />
        </section>

        <Card className="glass-card overflow-hidden py-0">
          <CardHeader className="border-b border-border/50 px-3 py-2">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <CardTitle className="text-base">{t("今日模型用量")}</CardTitle>
                <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
                  {t("按模型汇总当天 Token、费用与缓存率。")}
                </p>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <span className="text-[11px] text-muted-foreground">
                  {modelDailyUsageQuery.isLoading
                    ? "..."
                    : `${(modelDailyUsageQuery.data || []).length} ${t("个模型")}`}
                </span>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-7 gap-1 px-2 text-xs"
                  aria-expanded={modelDailyUsageExpanded}
                  aria-controls="model-daily-usage-table"
                  onClick={() => setModelDailyUsageExpanded((expanded) => !expanded)}
                >
                  {modelDailyUsageExpanded ? t("收起") : t("展开")}
                  <ChevronDown
                    className={`h-3.5 w-3.5 transition-transform ${
                      modelDailyUsageExpanded ? "rotate-180" : ""
                    }`}
                  />
                </Button>
              </div>
            </div>
          </CardHeader>
          {modelDailyUsageExpanded ? (
            <CardContent id="model-daily-usage-table" className="p-0">
              <div className="max-h-[180px] overflow-auto">
                <Table className="text-xs">
                  <TableHeader>
                    <TableRow className="hover:bg-transparent">
                      <TableHead className="sticky top-0 h-8 bg-card">{t("模型")}</TableHead>
                      <TableHead className="sticky top-0 h-8 bg-card">{t("请求")}</TableHead>
                      <TableHead className="sticky top-0 h-8 bg-card">{t("Token")}</TableHead>
                      <TableHead className="sticky top-0 h-8 bg-card">{t("费用")}</TableHead>
                      <TableHead className="sticky top-0 h-8 bg-card">{t("缓存率")}</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {modelDailyUsageQuery.isLoading ? (
                      Array.from({ length: 3 }).map((_, index) => (
                        <TableRow key={index}>
                          {Array.from({ length: 5 }).map((__, cell) => (
                            <TableCell key={cell} className="py-1.5">
                              <Skeleton className="h-4 w-full" />
                            </TableCell>
                          ))}
                        </TableRow>
                      ))
                    ) : (modelDailyUsageQuery.data || []).length === 0 ? (
                      <TableRow>
                        <TableCell colSpan={5} className="h-16 text-center text-muted-foreground">
                          {t("今日无请求")}
                        </TableCell>
                      </TableRow>
                    ) : (
                      (modelDailyUsageQuery.data || []).map((usage) => (
                        <TableRow key={usage.model}>
                          <TableCell className="max-w-[220px] py-1.5">
                            <Tooltip>
                              <TooltipTrigger
                                render={<div />}
                                className="cursor-help truncate font-medium"
                              >
                                {usage.model}
                              </TooltipTrigger>
                              <TooltipContent className="max-w-sm whitespace-pre-wrap break-words">
                                {buildModelDailyUsageTooltip(usage, t)}
                              </TooltipContent>
                            </Tooltip>
                          </TableCell>
                          <TableCell className="py-1.5 tabular-nums">{usage.requestCount}</TableCell>
                          <TableCell className="py-1.5 font-mono tabular-nums">
                            {formatMillionTokenAmount(usage.totalTokens)}
                          </TableCell>
                          <TableCell className="py-1.5 font-mono tabular-nums">
                            {formatUsdAmount(usage.estimatedCostUsd)}
                          </TableCell>
                          <TableCell className="py-1.5 tabular-nums">
                            {formatCacheRateValue(usage.cacheHitRate)}
                          </TableCell>
                        </TableRow>
                      ))
                    )}
                  </TableBody>
                </Table>
              </div>
            </CardContent>
          ) : null}
        </Card>

        <Card className="glass-card overflow-hidden py-0">
          <CardHeader className="border-b border-border/50 px-3 py-2">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <CardTitle className="text-base">{t("上游连接")}</CardTitle>
                <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
                  {t("连通性测试只使用已配置路由对应的模型。")}
                </p>
              </div>
              <Select value={providerFilter} onValueChange={(value) => setProviderFilter(value || "all")}>
                <SelectTrigger className="h-8 w-[140px]"><SelectValue /></SelectTrigger>
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
              <Table className="text-xs">
                <TableHeader>
                  <TableRow className="hover:bg-transparent">
                    <TableHead className="h-9">{t("排序")}</TableHead>
                    <TableHead className="h-9">{t("供应商")}</TableHead>
                    <TableHead className="h-9">{t("类型")}</TableHead>
                    <TableHead className="h-9">{t("密钥")}</TableHead>
                    <TableHead className="h-9">{t("模型路由")}</TableHead>
                    <TableHead className="h-9">{t("余额")}</TableHead>
                    <TableHead className="h-9">{t("今日用量")}</TableHead>
                    <TableHead className="h-9">{t("运行状态")}</TableHead>
                    <TableHead className="h-9">{t("健康监测")}</TableHead>
                    <TableHead className="h-9">{t("连通性")}</TableHead>
                    <TableHead className="h-9">{t("启用")}</TableHead>
                    <TableHead className="h-9 text-right">{t("操作")}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {isLoading ? (
                    Array.from({ length: 4 }).map((_, index) => (
                      <TableRow key={index}>
                        {Array.from({ length: AGGREGATE_API_TABLE_COLUMNS }).map((__, cell) => (
                          <TableCell key={cell}><Skeleton className="h-5 w-full" /></TableCell>
                        ))}
                      </TableRow>
                    ))
                  ) : filteredApis.length === 0 ? (
                    <TableRow>
                      <TableCell colSpan={AGGREGATE_API_TABLE_COLUMNS} className="h-28 text-center text-muted-foreground">
                        {t("暂无聚合 API，点击右上角新建")}
                      </TableCell>
                    </TableRow>
                  ) : (
                    filteredApis.map((api) => {
                      const revealed = revealedSecrets[api.id];
                      const balance = parseBalanceSnapshot(api);
                      const testError = String(api.lastTestError || "").trim();
                      const runtimeStatuses = aggregateApiRuntimeStatusById.get(api.id) || [];
                      const coolingStatuses = runtimeStatuses.filter(
                        (runtime) =>
                          runtime.isCoolingDown &&
                          Number(runtime.cooldownUntil || 0) > runtimeStatusNowSeconds,
                      );
                      const runtimeStatus = coolingStatuses[0] || runtimeStatuses[0];
                      const isCoolingDown = coolingStatuses.length > 0;
                      const usage = dailyUsageById.get(api.id);
                      const health = healthByApiId.get(api.id);
                      const probeCost = probeCostByApiId.get(api.id);
                      const healthMeta = HEALTH_STATE_META[health?.state || "unknown"];
                      return (
                        <TableRow key={api.id}>
                          <TableCell className="py-2 font-mono tabular-nums">
                            {api.sort}
                          </TableCell>
                          <TableCell className="min-w-[180px] py-2">
                            <Tooltip>
                              <TooltipTrigger
                                render={<div />}
                                className="grid max-w-[220px] cursor-help gap-0.5 text-left"
                              >
                                <div className="truncate text-sm font-medium leading-5">
                                  {api.supplierName || api.id}
                                </div>
                                <div className="truncate font-mono text-[10px] leading-4 text-muted-foreground">
                                  {api.url}
                                </div>
                              </TooltipTrigger>
                              <TooltipContent className="max-w-sm space-y-1 text-xs">
                                <div className="break-all font-medium">{api.supplierName || api.id}</div>
                                <div className="break-all font-mono text-[11px]">{api.url}</div>
                                <div className="text-muted-foreground">
                                  {t("创建时间")}: {formatTsFromSeconds(api.createdAt, "-")}
                                </div>
                              </TooltipContent>
                            </Tooltip>
                          </TableCell>
                          <TableCell className="py-2">
                            <Badge variant="secondary" className="h-5 text-[10px]">
                              {api.providerType === "compatible"
                                ? t("通用兼容（Codex + Claude）")
                                : PROVIDER_LABELS[api.providerType] || api.providerType}
                            </Badge>
                          </TableCell>
                          <TableCell className="py-2">
                            <div className="flex items-center gap-1">
                              <code className="max-w-[140px] truncate rounded border bg-muted/40 px-1.5 py-0.5 text-[10px]">
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
                          <TableCell className="min-w-[140px] max-w-[180px] py-2">
                            {api.modelSlugs.length > 0 ? (
                              <Tooltip>
                                <TooltipTrigger
                                  render={<div />}
                                  className="flex max-w-[170px] cursor-help items-center gap-1"
                                >
                                  <Badge
                                    variant="outline"
                                    className="h-5 max-w-[110px] truncate px-1.5 text-[10px]"
                                  >
                                    {api.modelSlugs[0]}
                                  </Badge>
                                  {api.modelSlugs.length > 1 ? (
                                    <Badge
                                      variant="secondary"
                                      className="h-5 shrink-0 px-1.5 text-[10px]"
                                    >
                                      +{api.modelSlugs.length - 1}
                                    </Badge>
                                  ) : null}
                                </TooltipTrigger>
                                <TooltipContent className="max-w-sm">
                                  <div className="mb-1 text-[11px] font-medium text-muted-foreground">
                                    {t("模型路由")} · {api.modelSlugs.length}
                                  </div>
                                  <div className="flex max-h-48 flex-wrap gap-1 overflow-y-auto">
                                    {api.modelSlugs.map((slug) => (
                                      <Badge key={slug} variant="outline" className="text-[10px]">
                                        {slug}
                                      </Badge>
                                    ))}
                                  </div>
                                </TooltipContent>
                              </Tooltip>
                            ) : (
                              <Badge variant="destructive" className="h-5 text-[10px]">
                                {t("未配置模型路由")}
                              </Badge>
                            )}
                          </TableCell>
                          <TableCell className="py-2">
                            <div className="flex items-center gap-1">
                              <span className="font-mono text-xs">{formatBalance(balance)}</span>
                              {api.balanceQueryEnabled ? (
                                <Button type="button" variant="ghost" size="icon" aria-label={t("刷新余额")} disabled={refreshingBalanceId === api.id || refreshingBalances} onClick={() => balanceMutation.mutate(api.id)}>
                                  <RefreshCw className={`h-4 w-4 ${refreshingBalanceId === api.id ? "animate-spin" : ""}`} />
                                </Button>
                              ) : null}
                            </div>
                          </TableCell>
                          <TableCell className="min-w-[130px] py-2">
                            {dailyUsageQuery.isLoading ? (
                              <Skeleton className="h-6 w-24" />
                            ) : !usage || usage.requestCount <= 0 ? (
                              <span className="text-xs text-muted-foreground">
                                {t("今日无请求")}
                              </span>
                            ) : (
                              <Tooltip>
                                <TooltipTrigger
                                  render={<div />}
                                  className="grid max-w-[170px] cursor-help gap-0.5 text-left"
                                >
                                  <span className="truncate text-xs font-semibold text-foreground">
                                    {formatMillionTokenAmount(usage.billableTotalTokens)} tok
                                  </span>
                                  <span className="truncate text-[10px] text-muted-foreground">
                                    {formatUsdAmount(usage.billableEstimatedCostUsd)} · cache{" "}
                                    {formatCacheRateValue(usage.cacheHitRate)}
                                  </span>
                                </TooltipTrigger>
                                <TooltipContent className="max-w-xs whitespace-pre-wrap break-words">
                                  {buildDailyUsageTooltip(usage, t)}
                                </TooltipContent>
                              </Tooltip>
                            )}
                          </TableCell>
                          <TableCell className="min-w-[120px] py-2 align-middle">
                            {isCoolingDown && runtimeStatus ? (
                              <Tooltip>
                                <TooltipTrigger
                                  render={<div />}
                                  className="flex flex-col items-start gap-1"
                                >
                                  <Badge className="h-5 w-fit border-amber-500/20 bg-amber-500/10 text-[10px] text-amber-600 dark:text-amber-300">
                                    {coolingStatuses.length > 1
                                      ? `${coolingStatuses.length} ${t("个模型冷却中")}`
                                      : t("冷却中")}{" "}
                                    {formatCooldownRemaining(
                                      runtimeStatus.cooldownUntil,
                                      runtimeStatusNowSeconds,
                                    )}
                                  </Badge>
                                  <div className="flex items-center gap-1">
                                    <span className="text-[10px] text-muted-foreground">
                                      {t("连续失败")} {runtimeStatus.consecutiveFailures}/
                                      {runtimeStatus.failureThreshold}
                                    </span>
                                    <Button
                                      type="button"
                                      variant="ghost"
                                      size="sm"
                                      className="h-5 w-fit gap-1 px-1 text-[10px] text-amber-700 hover:text-amber-800 dark:text-amber-300 dark:hover:text-amber-200"
                                      disabled={!isServiceReady || resetCooldownMutation.isPending}
                                      onClick={() => setResetCooldownApi(api)}
                                    >
                                      <RotateCcw className="h-3 w-3" />
                                      {t("解除冷却")}
                                    </Button>
                                  </div>
                                </TooltipTrigger>
                                <TooltipContent className="max-w-sm space-y-1 text-xs">
                                  <div className="grid gap-1">
                                    <span>
                                      {t("模型")}: {runtimeStatus.upstreamModel || t("未指定")}
                                    </span>
                                    <span>{runtimeStatus.reason || t("连续上游请求失败")}</span>
                                    <span>
                                      {t("最后失败")}:{" "}
                                      {formatTsFromSeconds(
                                        runtimeStatus.lastFailureAt,
                                        t("未知时间"),
                                      )}
                                    </span>
                                    <span>
                                      {t("冷却截止")}:{" "}
                                      {formatTsFromSeconds(
                                        runtimeStatus.cooldownUntil,
                                        t("未知时间"),
                                      )}
                                    </span>
                                  </div>
                                </TooltipContent>
                              </Tooltip>
                            ) : (
                              <div className="flex flex-col items-start gap-0.5">
                                <Badge className="h-5 w-fit border-emerald-500/20 bg-emerald-500/10 text-[10px] text-emerald-600 dark:text-emerald-300">
                                  {t("正常")}
                                </Badge>
                                {runtimeStatus && runtimeStatus.consecutiveFailures > 0 ? (
                                  <span className="text-[10px] text-muted-foreground">
                                    {t("连续失败")} {runtimeStatus.consecutiveFailures}/
                                    {runtimeStatus.failureThreshold}
                                  </span>
                                ) : null}
                              </div>
                            )}
                          </TableCell>
                          <TableCell className="min-w-[128px] py-2">
                            <Tooltip>
                              <TooltipTrigger render={<div />} className="flex cursor-help flex-col items-start gap-1">
                                <Badge className={`h-5 text-[10px] ${healthMeta.className}`}>
                                  {t(healthMeta.label)}
                                </Badge>
                                <span className="text-[10px] text-muted-foreground">
                                  {health?.latencyMs != null ? `${health.latencyMs} ms` : t("暂无观测")}
                                </span>
                              </TooltipTrigger>
                              <TooltipContent className="max-w-xs space-y-1 text-xs">
                                <div>{health?.errorCategory || t("暂无上游错误")}</div>
                                {health?.errorReason ? <div>{health.errorReason}</div> : null}
                                {health?.lastObservedAt ? <div>{t("最近观测")}: {formatTsFromSeconds(health.lastObservedAt, "-")}</div> : null}
                              </TooltipContent>
                            </Tooltip>
                            <div className="mt-1 flex items-center gap-1">
                              <Switch
                                checked={health?.activeProbeEnabled || false}
                                disabled={!isServiceReady || activeProbeMutation.isPending}
                                onCheckedChange={(enabled) => activeProbeMutation.mutate({ apiId: api.id, enabled })}
                                aria-label={t("主动监测")}
                              />
                              <span className="text-[10px] text-muted-foreground">{t("主动")}</span>
                            </div>
                            <Select
                              value={health?.probeModel || "__auto"}
                              onValueChange={(value) => activeProbeMutation.mutate({
                                apiId: api.id,
                                enabled: health?.activeProbeEnabled || false,
                                probeModel: value === "__auto" ? null : value,
                              })}
                              disabled={!isServiceReady || activeProbeMutation.isPending}
                            >
                              <SelectTrigger
                                className="mt-1 h-6 w-[126px] px-1.5 text-[10px]"
                                aria-label={t("检测模型")}
                              >
                                <SelectValue placeholder={t("自动选择模型")} />
                              </SelectTrigger>
                              <SelectContent>
                                <SelectItem value="__auto">{t("自动选择模型")}</SelectItem>
                                {(health?.availableProbeModels || []).map((model) => (
                                  <SelectItem key={model} value={model}>{model}</SelectItem>
                                ))}
                              </SelectContent>
                            </Select>
                            <Tooltip>
                              <TooltipTrigger render={<span />} className="mt-1 block cursor-help text-[10px] text-muted-foreground">
                                {probeCost?.probeCount
                                  ? `${t("本月探测")} ${probeCost.unknownCostProbeCount > 0
                                    ? `${probeCost.pricedProbeCount > 0 ? `${formatUsdAmount(probeCost.estimatedCostUsd)} + ` : ""}${t("金额未知")} ${probeCost.unknownCostProbeCount} ${t("次")}`
                                    : formatUsdAmount(probeCost.estimatedCostUsd)} · ${probeCost.probeCount} ${t("次")}`
                                  : t("本月暂无探测")}
                              </TooltipTrigger>
                              <TooltipContent className="max-w-xs space-y-1 text-xs">
                                <div>{t("主动")} {probeCost?.scheduledProbeCount || 0} · {t("半开恢复")} {probeCost?.halfOpenProbeCount || 0} · {t("手动")} {probeCost?.manualProbeCount || 0}</div>
                                <div>{t("已定价")} {probeCost?.pricedProbeCount || 0} · {t("金额未知")} {probeCost?.unknownCostProbeCount || 0}</div>
                              </TooltipContent>
                            </Tooltip>
                          </TableCell>
                          <TableCell className="py-2">
                            <div className="flex items-center gap-1.5">
                              {api.lastTestStatus === "failed" && testError ? (
                                <Tooltip>
                                  <TooltipTrigger
                                    render={<span />}
                                    className="inline-flex cursor-help"
                                  >
                                    <Badge variant="destructive" className="h-5 text-[10px]">
                                      {t("失败")}
                                    </Badge>
                                  </TooltipTrigger>
                                  <TooltipContent className="max-w-sm whitespace-pre-wrap break-words">
                                    {testError}
                                  </TooltipContent>
                                </Tooltip>
                              ) : (
                                <Badge
                                  variant={
                                    api.lastTestStatus === "success"
                                      ? "default"
                                      : api.lastTestStatus === "failed"
                                        ? "destructive"
                                        : "secondary"
                                  }
                                  className="h-5 text-[10px]"
                                >
                                  {api.lastTestStatus === "success"
                                    ? t("已连通")
                                    : api.lastTestStatus === "failed"
                                      ? t("失败")
                                      : t("未测试")}
                                </Badge>
                              )}
                              <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                className="h-6 px-1.5 text-[11px]"
                                disabled={testingApiId === api.id || api.modelSlugs.length === 0}
                                onClick={() => testMutation.mutate({
                                  apiId: api.id,
                                  model: health?.probeModel,
                                })}
                              >
                                {testingApiId === api.id ? t("测试中...") : t("测试")}
                              </Button>
                            </div>
                          </TableCell>
                          <TableCell className="py-2">
                            <Switch
                              checked={api.status === "active"}
                              disabled={togglingApiId === api.id}
                              onCheckedChange={(enabled) => toggleMutation.mutate({ api, enabled })}
                            />
                          </TableCell>
                          <TableCell className="py-2">
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
            <CardHeader className="flex-row items-center justify-between gap-3 py-3">
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

      <ConfirmDialog
        open={Boolean(resetCooldownApi)}
        onOpenChange={(open) => {
          if (!open) setResetCooldownApi(null);
        }}
        title={t("解除冷却")}
        description={t(
          "解除后该上游会立即重新进入路由候选。若上游仍不稳定，可能很快再次触发冷却。",
        )}
        confirmText={t("确认解除")}
        onConfirm={() => {
          if (!resetCooldownApi) return;
          resetCooldownMutation.mutate(resetCooldownApi.id);
        }}
      />
    </>
  );
}
