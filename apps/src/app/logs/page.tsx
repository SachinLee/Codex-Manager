"use client";

import { Suspense, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "next/navigation";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Database } from "lucide-react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/modals/confirm-dialog";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { accountClient } from "@/lib/api/account-client";
import { buildManagedModelSelectorQueryKey } from "@/lib/api/account-query-keys";
import {
  managedModelsV2Client,
  managedModelV2ToModelInfo,
} from "@/lib/api/managed-models-v2";
import {
  buildStartupSnapshotQueryKey,
  STARTUP_SNAPSHOT_REQUEST_LOG_LIMIT,
} from "@/lib/api/startup-snapshot";
import { serviceClient } from "@/lib/api/service-client";
import { useDesktopPageActive } from "@/hooks/useDesktopPageActive";
import { useDeferredDesktopActivation } from "@/hooks/useDeferredDesktopActivation";
import {
  isAdminRole,
  resolveSessionRole,
  useAppSession,
} from "@/hooks/useAppSession";
import { useLocalDayRange } from "@/hooks/useLocalDayRange";
import { usePageTransitionReady } from "@/hooks/usePageTransitionReady";
import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";
import { useCodexProfileModeStatus } from "@/hooks/useCodexProfileModeStatus";
import { DASHBOARD_ADMIN_USAGE_QUERY_KEY } from "@/hooks/useDashboardAdminUsageSummary";
import { MEMBER_DASHBOARD_SUMMARY_QUERY_KEY } from "@/hooks/useMemberDashboardSummary";
import { useI18n } from "@/lib/i18n/provider";
import { useAppStore } from "@/lib/store/useAppStore";
import { RequestLogsTabContent } from "./page-sections";
import {
  buildFixedTimePreset,
  formatCompactKeyLabel,
  fromDateTimeLocalValue,
  isLegacyRequestLogStructuredQuery,
  LogsPageSkeleton,
  resolveRequestLogTitleSessionIds,
  type LogsTab,
  type StatusFilter,
  type TimeRangePreset,
} from "./page-helpers";
import { buildSummaryPlaceholder } from "./page-cells";
import {
  AccountListResult,
  ApiKey,
  RequestLogListWithSummaryResult,
  StartupSnapshot,
} from "@/types";

const REQUEST_LOG_SESSION_LOOKUP_QUERY_KEY = ["requestlog", "session-titles"] as const;
const LOG_SEARCH_DEBOUNCE_MS = 300;
const LOG_REFRESH_ACTIVE_MS = 5_000;
const LOG_REFRESH_FILTERED_MS = 10_000;
const LOG_REFRESH_EMPTY_MS = 15_000;

function getLogRefreshIntervalMs(
  result: RequestLogListWithSummaryResult | undefined,
  hasActiveFilter: boolean,
): number {
  if (result && result.total === 0) {
    return LOG_REFRESH_EMPTY_MS;
  }
  return hasActiveFilter ? LOG_REFRESH_FILTERED_MS : LOG_REFRESH_ACTIVE_MS;
}

