"use client";

import { AlertTriangle, Database, DollarSign, RefreshCw, Trash2, Zap } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { buildStaticRouteUrl } from "@/lib/utils/static-routes";
import { formatTsFromSeconds } from "@/lib/utils/usage";
import { formatCacheRate, formatUsdAmount } from "@/lib/utils/billing";
import { cn } from "@/lib/utils";
import { ModelUsageStatsCard } from "./model-usage-stats";
import {
  AccountKeyInfoCell,
  ErrorInfoCell,
  ModelEffortCell,
  RequestRouteInfoCell,
  SessionInfoCell,
} from "./page-cells";
import {
  formatCompactTokenAmount,
  formatDuration,
  formatOutputRate,
  formatReasoningGuardTarget,
  formatTableTokenAmount,
  getStatusBadge,
  isReasoningGuardConverted502,
  searchFieldPlaceholder,
  type SearchField,
  type StatusFilter,
  type TimeRangePreset,
  type TranslateFn,
  resolveAccountDisplayName,
  resolveDisplayedStatusCode,
  SummaryCard,
} from "./page-helpers";
import type { CodexSession } from "@/lib/api/codex-launcher";
import type { AggregateApi, ApiKey, RequestLog, RequestLogFilterSummary } from "@/types";

export function RequestLogsTabContent({
  t,
  isDirectAccountMode,
  isAdminMode,
  serviceConnected,
  search,
  searchField,
  filter,
  timePreset,
  startTimeInput,
  endTimeInput,
  compactMetaText,
  hasActiveTimeRange,
  pageSize,
  currentFilterLabel,
  summary,
  logs,
  isLogsLoading,
  currentPage,
  totalPages,
  accountNameMap,
  apiKeyMap,
  aggregateApiMap,
  codexSessionMap,
  clearMutationPending,
  onSearchChange,
  onSearchFieldChange,
  onFilterChange,
  onRefresh,
  onOpenClearConfirm,
  onApplyTimePreset,
  onStartTimeChange,
  onEndTimeChange,
  onClearTimeRange,
  onPageSizeChange,
  onPreviousPage,
  onNextPage,
}: {
  t: TranslateFn;
  isDirectAccountMode: boolean;
  isAdminMode: boolean;
  serviceConnected: boolean;
  search: string;
  searchField: SearchField;
  filter: StatusFilter;
  timePreset: TimeRangePreset;
  startTimeInput: string;
  endTimeInput: string;
  compactMetaText: string;
  hasActiveTimeRange: boolean;
  pageSize: string;
  currentFilterLabel: string;
  summary: RequestLogFilterSummary;
  logs: RequestLog[];
  isLogsLoading: boolean;
  currentPage: number;
  totalPages: number;
  accountNameMap: Map<string, string>;
  apiKeyMap: Map<string, ApiKey>;
  aggregateApiMap: Map<string, AggregateApi>;
  codexSessionMap: Map<string, CodexSession>;
  clearMutationPending: boolean;
  onSearchChange: (value: string) => void;
  onSearchFieldChange: (value: SearchField) => void;
  onFilterChange: (value: StatusFilter) => void;
  onRefresh: () => void;
  onOpenClearConfirm: () => void;
  onApplyTimePreset: (preset: TimeRangePreset) => void;
  onStartTimeChange: (value: string) => void;
  onEndTimeChange: (value: string) => void;
  onClearTimeRange: () => void;
  onPageSizeChange: (value: string | null) => void;
  onPreviousPage: () => void;
  onNextPage: () => void;
}) {
  return (
    <div className="space-y-5">
      {isDirectAccountMode ? (
        <div className="flex flex-col gap-3 rounded-xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm sm:flex-row sm:items-center sm:justify-between">
          <div className="flex min-w-0 items-start gap-3">
            <AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-600 dark:text-amber-300" />
            <div>
              <div className="font-semibold text-amber-700 dark:text-amber-200">
                {t("账号直连模式不会产生新的 CodexManager 请求日志")}
              </div>
              <div className="mt-1 text-xs text-muted-foreground">
                {t("这里仅展示历史网关请求；如需记录请求，请切换到本地网关模式。")}
              </div>
            </div>
          </div>
          <a
            href={buildStaticRouteUrl("/platform-mode")}
            className="inline-flex h-8 w-fit items-center justify-center rounded-lg border border-amber-500/40 bg-background/70 px-3 text-xs font-medium text-foreground transition-colors hover:bg-background"
          >
            {t("去切换为本地网关")}
          </a>
        </div>
      ) : null}

      <Card className="glass-card shadow-sm">
        <CardContent className="space-y-3 pt-0">
          <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_auto_auto] xl:items-center">
            <div className="flex min-w-0 flex-col gap-2 sm:flex-row sm:items-center">
              <Select
                value={searchField}
                onValueChange={(value) => {
                  if (
                    value === "all" ||
                    value === "model" ||
                    value === "session_title"
                  ) {
                    onSearchFieldChange(value);
                  }
                }}
              >
                <SelectTrigger className="glass-card h-10 w-full rounded-xl sm:w-[140px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectItem value="all">{t("全部字段")}</SelectItem>
                    <SelectItem value="model">{t("模型")}</SelectItem>
                    <SelectItem value="session_title">{t("会话标题")}</SelectItem>
                  </SelectGroup>
                </SelectContent>
              </Select>
              <Input
                placeholder={searchFieldPlaceholder(searchField, t)}
                className="glass-card h-10 min-w-0 flex-1 rounded-xl px-3"
                value={search}
                onChange={(event) => onSearchChange(event.target.value)}
              />
            </div>
            <div className="flex shrink-0 items-center gap-1 rounded-xl border border-border/60 bg-muted/30 p-1">
              {["all", "2xx", "4xx", "5xx"].map((item) => (
                <Button
                  key={item}
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => onFilterChange(item as StatusFilter)}
                  className={cn(
                    "h-auto rounded-lg px-3 py-1.5 text-xs font-semibold uppercase tracking-wide transition-all",
                    filter === item
                      ? "bg-background text-foreground shadow-sm"
                      : "text-muted-foreground hover:bg-background/60 hover:text-foreground",
                  )}
                >
                  {item.toUpperCase()}
                </Button>
              ))}
            </div>
            <div className="flex shrink-0 items-center gap-2 xl:justify-self-end">
              <Button
                variant="outline"
                size="sm"
                className="glass-card h-9 rounded-xl px-3.5"
                onClick={onRefresh}
              >
                <RefreshCw className="mr-1.5 h-4 w-4" /> {t("刷新")}
              </Button>
              {isAdminMode ? (
                <Button
                  variant="destructive"
                  size="sm"
                  className="h-9 rounded-xl px-3.5"
                  onClick={onOpenClearConfirm}
                  disabled={clearMutationPending}
                >
                  <Trash2 className="mr-1.5 h-4 w-4" /> {t("清空日志")}
                </Button>
              ) : null}
            </div>
          </div>

          <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] xl:items-end">
            <div className="space-y-2">
              <div className="text-[11px] font-medium text-muted-foreground">
                {t("快捷时间")}
              </div>
              <div className="flex flex-wrap items-center gap-1 rounded-xl border border-border/60 bg-muted/30 p-1">
                {(
                  [
                    ["all", t("全部时间")],
                    ["30m", t("最近30分钟")],
                    ["2h", t("最近2小时")],
                    ["24h", t("最近24小时")],
                    ["today", t("今天")],
                  ] as Array<[TimeRangePreset, string]>
                ).map(([value, label]) => (
                  <Button
                    key={value}
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => onApplyTimePreset(value)}
                    className={cn(
                      "h-auto rounded-lg px-3 py-1.5 text-xs font-semibold transition-all",
                      timePreset === value
                        ? "bg-background text-foreground shadow-sm"
                        : "text-muted-foreground hover:bg-background/60 hover:text-foreground",
                    )}
                  >
                    {label}
                  </Button>
                ))}
              </div>
            </div>

            <div className="grid gap-2 sm:grid-cols-2">
              <div className="space-y-1">
                <div className="text-[11px] font-medium text-muted-foreground">
                  {t("开始时间")}
                </div>
                <Input
                  type="datetime-local"
                  className="glass-card h-10 rounded-xl px-3"
                  value={startTimeInput}
                  onChange={(event) => onStartTimeChange(event.target.value)}
                />
              </div>
              <div className="space-y-1">
                <div className="text-[11px] font-medium text-muted-foreground">
                  {t("结束时间")}
                </div>
                <Input
                  type="datetime-local"
                  className="glass-card h-10 rounded-xl px-3"
                  value={endTimeInput}
                  onChange={(event) => onEndTimeChange(event.target.value)}
                />
              </div>
            </div>

            <div className="text-[11px] text-muted-foreground xl:justify-self-end xl:text-right">
              <div className="font-medium text-foreground">{compactMetaText}</div>
              {hasActiveTimeRange ? (
                <Button
                  type="button"
                  variant="link"
                  className="mt-1 h-auto p-0 text-xs text-primary hover:underline"
                  onClick={onClearTimeRange}
                >
                  {t("清除时间筛选")}
                </Button>
              ) : null}
            </div>
          </div>
        </CardContent>
      </Card>

      <div className="grid gap-3 lg:grid-cols-[minmax(0,1.25fr)_minmax(240px,0.75fr)] xl:grid-cols-[minmax(360px,1.1fr)_minmax(240px,0.7fr)_minmax(420px,1.25fr)]">
        <Card className="glass-card relative overflow-hidden rounded-2xl border-border/55 py-0 shadow-sm">
          <div
            aria-hidden="true"
            className="pointer-events-none absolute -left-10 -top-14 size-32 rounded-full bg-primary/8 blur-3xl"
          />
          <CardContent className="relative flex min-h-[142px] flex-col p-0">
            <div className="flex items-center justify-between gap-3 px-4 py-3">
              <div>
                <div className="text-[11px] font-semibold tracking-[0.08em] text-muted-foreground uppercase">
                  {t("当前结果")}
                </div>
                <div className="mt-0.5 text-[10px] text-muted-foreground">
                  {t("总日志")} {summary.totalCount} {t("条")}
                  {isDirectAccountMode ? ` · ${t("仅网关流量")}` : ""}
                </div>
              </div>
              <span className="flex size-8 shrink-0 items-center justify-center rounded-xl bg-primary/12 text-primary ring-1 ring-inset ring-primary/10">
                <Zap className="size-4" />
              </span>
            </div>

            <div className="grid flex-1 grid-cols-[minmax(0,1.15fr)_minmax(86px,0.75fr)_minmax(86px,0.75fr)] border-t border-border/40">
              <div className="flex min-w-0 flex-col justify-center px-4 py-3">
                <div className="truncate text-[2rem] leading-none font-semibold tracking-[-0.04em] text-foreground">
                  {summary.filteredCount}
                </div>
                <div className="mt-1 text-[10px] text-muted-foreground">{t("当前结果")}</div>
              </div>
              <div className="flex min-w-0 flex-col justify-center border-l border-border/40 bg-green-500/[0.035] px-3 py-3">
                <div className="truncate text-xl leading-none font-semibold tracking-tight text-green-600 dark:text-green-400">
                  {summary.successCount}
                </div>
                <div className="mt-1 truncate text-[10px] font-medium text-muted-foreground">
                  {t("2XX 成功")}
                </div>
              </div>
              <div className="flex min-w-0 flex-col justify-center border-l border-border/40 bg-red-500/[0.035] px-3 py-3">
                <div className="truncate text-xl leading-none font-semibold tracking-tight text-red-500">
                  {summary.errorCount}
                </div>
                <div className="mt-1 truncate text-[10px] font-medium text-muted-foreground">
                  {t("异常请求")}
                </div>
              </div>
            </div>
          </CardContent>
        </Card>

        <SummaryCard
          title={t("累计Token")}
          value={formatCompactTokenAmount(summary.totalTokens)}
          detail={
            summary.guardRetryTotalTokens > 0
              ? `Guard +${formatCompactTokenAmount(summary.guardRetryTotalTokens)}`
              : null
          }
          description={
            isDirectAccountMode
              ? `${t("当前筛选结果中的总Token")} · ${t("仅网关流量")}`
              : t("当前筛选结果中的总Token")
          }
          icon={Database}
          toneClass="bg-amber-500/12 text-amber-500"
        />

        <Card className="glass-card relative overflow-hidden rounded-2xl border-border/55 py-0 shadow-sm lg:col-span-2 xl:col-span-1">
          <div
            aria-hidden="true"
            className="pointer-events-none absolute -right-14 -top-16 size-40 rounded-full bg-violet-500/10 blur-3xl"
          />
          <CardContent className="relative grid h-full min-h-[142px] grid-cols-1 divide-y divide-border/45 p-0 sm:grid-cols-2 sm:divide-x sm:divide-y-0">
            <div className="flex min-w-0 flex-col p-4">
              <div className="flex items-center justify-between gap-2">
                <span className="truncate text-[11px] font-semibold tracking-[0.08em] text-muted-foreground uppercase">
                  {t("筛选费用")}
                </span>
                <span className="flex size-8 shrink-0 items-center justify-center rounded-xl bg-emerald-500/12 text-emerald-500 ring-1 ring-inset ring-emerald-500/10">
                  <DollarSign className="size-4" />
                </span>
              </div>
              <div className="mt-4 truncate text-[2rem] leading-none font-semibold tracking-[-0.04em] text-foreground">
                {formatUsdAmount(summary.totalCostUsd)}
              </div>
              {summary.guardRetryEstimatedCostUsd > 0 ? (
                <div className="mt-1 truncate text-[11px] font-semibold text-amber-500">
                  Guard +{formatUsdAmount(summary.guardRetryEstimatedCostUsd)}
                </div>
              ) : null}
              <p className="mt-auto pt-3 text-[11px] leading-4 text-muted-foreground">
                {t("当前筛选结果估算费用")}
              </p>
            </div>

            <div className="flex min-w-0 flex-col bg-violet-500/[0.035] p-4">
              <div className="flex items-center justify-between gap-2">
                <span className="truncate text-[11px] font-semibold tracking-[0.08em] text-muted-foreground uppercase">
                  {t("长上下文费用")}
                </span>
                <span className="flex size-8 shrink-0 items-center justify-center rounded-xl bg-violet-500/12 text-violet-500 ring-1 ring-inset ring-violet-500/10">
                  <DollarSign className="size-4" />
                </span>
              </div>
              <div className="mt-4 flex min-w-0 items-end gap-2">
                <div className="truncate text-[2rem] leading-none font-semibold tracking-[-0.04em] text-foreground">
                  {formatUsdAmount(summary.longContextCostUsd)}
                </div>
                {summary.longContextUpliftUsd > 0 ? (
                  <div className="mb-0.5 truncate text-[11px] font-semibold text-violet-500">
                    +{formatUsdAmount(summary.longContextUpliftUsd)}
                  </div>
                ) : null}
              </div>
              <p className="mt-auto pt-3 text-[11px] leading-4 text-muted-foreground">
                {summary.longContextCount} {t("条已按长上下文计价")} ·{" "}
                {summary.legacyCandidateCount} {t("条历史候选")}
              </p>
            </div>
          </CardContent>
        </Card>
      </div>

      <ModelUsageStatsCard
        t={t}
        summary={summary}
        isLoading={isLogsLoading && summary.modelStats.length === 0}
        onModelClick={(model) => {
          if (model && model !== "(unknown)") {
            onSearchFieldChange("model");
            onSearchChange(model);
          }
        }}
      />

      <Card className="glass-card overflow-hidden gap-0 py-0 shadow-sm">
        <CardHeader className="flex min-h-1 items-center border-b border-border/40 bg-[var(--table-section-bg)] py-3">
          <div className="flex w-full flex-col gap-1 xl:flex-row xl:items-center xl:justify-between">
            <div>
              <CardTitle className="text-[15px] font-semibold">
                {t("请求明细 按")}{" "}
                <span className="font-medium text-foreground">{currentFilterLabel}</span>{" "}
                {t("展示")}
              </CardTitle>
            </div>
            <div className="text-xs text-muted-foreground"></div>
          </div>
        </CardHeader>
        <CardContent className="px-0">
          <Table className="min-w-[1934px] table-fixed">
            <TableHeader>
              <TableRow>
                <TableHead className="h-12 w-[200px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                  {t("会话")}
                </TableHead>
                <TableHead className="h-12 w-[150px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                  {t("时间")}
                </TableHead>
                <TableHead className="w-[224px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                  {t("账号 / 密钥")}
                </TableHead>
                <TableHead className="w-[180px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                  {t("模型 / 推理 / 等级")}
                </TableHead>
                <TableHead className="w-[92px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                  {t("状态")}
                </TableHead>
                <TableHead className="w-[188px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                  {t("用时 / 首响 / 输出速率")}
                </TableHead>
                <TableHead className="w-[148px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                  {t("Token")}
                </TableHead>
                <TableHead className="w-[96px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                  {t("缓存率")}
                </TableHead>
                <TableHead className="w-[176px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                  {t("费用")}
                </TableHead>
                <TableHead className="w-[240px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                  {t("类型 / 方法 / 路径")}
                </TableHead>
                <TableHead className="w-[240px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                  {t("错误")}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLogsLoading ? (
                Array.from({ length: 10 }).map((_, index) => (
                  <TableRow key={index}>
                    <TableCell><Skeleton className="h-4 w-28" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-32" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-32" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-24" /></TableCell>
                    <TableCell><Skeleton className="h-6 w-12 rounded-full" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-12" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-12" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-16" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-20" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-40" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-full" /></TableCell>
                  </TableRow>
                ))
              ) : logs.length === 0 ? (
                <TableRow>
                  <TableCell
                    colSpan={11}
                    className="h-52 px-4 text-center text-sm text-muted-foreground"
                  >
                    {!serviceConnected
                      ? t("服务未连接，无法获取日志")
                      : isDirectAccountMode
                        ? t("账号直连模式下不会产生请求日志，如需记录请求请切换到本地网关模式。")
                        : t("暂无请求日志")}
                  </TableCell>
                </TableRow>
              ) : (
                logs.map((log) => (
                  <TableRow key={log.id} className="group text-xs hover:bg-muted/20">
                    <TableCell className="px-4 py-3 align-top">
                      <SessionInfoCell
                        sessionId={log.sessionId}
                        conversationAnchor={log.conversationAnchor}
                        session={codexSessionMap.get(String(log.sessionId || "").trim())}
                      />
                    </TableCell>
                    <TableCell className="px-4 py-3 font-mono text-[11px] text-muted-foreground">
                      {formatTsFromSeconds(log.createdAt, t("未知时间"))}
                    </TableCell>
                    <TableCell className="px-4 py-3 align-top">
                      <AccountKeyInfoCell
                        log={log}
                        accountLabel={resolveAccountDisplayName(log, accountNameMap)}
                        accountNameMap={accountNameMap}
                        apiKeyMap={apiKeyMap}
                        aggregateApiMap={aggregateApiMap}
                      />
                    </TableCell>
                    <TableCell className="px-4 py-3 align-top">
                      <ModelEffortCell log={log} />
                    </TableCell>
                    <TableCell className="px-4 py-3 align-top">
                      <div className="flex flex-col items-start gap-1">
                        {getStatusBadge(resolveDisplayedStatusCode(log))}
                        {isReasoningGuardConverted502(log) ? (
                          <span
                            className="text-[10px] font-medium text-amber-500"
                            title={t("这是 Reasoning Guard 被网关保护转换成的 502，不是真实上游 502。")}
                          >
                            {formatReasoningGuardTarget(log)} -&gt; 502
                          </span>
                        ) : log.guardInternalRetryCount > 0 ? (
                          <span
                            className="text-[10px] font-medium text-emerald-500"
                            title={t("Guard 命中后已在网关内部重试并恢复。")}
                          >
                            Guard retry {log.guardInternalRetryCount}
                          </span>
                        ) : null}
                      </div>
                    </TableCell>
                    <TableCell className="px-4 py-3 align-top font-mono">
                      <span
                        className="inline-flex whitespace-nowrap text-xs text-primary"
                        title={t("首响表示从请求开始到首个上游响应片段的耗时；输出速率按输出 Token / 总用时计算")}
                      >
                        {formatDuration(log.durationMs)}/
                        {formatDuration(log.firstResponseMs)}/
                        {formatOutputRate(log.outputTokens, log.durationMs)}
                      </span>
                    </TableCell>
                    <TableCell className="px-4 py-3 align-top">
                      <div className="flex flex-col gap-0.5 text-[10px] text-muted-foreground">
                        <span>{t("总")} {formatTableTokenAmount(log.totalTokens)}</span>
                        {log.guardRetryTotalTokens > 0 ? (
                          <span className="text-amber-500">
                            Guard +{formatTableTokenAmount(log.guardRetryTotalTokens)}
                          </span>
                        ) : null}
                        {log.billableTotalTokens != null &&
                        log.billableTotalTokens !== log.totalTokens ? (
                          <span className="text-foreground">
                            {t("计费")} {formatTableTokenAmount(log.billableTotalTokens)}
                          </span>
                        ) : null}
                        <span>{t("输入")} {formatTableTokenAmount(log.inputTokens)}</span>
                        <span className="opacity-60">
                          {t("缓存")} {formatTableTokenAmount(log.cachedInputTokens)}
                        </span>
                        <span className="opacity-60">
                          {t("缓存写入")} {formatTableTokenAmount(log.cacheWriteInputTokens)}
                        </span>
                      </div>
                    </TableCell>
                    <TableCell className="px-4 py-3 align-top font-mono text-xs text-muted-foreground">
                      {formatCacheRate(log.inputTokens, log.cachedInputTokens)}
                    </TableCell>
                    <TableCell className="w-[176px] max-w-[176px] overflow-hidden px-4 py-3 align-top font-mono text-xs whitespace-normal text-foreground">
                      <div className="flex min-w-0 flex-col gap-1">
                        <span>{formatUsdAmount(log.estimatedCostUsd)}</span>
                        {log.pricingCostSource === "provider_reported" ? (
                          <span className="w-fit rounded border border-emerald-500/25 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] text-emerald-600 dark:text-emerald-300">
                            {t("官方实际费用")}
                          </span>
                        ) : log.pricingCostSource === "local_estimate" ? (
                          <span className="w-fit rounded border border-sky-500/25 bg-sky-500/10 px-1.5 py-0.5 text-[10px] text-sky-600 dark:text-sky-300">
                            {t("本地估算")}
                          </span>
                        ) : null}
                        {log.pricingContextBand === "long" ? (
                          <span
                            className="w-fit max-w-full truncate rounded border border-violet-500/25 bg-violet-500/10 px-1.5 py-0.5 text-[10px] text-violet-600 dark:text-violet-300"
                            title={`${t("阈值")} ${formatTableTokenAmount(log.longContextThresholdTokens)} · ${t("规则")} ${log.pricingMatchedPattern || "-"}`}
                          >
                            {t("长上下文")}{log.longContextUpliftUsd != null ? ` +${formatUsdAmount(log.longContextUpliftUsd)}` : ""}
                          </span>
                        ) : log.pricingContextBand === "single_tier" ? (
                          <span className="text-[10px] text-muted-foreground">{t("单档价格")}</span>
                        ) : log.pricingContextBand === "legacy_candidate" ? (
                          <span title={t("输入超过当前长上下文阈值，但历史日志未保存实际计价规则。")} className="text-[10px] text-amber-500">
                            {t("历史长上下文候选")}
                          </span>
                        ) : null}
                        {log.pricingContextBand === "long" ? (
                          <div className="grid min-w-0 grid-cols-2 gap-x-2 gap-y-0.5 text-[10px] leading-4 text-muted-foreground">
                            <span className="truncate">{t("普通")} {formatUsdAmount(log.plainInputCostUsd)}</span>
                            <span className="truncate">{t("缓存")} {formatUsdAmount(log.cachedInputCostUsd)}</span>
                            <span className="truncate">{t("写入")} {formatUsdAmount(log.cacheWriteCostUsd)}</span>
                            <span className="truncate">{t("输出")} {formatUsdAmount(log.outputCostUsd)}</span>
                          </div>
                        ) : null}
                        {log.guardRetryEstimatedCostUsd > 0 ? (
                          <span className="text-[10px] text-amber-500">
                            Guard +{formatUsdAmount(log.guardRetryEstimatedCostUsd)}
                          </span>
                        ) : null}
                        {log.billableEstimatedCostUsd != null &&
                        log.billableEstimatedCostUsd !== log.estimatedCostUsd ? (
                          <span className="text-[10px] text-muted-foreground">
                            {t("计费")} {formatUsdAmount(log.billableEstimatedCostUsd)}
                          </span>
                        ) : null}
                      </div>
                    </TableCell>
                    <TableCell className="px-4 py-3 align-top">
                      <RequestRouteInfoCell log={log} />
                    </TableCell>
                    <TableCell className="px-4 py-3 text-left align-top">
                      <ErrorInfoCell log={log} />
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <div className="flex items-center justify-between px-2">
        <div className="text-xs text-muted-foreground">
          {t("共")} {summary.filteredCount} {t("条匹配日志")}
        </div>
        <div className="flex items-center gap-6">
          <div className="flex items-center gap-2">
            <span className="whitespace-nowrap text-xs text-muted-foreground">
              {t("每页显示")}
            </span>
            <Select value={pageSize} onValueChange={onPageSizeChange}>
              <SelectTrigger className="h-8 w-[78px] text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {["5", "10", "20", "50", "100", "200"].map((value) => (
                    <SelectItem key={value} value={value}>
                      {value}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              className="h-8 px-3 text-xs"
              disabled={currentPage <= 1}
              onClick={onPreviousPage}
            >
              {t("上一页")}
            </Button>
            <div className="min-w-[68px] text-center text-xs font-medium">
              {t("第")} {currentPage} / {totalPages} {t("页")}
            </div>
            <Button
              variant="outline"
              size="sm"
              className="h-8 px-3 text-xs"
              disabled={currentPage >= totalPages}
              onClick={onNextPage}
            >
              {t("下一页")}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
