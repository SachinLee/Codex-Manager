const fs = require("fs");
const path = "apps/src/app/aggregate-api/page.tsx";
let src = fs.readFileSync(path, "utf8");
const hasCrlf = src.includes("\r\n");
const n = (s) => s.replace(/\r\n/g, "\n");
const den = (s) => (hasCrlf ? s.replace(/\n/g, "\r\n") : s);
let w = n(src);

function replaceOnce(label, from, to) {
  if (!w.includes(from)) {
    console.log("MISS", label);
    return false;
  }
  w = w.replace(from, to);
  console.log("OK", label);
  return true;
}

replaceOnce(
  "import-type",
  `import type {
  AggregateApi,
  AggregateApiCapabilityDiagnosticsResult,
  AggregateApiBalanceSnapshot,
  AggregateApiDailyUsageStat,
  AggregateApiSecretResult,
} from "@/types/api-key";`,
  `import type {
  AggregateApi,
  AggregateApiCapabilityDiagnosticsResult,
  AggregateApiBalanceSnapshot,
  AggregateApiDailyUsageStat,
  AggregateApiSecretResult,
} from "@/types/api-key";
import type { ModelDailyUsageStat } from "@/types/request-log";`
);

replaceOnce(
  "tooltip-helper",
  `function buildDailyUsageTooltip(
  usage: AggregateApiDailyUsageStat,
  t: (key: string) => string,
): string {
  return [
    \`\${t("请求")} \${usage.requestCount}\`,
    \`\${t("输入")} \${formatMillionTokenAmount(usage.inputTokens)} / \${t("缓存")} \${formatMillionTokenAmount(usage.cachedInputTokens)} / \${t("缓存写入")} \${formatMillionTokenAmount(usage.cacheWriteInputTokens)} / \${t("计费输入")} \${formatMillionTokenAmount(usage.billableInputTokens)}\`,
    \`\${t("输出")} \${formatMillionTokenAmount(usage.outputTokens)} / \${t("推理输出")} \${formatMillionTokenAmount(usage.reasoningOutputTokens)}\`,
    \`\${t("Guard 重试")} \${formatMillionTokenAmount(usage.guardRetryTotalTokens)} tok / \${formatUsdAmount(usage.guardRetryEstimatedCostUsd)}\`,
    \`\${t("计费合计")} \${formatMillionTokenAmount(usage.billableTotalTokens)} tok / \${formatUsdAmount(usage.billableEstimatedCostUsd)}\`,
    t("含 Guard 重试"),
  ].join("\\n");
}`,
  `function buildDailyUsageTooltip(
  usage: AggregateApiDailyUsageStat,
  t: (key: string) => string,
): string {
  return [
    \`\${t("请求")} \${usage.requestCount}\`,
    \`\${t("输入")} \${formatMillionTokenAmount(usage.inputTokens)} / \${t("缓存")} \${formatMillionTokenAmount(usage.cachedInputTokens)} / \${t("缓存写入")} \${formatMillionTokenAmount(usage.cacheWriteInputTokens)} / \${t("计费输入")} \${formatMillionTokenAmount(usage.billableInputTokens)}\`,
    \`\${t("输出")} \${formatMillionTokenAmount(usage.outputTokens)} / \${t("推理输出")} \${formatMillionTokenAmount(usage.reasoningOutputTokens)}\`,
    \`\${t("Guard 重试")} \${formatMillionTokenAmount(usage.guardRetryTotalTokens)} tok / \${formatUsdAmount(usage.guardRetryEstimatedCostUsd)}\`,
    \`\${t("计费合计")} \${formatMillionTokenAmount(usage.billableTotalTokens)} tok / \${formatUsdAmount(usage.billableEstimatedCostUsd)}\`,
    t("含 Guard 重试"),
  ].join("\\n");
}

function buildModelDailyUsageTooltip(
  usage: ModelDailyUsageStat,
  t: (key: string) => string,
): string {
  return [
    \`\${t("请求")} \${usage.requestCount}\`,
    \`\${t("输入")} \${formatMillionTokenAmount(usage.inputTokens)} / \${t("缓存")} \${formatMillionTokenAmount(usage.cachedInputTokens)} / \${t("缓存写入")} \${formatMillionTokenAmount(usage.cacheWriteInputTokens)} / \${t("计费输入")} \${formatMillionTokenAmount(usage.billableInputTokens)}\`,
    \`\${t("输出")} \${formatMillionTokenAmount(usage.outputTokens)} / \${t("推理输出")} \${formatMillionTokenAmount(usage.reasoningOutputTokens)}\`,
    \`\${t("合计")} \${formatMillionTokenAmount(usage.totalTokens)} tok / \${formatUsdAmount(usage.estimatedCostUsd)}\`,
    \`\${t("缓存率")} \${formatCacheRateValue(usage.cacheHitRate)}\`,
  ].join("\\n");
}`
);

replaceOnce(
  "daily-query",
  `  const dailyUsageQuery = useQuery({
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
    staleTime: 10_000,
    refetchInterval: isPageActive ? 30_000 : false,
    refetchIntervalInBackground: false,
  });`,
  `  const dailyUsageQuery = useQuery({
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
    staleTime: 10_000,
    refetchInterval: isPageActive ? 30_000 : false,
    refetchIntervalInBackground: false,
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
    staleTime: 10_000,
    refetchInterval: isPageActive ? 30_000 : false,
    refetchIntervalInBackground: false,
  });`
);

replaceOnce(
  "model-section",
  `        </section>
        <Card className="glass-card overflow-hidden py-0">
          <CardHeader className="border-b border-border/50 px-3 py-2">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <CardTitle className="text-base">{t("上游连接")}</CardTitle>`,
  `        </section>

        <Card className="glass-card overflow-hidden py-0">
          <CardHeader className="border-b border-border/50 px-3 py-2">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <CardTitle className="text-base">{t("今日模型用量")}</CardTitle>
                <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
                  {t("按模型汇总当天 Token、费用与缓存率。")}
                </p>
              </div>
              <span className="shrink-0 text-[11px] text-muted-foreground">
                {modelDailyUsageQuery.isLoading
                  ? "..."
                  : `${(modelDailyUsageQuery.data || []).length} ${t("个模型")}`}
              </span>
            </div>
          </CardHeader>
          <CardContent className="p-0">
            <div className="max-h-[180px] overflow-auto">
              <Table className="text-xs">
                <TableHeader>
                  <TableRow className="hover:bg-transparent">
                    <TableHead className="h-8 sticky top-0 bg-card">{t("模型")}</TableHead>
                    <TableHead className="h-8 sticky top-0 bg-card">{t("请求")}</TableHead>
                    <TableHead className="h-8 sticky top-0 bg-card">{t("Token")}</TableHead>
                    <TableHead className="h-8 sticky top-0 bg-card">{t("费用")}</TableHead>
                    <TableHead className="h-8 sticky top-0 bg-card">{t("缓存率")}</TableHead>
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
        </Card>

        <Card className="glass-card overflow-hidden py-0">
          <CardHeader className="border-b border-border/50 px-3 py-2">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <CardTitle className="text-base">{t("上游连接")}</CardTitle>`
);

fs.writeFileSync(path, den(w));
console.log({
  hasModelType: w.includes("ModelDailyUsageStat"),
  hasQuery: w.includes("listModelDailyUsageStats"),
  hasSection: w.includes("今日模型用量"),
});