function LogsPageContent() {
  const { t } = useI18n();
  const localDayRange = useLocalDayRange();
  const searchParams = useSearchParams();
  const serviceStatus = useAppStore((state) => state.serviceStatus);
  const { isDesktopRuntime } = useRuntimeCapabilities();
  const { data: session, isLoading: isSessionLoading } = useAppSession();
  const role = resolveSessionRole(session, isSessionLoading, isDesktopRuntime);
  const isAdminMode = isAdminRole(role);
  const isPageActive = useDesktopPageActive("/logs/");
  const { isDirectAccountMode } = useCodexProfileModeStatus({
    enabled: isAdminMode && isPageActive,
    refetchIntervalMs: 10_000,
  });
  const queryClient = useQueryClient();
  const areLogQueriesEnabled = useDeferredDesktopActivation(serviceStatus.connected);
  const routeQuery = searchParams.get("query") || "";
  const routeLegacyQuery = isLegacyRequestLogStructuredQuery(routeQuery)
    ? routeQuery
    : "";
  const routeTitleSeed = routeLegacyQuery ? "" : routeQuery;
  const [titleInput, setTitleInput] = useState(routeTitleSeed);
  const [titleSearch, setTitleSearch] = useState(routeTitleSeed);
  const [legacyQuery, setLegacyQuery] = useState(routeLegacyQuery);
  const [modelFilter, setModelFilter] = useState("all");
  const [keyIdFilter, setKeyIdFilter] = useState("all");
  const [filter, setFilter] = useState<StatusFilter>("all");
  const [timePreset, setTimePreset] = useState<TimeRangePreset>("all");
  const [startTimeInput, setStartTimeInput] = useState("");
  const [endTimeInput, setEndTimeInput] = useState("");
  const [pageSize, setPageSize] = useState("10");
  const [page, setPage] = useState(1);
  const [clearConfirmOpen, setClearConfirmOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<LogsTab>("requests");
  const pageSizeNumber = Number(pageSize) || 10;
  const startTs = useMemo(
    () => fromDateTimeLocalValue(startTimeInput),
    [startTimeInput],
  );
  const endTs = useMemo(() => fromDateTimeLocalValue(endTimeInput), [endTimeInput]);
  const hasActiveTimeRange = startTs != null || endTs != null;
  const hasActiveLogFilter =
    Boolean(legacyQuery) ||
    Boolean(titleSearch.trim()) ||
    modelFilter !== "all" ||
    keyIdFilter !== "all" ||
    filter !== "all" ||
    hasActiveTimeRange;
  const startupSnapshot = queryClient.getQueryData<StartupSnapshot>(
    buildStartupSnapshotQueryKey(
      serviceStatus.addr,
      STARTUP_SNAPSHOT_REQUEST_LOG_LIMIT,
      localDayRange.dayStartTs,
      localDayRange.dayEndTs,
    )
  );
  const startupAccounts = startupSnapshot?.accounts || [];
  const startupApiKeys = startupSnapshot?.apiKeys || [];
  const startupRequestLogs = startupSnapshot?.requestLogs || [];
  const startupApiModels = startupSnapshot?.apiModels;
  const canUseStartupLogsPlaceholder =
    !routeQuery.trim() &&
    !titleInput.trim() &&
    !titleSearch.trim() &&
    modelFilter === "all" &&
    keyIdFilter === "all" &&
    filter === "all" &&
    page === 1 &&
    !hasActiveTimeRange;
  const hasStartupLogsSnapshot =
    canUseStartupLogsPlaceholder && startupRequestLogs.length > 0;

  const { data: accountsResult } = useQuery({
    queryKey: ["accounts", "lookup"],
    queryFn: () => accountClient.list(),
    enabled: areLogQueriesEnabled && isPageActive && isAdminMode,
    staleTime: 60_000,
    retry: 1,
    placeholderData: (previousData): AccountListResult | undefined =>
      previousData ||
      (startupAccounts.length > 0
        ? {
            items: startupAccounts,
            total: startupAccounts.length,
            page: 1,
            pageSize: startupAccounts.length,
          }
        : undefined),
  });

  const { data: apiKeysResult } = useQuery({
    queryKey: ["apikeys", "lookup"],
    queryFn: () => accountClient.listApiKeys(),
    enabled: areLogQueriesEnabled && isPageActive,
    staleTime: 60_000,
    retry: 1,
    placeholderData: (previousData): ApiKey[] | undefined =>
      previousData || (startupApiKeys.length > 0 ? startupApiKeys : undefined),
  });

  const { data: aggregateApisResult } = useQuery({
    queryKey: ["aggregate-apis", "lookup"],
    queryFn: () => accountClient.listAggregateApis(),
    enabled: areLogQueriesEnabled && isPageActive && isAdminMode,
    staleTime: 60_000,
    retry: 1,
  });


  // 模型下拉沿用密钥管理页的目录来源：优先实时模型目录，回落到启动快照。
  const { data: modelCatalogResult } = useQuery({
    queryKey: buildManagedModelSelectorQueryKey(serviceStatus.addr),
    queryFn: async () => {
      const result = await managedModelsV2Client.list(false, serviceStatus.addr);
      return { models: result.items.map(managedModelV2ToModelInfo) };
    },
    enabled: areLogQueriesEnabled && isPageActive && isAdminMode,
    staleTime: 60_000,
    retry: 1,
    placeholderData: () =>
      startupApiModels?.models?.length ? startupApiModels : undefined,
  });
  const { data: requestLogSessions = [] } = useQuery({
    queryKey: REQUEST_LOG_SESSION_LOOKUP_QUERY_KEY,
    queryFn: () => serviceClient.listRequestLogSessionTitles({ limit: 2000 }),
    enabled: areLogQueriesEnabled && isPageActive && isAdminMode,
    staleTime: 5_000,
    refetchInterval: 5000,
    retry: 1,
  });

  // 标题没有独立数据库字段，先经会话标题侧车解析成会话 ID 集合再交给服务端过滤。
  const titleSessionIds = useMemo(
    () => resolveRequestLogTitleSessionIds(titleSearch, requestLogSessions),
    [titleSearch, requestLogSessions],
  );
  const modelParam = modelFilter === "all" ? null : modelFilter;
  const keyIdParam = keyIdFilter === "all" ? null : keyIdFilter;
  const { data: logsResult, isLoading, isError: isLogsError } = useQuery({
    queryKey: [
      "logs",
      "list-with-summary",
      legacyQuery,
      titleSessionIds,
      modelParam,
      keyIdParam,
      filter,
      startTs,
      endTs,
      page,
      pageSizeNumber,
    ],
    queryFn: ({ signal }) =>
      serviceClient.listRequestLogsWithSummary(
        {
          query: legacyQuery,
          statusFilter: filter,
          startTs,
          endTs,
          page,
          pageSize: pageSizeNumber,
          sessionIds: titleSessionIds,
          model: modelParam,
          keyId: keyIdParam,
        },
        { signal },
      ),
    enabled: areLogQueriesEnabled && isPageActive,
    refetchInterval: (query) => {
      if (!areLogQueriesEnabled || !isPageActive) {
        return false;
      }
      if (typeof document !== "undefined" && document.visibilityState !== "visible") {
        return false;
      }
      return getLogRefreshIntervalMs(
        query.state.data as RequestLogListWithSummaryResult | undefined,
        hasActiveLogFilter,
      );
    },
    refetchIntervalInBackground: false,
    retry: 1,
    placeholderData: (previousData): RequestLogListWithSummaryResult | undefined =>
      previousData ||
      (hasStartupLogsSnapshot
        ? {
            items: startupRequestLogs,
            total: startupRequestLogs.length,
            page: 1,
            pageSize: pageSizeNumber,
            summary: buildSummaryPlaceholder(startupRequestLogs),
          }
        : undefined),
  });

  const clearMutation = useMutation({
    mutationFn: () => serviceClient.clearRequestLogs(),
    onSuccess: async () => {
      queryClient.setQueriesData<RequestLogListWithSummaryResult>(
        { queryKey: ["logs", "list-with-summary"] },
        (previousData) =>
          previousData
            ? {
                ...previousData,
                items: [],
                total: 0,
                page: 1,
                summary: {
                  ...previousData.summary,
                  totalCount: 0,
                  filteredCount: 0,
                  successCount: 0,
                  errorCount: 0,
                  totalTokens: 0,
                  totalCostUsd: 0,
                  inputTokens: 0,
                  cachedInputTokens: 0,
                },
              }
            : previousData,
      );
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["logs"] }),
        queryClient.invalidateQueries({ queryKey: ["today-summary"] }),
        queryClient.invalidateQueries({ queryKey: DASHBOARD_ADMIN_USAGE_QUERY_KEY }),
        queryClient.invalidateQueries({ queryKey: MEMBER_DASHBOARD_SUMMARY_QUERY_KEY }),
        queryClient.invalidateQueries({ queryKey: ["startup-snapshot"] }),
      ]);
      toast.success(t("日志已清空"));
    },
    onError: (error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    },
  });

  const accountNameMap = useMemo(() => {
    return new Map(
      (accountsResult?.items || []).map((account) => [
        account.id,
        account.label || account.name || account.id,
      ]),
    );
  }, [accountsResult?.items]);

  const apiKeyMap = useMemo(() => {
    return new Map((apiKeysResult || []).map((apiKey) => [apiKey.id, apiKey]));
  }, [apiKeysResult]);

  const aggregateApiMap = useMemo(() => {
    return new Map(
      (aggregateApisResult || []).map((aggregateApi) => [
        aggregateApi.id,
        aggregateApi,
      ]),
    );
  }, [aggregateApisResult]);

  const requestLogSessionMap = useMemo(() => {
    return new Map(
      requestLogSessions.map((session) => [session.sessionId, session]),
    );
  }, [requestLogSessions]);

  // 模型下拉取平台模型目录的 slug；日志里的 model 字段就是该 slug。
  const modelOptions = useMemo(() => {
    const slugs = (modelCatalogResult?.models || [])
      .map((model) => String(model.slug || "").trim())
      .filter(Boolean);
    return Array.from(new Set(slugs)).sort((left, right) => left.localeCompare(right));
  }, [modelCatalogResult?.models]);

  // 平台密钥下拉沿用列表里的展示口径：有名称用名称，否则退化成紧凑 ID。
  const keyOptions = useMemo(
    () =>
      (apiKeysResult || [])
        .map((apiKey) => ({
          id: apiKey.id,
          label: String(apiKey.name || "").trim() || formatCompactKeyLabel(apiKey.id),
        }))
        .sort((left, right) => left.label.localeCompare(right.label)),
    [apiKeysResult],
  );

  const logs = logsResult?.items || [];
  const isLogsLoading =
    serviceStatus.connected &&
    !hasStartupLogsSnapshot &&
    (!areLogQueriesEnabled || isLoading);
  usePageTransitionReady(
    "/logs/",
    !serviceStatus.connected ||
      (!isLogsLoading && (Boolean(logsResult?.summary) || isLogsError)),
  );
  const currentPage = logsResult?.page || page;
  const summary = logsResult?.summary || {
    totalCount: logsResult?.total || 0,
    filteredCount: logsResult?.total || 0,
    successCount: 0,
    errorCount: 0,
    totalTokens: 0,
    totalCostUsd: 0,
    guardRetryTotalTokens: 0,
    guardRetryEstimatedCostUsd: 0,
    longContextCount: 0,
    longContextCostUsd: 0,
    longContextUpliftUsd: 0,
    legacyCandidateCount: 0,
    inputTokens: 0,
    cachedInputTokens: 0,
    modelStats: [],
    modelStatsTruncated: false,
  };
  const totalPages = Math.max(
    1,
    Math.ceil((logsResult?.total || 0) / pageSizeNumber),
  );
  const currentRefreshIntervalMs = getLogRefreshIntervalMs(
    logsResult,
    hasActiveLogFilter,
  );

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    const frameId = window.requestAnimationFrame(() => {
      setTitleInput((current) =>
        current === routeTitleSeed ? current : routeTitleSeed,
      );
      setTitleSearch((current) =>
        current === routeTitleSeed ? current : routeTitleSeed,
      );
      setLegacyQuery((current) =>
        current === routeLegacyQuery ? current : routeLegacyQuery,
      );
      setPage(1);
    });
    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, [routeLegacyQuery, routeTitleSeed]);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    const timeoutId = window.setTimeout(() => {
      setTitleSearch((current) => (current === titleInput ? current : titleInput));
      setPage(1);
    }, LOG_SEARCH_DEBOUNCE_MS);
    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [titleInput]);

  useEffect(() => {
    if (isPageActive) {
      return;
    }
    if (typeof window === "undefined") {
      return;
    }
    const frameId = window.requestAnimationFrame(() => {
      setClearConfirmOpen(false);
    });
    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, [isPageActive]);

  useEffect(() => {
    if (timePreset !== "today") {
      return;
    }
    const frameId = window.requestAnimationFrame(() => {
      const todayRange = buildFixedTimePreset(
        "today",
        localDayRange.dayStartTs,
        localDayRange.dayEndTs,
      );
      setStartTimeInput((current) =>
        current === todayRange.startInput ? current : todayRange.startInput,
      );
      setEndTimeInput((current) =>
        current === todayRange.endInput ? current : todayRange.endInput,
      );
    });
    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, [localDayRange.dayEndTs, localDayRange.dayStartTs, timePreset]);

  const currentFilterLabel =
    filter === "all"
      ? t("全部状态")
      : filter === "2xx"
        ? t("成功请求")
        : filter === "4xx"
          ? t("客户端错误")
          : t("服务端错误");
  const currentTimeRangeLabel =
    timePreset === "30m"
      ? t("最近30分钟")
      : timePreset === "2h"
        ? t("最近2小时")
        : timePreset === "24h"
          ? t("最近24小时")
          : timePreset === "today"
            ? t("今天")
            : hasActiveTimeRange
              ? t("自定义时间")
              : t("全部时间");
  const compactMetaText = `${summary.filteredCount}/${summary.totalCount} ${t("条")} · ${currentFilterLabel} · ${currentTimeRangeLabel} · ${
    serviceStatus.connected
      ? currentRefreshIntervalMs === LOG_REFRESH_ACTIVE_MS
        ? t("5 秒刷新")
        : currentRefreshIntervalMs === LOG_REFRESH_FILTERED_MS
          ? t("10 秒刷新")
          : t("15 秒刷新")
      : t("服务未连接")
  }`;

  const applyTimePreset = (preset: TimeRangePreset) => {
    setTimePreset(preset);
    setPage(1);
    if (preset === "all") {
      setStartTimeInput("");
      setEndTimeInput("");
      return;
    }
    if (preset === "custom") {
      return;
    }
    const nextRange = buildFixedTimePreset(
      preset,
      localDayRange.dayStartTs,
      localDayRange.dayEndTs,
    );
    setStartTimeInput(nextRange.startInput);
    setEndTimeInput(nextRange.endInput);
  };

  return (
    <div className="animate-in space-y-5 fade-in duration-500">
      <Tabs
        value={activeTab}
        onValueChange={(value) => {
          if (value === "requests") {
            setActiveTab("requests");
          }
        }}
        className="w-full"
      >
        <TabsList className="glass-card flex h-11 w-full justify-start overflow-x-auto rounded-xl p-1 no-scrollbar lg:w-fit">
          <TabsTrigger value="requests" className="gap-2 px-5 shrink-0">
            <Database className="h-4 w-4" /> {t("请求日志")}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="requests" className="space-y-5">
          <RequestLogsTabContent
            t={t}
            isDirectAccountMode={isDirectAccountMode}
            isAdminMode={isAdminMode}
            serviceConnected={serviceStatus.connected}
            titleSearch={titleInput}
            modelFilter={modelFilter}
            keyIdFilter={keyIdFilter}
            modelOptions={modelOptions}
            keyOptions={keyOptions}
            filter={filter}
            timePreset={timePreset}
            startTimeInput={startTimeInput}
            endTimeInput={endTimeInput}
            compactMetaText={compactMetaText}
            hasActiveTimeRange={hasActiveTimeRange}
            pageSize={pageSize}
            currentFilterLabel={currentFilterLabel}
            summary={summary}
            logs={logs}
            isLogsLoading={isLogsLoading}
            onTitleSearchChange={(value) => {
              setLegacyQuery("");
              setTitleInput(value);
              setPage(1);
            }}
            currentPage={currentPage}
            totalPages={totalPages}
            accountNameMap={accountNameMap}
            apiKeyMap={apiKeyMap}
            aggregateApiMap={aggregateApiMap}
            sessionTitleMap={requestLogSessionMap}
            clearMutationPending={clearMutation.isPending}
            onModelFilterChange={(value) => {
              setLegacyQuery("");
              setModelFilter(value || "all");
              setPage(1);
            }}
            onKeyIdFilterChange={(value) => {
              setLegacyQuery("");
              setKeyIdFilter(value || "all");
              setPage(1);
            }}
            onFilterChange={(value) => {
              setFilter(value);
              setPage(1);
            }}
            onRefresh={() => {
              void Promise.all([
                queryClient.invalidateQueries({ queryKey: ["logs"] }),
                queryClient.invalidateQueries({
                  queryKey: REQUEST_LOG_SESSION_LOOKUP_QUERY_KEY,
                }),
              ]);
            }}
            onOpenClearConfirm={() => setClearConfirmOpen(true)}
            onApplyTimePreset={applyTimePreset}
            onStartTimeChange={(value) => {
              setTimePreset("custom");
              setStartTimeInput(value);
              setPage(1);
            }}
            onEndTimeChange={(value) => {
              setTimePreset("custom");
              setEndTimeInput(value);
              setPage(1);
            }}
            onClearTimeRange={() => applyTimePreset("all")}
            onPageSizeChange={(value) => {
              setPageSize(value || "10");
              setPage(1);
            }}
            onFirstPage={() => setPage(1)}
            onPreviousPage={() => setPage(Math.max(1, currentPage - 1))}
            onNextPage={() => setPage(Math.min(totalPages, currentPage + 1))}
            onJumpPage={setPage}
          />
        </TabsContent>

      </Tabs>
      {isAdminMode ? (
        <ConfirmDialog
          open={clearConfirmOpen}
          onOpenChange={setClearConfirmOpen}
          title={t("清空请求日志")}
          description={t("确定清空全部请求日志吗？该操作不可恢复。")}
          confirmText={t("清空")}
          confirmVariant="destructive"
          onConfirm={() => clearMutation.mutate()}
        />
      ) : null}
    </div>
  );
}

export default function LogsPage() {
  return (
    <Suspense fallback={<LogsPageSkeleton />}>
      <LogsPageContent />
    </Suspense>
  );
}
