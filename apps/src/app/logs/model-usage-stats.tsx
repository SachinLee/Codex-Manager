"use client";

import { useMemo, useState } from "react";
import { BarChart3, ChevronDown, ChevronUp } from "lucide-react";
import { Button } from "@/components/ui/button";
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
import { formatCacheRate, formatUsdAmount } from "@/lib/utils/billing";
import { cn } from "@/lib/utils";
import type { RequestLogFilterSummary, RequestLogModelUsageStat } from "@/types";
import {
  formatCompactTokenAmount,
  type TranslateFn,
} from "./page-helpers";

type ModelSortKey = "cost" | "tokens" | "requests" | "model";

const DEFAULT_VISIBLE_ROWS = 8;

function shareOf(value: number, total: number): number {
  if (!Number.isFinite(value) || !Number.isFinite(total) || total <= 0) {
    return 0;
  }
  return Math.min(1, Math.max(0, value / total));
}

function formatShare(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0%";
  const percent = value * 100;
  if (percent > 0 && percent < 0.1) return "<0.1%";
  if (percent < 10) return `${percent.toFixed(1)}%`;
  return `${Math.round(percent)}%`;
}

function sortModelStats(
  items: RequestLogModelUsageStat[],
  sortKey: ModelSortKey,
): RequestLogModelUsageStat[] {
  const next = [...items];
  next.sort((left, right) => {
    if (sortKey === "model") {
      return left.model.localeCompare(right.model);
    }
    if (sortKey === "requests") {
      if (right.requestCount !== left.requestCount) {
        return right.requestCount - left.requestCount;
      }
    } else if (sortKey === "tokens") {
      if (right.totalTokens !== left.totalTokens) {
        return right.totalTokens - left.totalTokens;
      }
    } else if (right.estimatedCostUsd !== left.estimatedCostUsd) {
      return right.estimatedCostUsd - left.estimatedCostUsd;
    }
    if (right.totalTokens !== left.totalTokens) {
      return right.totalTokens - left.totalTokens;
    }
    if (right.requestCount !== left.requestCount) {
      return right.requestCount - left.requestCount;
    }
    return left.model.localeCompare(right.model);
  });
  return next;
}

