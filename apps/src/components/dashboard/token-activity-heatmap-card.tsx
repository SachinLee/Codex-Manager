"use client";

import { useEffect, useMemo, useRef } from "react";
import { AlertTriangle, BarChart3 } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatTokenAmountZh } from "@/lib/dashboard/format";
import {
  buildTokenActivityMonthLabels,
  computeHeatmapColumnCount,
  computeLeadingPlaceholders,
  TOKEN_ACTIVITY_WEEKDAY_ROWS,
} from "@/lib/dashboard/token-activity-grid";
import { useI18n } from "@/lib/i18n/provider";
import { cn } from "@/lib/utils";
import type { DashboardDailyUsagePoint, DashboardTokenActivity } from "@/types";

const CELL_CLASS = "size-3.5 rounded-[3px] border";
const GRID_GAP_CLASS = "gap-1";

const TOOLTIP_CONTENT_CLASS =
  "pointer-events-none max-w-[min(18rem,calc(100vw-1.5rem))] border border-border/70 bg-card/95 px-3 py-2.5 text-xs text-foreground shadow-lg backdrop-blur-sm [&>svg]:hidden";

export interface TokenActivityHeatmapCardProps {
  activity: DashboardTokenActivity | undefined;
  isLoading: boolean;
  isError: boolean;
}

function formatActivityDateRange(
  startTs: number | null | undefined,
  endTsExclusive: number | null | undefined,
): string {
  if (!startTs || !endTsExclusive || endTsExclusive <= startTs) {
    return "--";
  }
  const endTs = endTsExclusive - 1;
  const startDate = new Date(startTs * 1000);
  const endDate = new Date(endTs * 1000);
  if (Number.isNaN(startDate.getTime()) || Number.isNaN(endDate.getTime())) {
    return "--";
  }
  const formatter = new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  });
  return `${formatter.format(startDate)} – ${formatter.format(endDate)}`;
}

function formatDayLabel(value: number): string {
  const date = new Date(value * 1000);
  if (Number.isNaN(date.getTime())) {
    return "--";
  }
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(date);
}

function tokenActivityTone(totalTokens: number, maxTokens: number): string {
  if (totalTokens <= 0 || maxTokens <= 0) {
    return "border-border/40 bg-muted/45 hover:bg-muted/60";
  }
  const ratio = totalTokens / maxTokens;
  if (ratio >= 0.75) {
    return "border-blue-500/60 bg-blue-500 hover:bg-blue-500/90";
  }
  if (ratio >= 0.5) {
    return "border-sky-500/50 bg-sky-400 hover:bg-sky-400/90";
  }
  if (ratio >= 0.25) {
    return "border-sky-400/40 bg-sky-300/80 hover:bg-sky-300";
  }
  return "border-sky-300/40 bg-sky-200/70 hover:bg-sky-200 dark:bg-sky-900/60 dark:hover:bg-sky-900";
}

function TokenActivityLegend() {
  const { t } = useI18n();
  return (
    <div className="flex items-center gap-1 text-[11px] text-muted-foreground">
      <span>{t("低")}</span>
      {[0, 1, 2, 3, 4].map((level) => (
        <span
          key={level}
          className={cn(CELL_CLASS, tokenActivityTone(level === 0 ? 0 : level, 4))}
        />
      ))}
      <span>{t("高")}</span>
    </div>
  );
}

function DayHoverTooltipContent({ day }: { day: DashboardDailyUsagePoint }) {
  const { t } = useI18n();
  return (
    <div className="w-full min-w-[10rem]">
      <div className="font-medium">{formatDayLabel(day.dayStartTs)}</div>
      <div className="mt-2 space-y-1 text-muted-foreground">
        <div className="flex items-center justify-between gap-4">
          <span>Token</span>
          <span className="font-semibold text-foreground">
            {formatTokenAmountZh(day.usage.totalTokens)}
          </span>
        </div>
        <div className="flex items-center justify-between gap-4">
          <span>{t("次请求")}</span>
          <span className="font-semibold tabular-nums text-foreground">
            {day.usage.requestCount.toLocaleString("zh-CN")}
          </span>
        </div>
      </div>
    </div>
  );
}

function HeatmapDayCell({
  day,
  maxDailyTokens,
}: {
  day: DashboardDailyUsagePoint;
  maxDailyTokens: number;
}) {
  const tokenLabel = formatTokenAmountZh(day.usage.totalTokens);
  return (
    <Tooltip>
      <TooltipTrigger
        render={<button type="button" />}
        className={cn(
          CELL_CLASS,
          "transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1",
          tokenActivityTone(day.usage.totalTokens, maxDailyTokens),
        )}
        aria-label={`${formatDayLabel(day.dayStartTs)} ${tokenLabel} Token`}
      />
      <TooltipContent side="top" sideOffset={6} className={TOOLTIP_CONTENT_CLASS}>
        <DayHoverTooltipContent day={day} />
      </TooltipContent>
    </Tooltip>
  );
}