export function ModelUsageStatsCard({
  t,
  summary,
  isLoading,
  onModelClick,
}: {
  t: TranslateFn;
  summary: RequestLogFilterSummary;
  isLoading: boolean;
  onModelClick?: (model: string) => void;
}) {
  const [expanded, setExpanded] = useState(true);
  const [showAll, setShowAll] = useState(false);
  const [sortKey, setSortKey] = useState<ModelSortKey>("cost");

  const modelStats = summary.modelStats || [];
  const sorted = useMemo(
    () => sortModelStats(modelStats, sortKey),
    [modelStats, sortKey],
  );
  const visible = showAll ? sorted : sorted.slice(0, DEFAULT_VISIBLE_ROWS);
  const hasMore = sorted.length > DEFAULT_VISIBLE_ROWS;

  const totalCost = modelStats.reduce(
    (sum, item) => sum + Math.max(0, item.estimatedCostUsd || 0),
    0,
  );
  const totalTokens = modelStats.reduce(
    (sum, item) => sum + Math.max(0, item.totalTokens || 0),
    0,
  );
  const useTokenShare = totalCost <= 0;

  return (
    <Card className="glass-card overflow-hidden gap-0 py-0 shadow-sm">
      <CardHeader className="flex min-h-1 items-center border-b border-border/40 bg-[var(--table-section-bg)] py-3">
        <div className="flex w-full flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
          <div className="min-w-0">
            <CardTitle className="flex items-center gap-2 text-[15px] font-semibold">
              <BarChart3 className="size-4 text-primary" />
              {t("模型使用统计")}
            </CardTitle>
            <p className="mt-1 text-xs text-muted-foreground">
              {t("跟随当前筛选")} · {sorted.length} {t("个模型")}
              {summary.modelStatsTruncated
                ? ` · ${t("仅展示费用最高的 50 个模型")}`
                : ""}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <div className="flex items-center gap-1 rounded-xl border border-border/50 bg-background/40 p-1">
              {(
                [
                  ["cost", t("费用")],
                  ["tokens", "Token"],
                  ["requests", t("请求")],
                ] as const
              ).map(([key, label]) => (
                <Button
                  key={key}
                  type="button"
                  size="sm"
                  variant={sortKey === key ? "secondary" : "ghost"}
                  className="h-7 rounded-lg px-2.5 text-xs"
                  onClick={() => setSortKey(key)}
                >
                  {label}
                </Button>
              ))}
            </div>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-8 gap-1 rounded-xl px-2.5 text-xs"
              onClick={() => setExpanded((value) => !value)}
            >
              {expanded ? t("收起") : t("展开")}
              {expanded ? (
                <ChevronUp className="size-3.5" />
              ) : (
                <ChevronDown className="size-3.5" />
              )}
            </Button>
          </div>
        </div>
      </CardHeader>

      {expanded ? (
        <CardContent className="px-0">
          {isLoading ? (
            <div className="space-y-2 p-4">
              <Skeleton className="h-8 w-full" />
              <Skeleton className="h-8 w-full" />
              <Skeleton className="h-8 w-full" />
            </div>
          ) : sorted.length === 0 ? (
            <div className="px-4 py-8 text-center text-sm text-muted-foreground">
              {t("当前筛选下暂无模型用量")}
            </div>
          ) : (
            <>
              <Table className="min-w-[920px] table-fixed">
                <TableHeader>
                  <TableRow>
                    <TableHead className="h-11 w-[220px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                      {t("模型")}
                    </TableHead>
                    <TableHead className="h-11 w-[80px] px-3 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                      {t("请求")}
                    </TableHead>
                    <TableHead className="h-11 w-[80px] px-3 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                      {t("成功")}
                    </TableHead>
                    <TableHead className="h-11 w-[80px] px-3 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                      {t("异常")}
                    </TableHead>
                    <TableHead className="h-11 w-[100px] px-3 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                      Token
                    </TableHead>
                    <TableHead className="h-11 w-[90px] px-3 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                      {t("费用")}
                    </TableHead>
                    <TableHead className="h-11 w-[160px] px-3 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                      {t("费用占比")}
                    </TableHead>
                    <TableHead className="h-11 w-[80px] px-3 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                      {t("缓存率")}
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {visible.map((item) => {
                    const share = useTokenShare
                      ? shareOf(item.totalTokens, totalTokens)
                      : shareOf(item.estimatedCostUsd, totalCost);
                    const clickable =
                      Boolean(onModelClick) && item.model !== "(unknown)";
                    return (
                      <TableRow
                        key={item.model}
                        className={cn(clickable && "cursor-pointer hover:bg-muted/30")}
                        onClick={() => {
                          if (clickable && onModelClick) {
                            onModelClick(item.model);
                          }
                        }}
                      >
                        <TableCell className="px-4 py-3">
                          <div
                            className="break-all font-mono text-[11px] font-medium"
                            title={item.model}
                          >
                            {item.model}
                          </div>
                        </TableCell>
                        <TableCell className="px-3 py-3 text-sm font-semibold tabular-nums">
                          {item.requestCount}
                        </TableCell>
                        <TableCell className="px-3 py-3 text-sm tabular-nums text-green-600 dark:text-green-400">
                          {item.successCount}
                        </TableCell>
                        <TableCell className="px-3 py-3 text-sm tabular-nums text-red-500">
                          {item.errorCount}
                        </TableCell>
                        <TableCell className="px-3 py-3 text-sm font-medium tabular-nums">
                          {formatCompactTokenAmount(item.totalTokens)}
                        </TableCell>
                        <TableCell className="px-3 py-3 text-sm font-semibold tabular-nums">
                          {formatUsdAmount(item.estimatedCostUsd)}
                        </TableCell>
                        <TableCell className="px-3 py-3">
                          <div className="flex min-w-0 items-center gap-2">
                            <div className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-muted/60">
                              <div
                                className="h-full rounded-full bg-primary/75"
                                style={{ width: `${Math.max(share * 100, share > 0 ? 2 : 0)}%` }}
                              />
                            </div>
                            <span className="w-12 shrink-0 text-right text-xs tabular-nums text-muted-foreground">
                              {formatShare(share)}
                            </span>
                          </div>
                        </TableCell>
                        <TableCell className="px-3 py-3 text-sm tabular-nums text-muted-foreground">
                          {formatCacheRate(item.inputTokens, item.cachedInputTokens)}
                        </TableCell>
                      </TableRow>
                    );
                  })}
                </TableBody>
              </Table>
              <div className="flex flex-col gap-2 border-t border-border/40 px-4 py-3 text-xs text-muted-foreground sm:flex-row sm:items-center sm:justify-between">
                <div>{t("不含 Guard 重试用量")}</div>
                {hasMore ? (
                  <Button
                    type="button"
                    variant="link"
                    className="h-auto p-0 text-xs"
                    onClick={() => setShowAll((value) => !value)}
                  >
                    {showAll
                      ? t("收起")
                      : `${t("展开全部")} (${sorted.length})`}
                  </Button>
                ) : null}
              </div>
            </>
          )}
        </CardContent>
      ) : null}
    </Card>
  );
}