export function TokenActivityHeatmapCard({
  activity,
  isLoading,
  isError,
}: TokenActivityHeatmapCardProps) {
  const { t } = useI18n();
  const scrollRef = useRef<HTMLDivElement | null>(null);

  const days = activity?.days ?? [];
  const dayStartTsList = useMemo(
    () => days.map((item) => item.dayStartTs),
    [days],
  );
  const maxDailyTokens = useMemo(
    () => days.reduce((max, item) => Math.max(max, item.usage.totalTokens), 0),
    [days],
  );
  const leadingPlaceholders = useMemo(() => {
    if (days.length === 0) {
      return 0;
    }
    return computeLeadingPlaceholders(days[0].dayStartTs);
  }, [days]);
  const columnCount = useMemo(
    () => computeHeatmapColumnCount(days.length, leadingPlaceholders),
    [days.length, leadingPlaceholders],
  );
  const monthLabels = useMemo(
    () => buildTokenActivityMonthLabels(dayStartTsList, leadingPlaceholders, columnCount),
    [columnCount, dayStartTsList, leadingPlaceholders],
  );
  const monthLabelByColumn = useMemo(() => {
    const map = new Map<number, string>();
    for (const item of monthLabels) {
      map.set(item.columnIndex, item.label);
    }
    return map;
  }, [monthLabels]);

  useEffect(() => {
    const container = scrollRef.current;
    if (!container) {
      return;
    }
    container.scrollLeft = container.scrollWidth;
  }, [columnCount, days.length]);

  if (isLoading) {
    return <Skeleton className="h-72 w-full rounded-xl" />;
  }
  if (isError) {
    return (
      <Card className="glass-card shadow-sm">
        <CardContent>
          <Alert variant="destructive">
            <AlertTriangle />
            <AlertTitle>{t("Token 活动读取失败")}</AlertTitle>
            <AlertDescription>{t("请稍后重试或检查核心服务状态。")}</AlertDescription>
          </Alert>
        </CardContent>
      </Card>
    );
  }
  if (!activity || days.length === 0) {
    return (
      <Card className="glass-card shadow-sm">
        <CardContent>
          <Empty className="min-h-40 border bg-muted/20">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <BarChart3 />
              </EmptyMedia>
              <EmptyTitle>{t("暂无 Token 活动")}</EmptyTitle>
              <EmptyDescription>{t("有请求日志后会自动生成全年活动热力图。")}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className="glass-card overflow-hidden shadow-sm">
      <CardHeader className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <CardTitle className="flex items-center gap-2 text-base font-semibold">
            <BarChart3 className="h-4 w-4 text-primary" />
            {t("Token 活动")}
          </CardTitle>
          <p className="mt-1 text-xs text-muted-foreground">
            {formatActivityDateRange(activity.rangeStartTs, activity.rangeEndTs)}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-3 sm:justify-end">
          <div className="rounded-lg bg-primary/10 px-3 py-2">
            <div className="text-[11px] text-muted-foreground">{t("累计 Token")}</div>
            <div className="mt-0.5 text-lg font-semibold text-primary">
              {formatTokenAmountZh(activity.totalTokens)}
            </div>
          </div>
          <TokenActivityLegend />
        </div>
      </CardHeader>
      <CardContent>
        <div className="flex gap-2">
          <div
            className={cn(
              "grid shrink-0 text-[10px] leading-none text-muted-foreground",
              GRID_GAP_CLASS,
            )}
            style={{ gridTemplateRows: `repeat(${TOKEN_ACTIVITY_WEEKDAY_ROWS}, minmax(0, 1fr))` }}
          >
            {Array.from({ length: TOKEN_ACTIVITY_WEEKDAY_ROWS }).map((_, rowIndex) => (
              <div
                key={`weekday-${rowIndex}`}
                className="flex size-3.5 items-center justify-end pr-0.5"
              >
                {rowIndex === 1 ? "一" : rowIndex === 3 ? "三" : rowIndex === 5 ? "五" : ""}
              </div>
            ))}
          </div>
          <div ref={scrollRef} className="min-w-0 flex-1 overflow-x-auto pb-1">
            <div className="w-max">
              <div
                className={cn("grid w-max grid-flow-col", GRID_GAP_CLASS)}
                style={{ gridTemplateRows: `repeat(${TOKEN_ACTIVITY_WEEKDAY_ROWS}, minmax(0, 1fr))` }}
              >
                {Array.from({ length: leadingPlaceholders }).map((_, index) => (
                  <div key={`placeholder-${index}`} className="size-3.5" aria-hidden />
                ))}
                {days.map((day) => (
                  <HeatmapDayCell key={day.dayStartTs} day={day} maxDailyTokens={maxDailyTokens} />
                ))}
              </div>
              <div
                className={cn("mt-1.5 grid w-max grid-flow-col", GRID_GAP_CLASS)}
                style={{ gridTemplateRows: "repeat(1, minmax(0, 1fr))" }}
              >
                {Array.from({ length: columnCount }).map((_, columnIndex) => (
                  <div
                    key={`month-${columnIndex}`}
                    className="relative h-4 w-3.5 text-[10px] leading-none text-muted-foreground"
                  >
                    {monthLabelByColumn.get(columnIndex) ? (
                      <span className="absolute left-0 top-0 whitespace-nowrap">
                        {monthLabelByColumn.get(columnIndex)}
                      </span>
                    ) : null}
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
        <p className="mt-2 text-[11px] text-muted-foreground">{t("悬停格子查看当日 Token")}</p>
      </CardContent>
    </Card>
  );
}
