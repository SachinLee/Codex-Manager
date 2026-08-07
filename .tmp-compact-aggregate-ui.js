const fs = require("fs");
const path = "apps/src/app/aggregate-api/page.tsx";
let src = fs.readFileSync(path, "utf8");
const nl = src.includes("\r\n") ? "\r\n" : "\n";

function replaceOnce(label, from, to) {
  if (!src.includes(from)) {
    console.log("MISS", label);
    console.log("preview", JSON.stringify(from.slice(0, 120)));
    return false;
  }
  src = src.replace(from, to);
  console.log("OK", label);
  return true;
}

// helper for model routes tooltip
if (!src.includes("function buildModelRoutesTooltip")) {
  replaceOnce(
    "helper",
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

function buildModelRoutesTooltip(
  modelSlugs: string[],
  t: (key: string) => string,
): string {
  if (modelSlugs.length === 0) return t("未配置模型路由");
  return [
    \`\${t("模型路由")} · \${modelSlugs.length}\`,
    ...modelSlugs,
  ].join("\\n");
}`
  );
}

// denser metric grids
replaceOnce(
  "metric-grid-1",
  `<section className="grid grid-cols-2 gap-2 lg:grid-cols-5">`,
  `<section className="grid grid-cols-2 gap-1.5 sm:grid-cols-3 lg:grid-cols-5">`
);
replaceOnce(
  "metric-grid-2",
  `<section className="grid grid-cols-1 gap-2 sm:grid-cols-3">`,
  `<section className="grid grid-cols-1 gap-1.5 sm:grid-cols-3">`
);

// compact card header
replaceOnce(
  "card-header",
  `<CardHeader className="border-b border-border/50 px-4 py-3">
            <div className="flex items-center justify-between gap-3">
              <div>
                <CardTitle>{t("上游连接")}</CardTitle>
                <p className="mt-1 text-xs text-muted-foreground">
                  {t("连通性测试只使用已配置路由对应的模型。")}
                </p>
              </div>`,
  `<CardHeader className="border-b border-border/50 px-3 py-2">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <CardTitle className="text-base">{t("上游连接")}</CardTitle>
                <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
                  {t("连通性测试只使用已配置路由对应的模型。")}
                </p>
              </div>`
);

// supplier cell compact + tooltip
replaceOnce(
  "supplier-cell",
  `<TableCell className="min-w-[240px]">
                            <div className="font-medium">{api.supplierName || api.id}</div>
                            <div className="max-w-[360px] truncate font-mono text-[11px] text-muted-foreground">{api.url}</div>
                            <div className="mt-1 text-[10px] text-muted-foreground">
                              {t("创建时间")}: {formatTsFromSeconds(api.createdAt, "-")}
                            </div>
                          </TableCell>`,
  `<TableCell className="min-w-[180px] py-2">
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
                          </TableCell>`
);

// model routes compact with tooltip
replaceOnce(
  "model-routes",
  `<TableCell className="max-w-[240px]">
                            {api.modelSlugs.length > 0 ? (
                              <div className="flex flex-wrap gap-1">
                                {api.modelSlugs.slice(0, 3).map((slug) => <Badge key={slug} variant="outline">{slug}</Badge>)}
                                {api.modelSlugs.length > 3 ? <Badge variant="secondary">+{api.modelSlugs.length - 3}</Badge> : null}
                              </div>
                            ) : (
                              <Badge variant="destructive">missing route</Badge>
                            )}
                          </TableCell>`,
  `<TableCell className="min-w-[140px] max-w-[180px] py-2">
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
                                missing route
                              </Badge>
                            )}
                          </TableCell>`
);

// connectivity compact row
replaceOnce(
  "connectivity",
  `<TableCell>
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
                          </TableCell>`,
  `<TableCell className="py-2">
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
                                onClick={() => testMutation.mutate(api.id)}
                              >
                                {testingApiId === api.id ? t("测试中...") : t("测试")}
                              </Button>
                            </div>
                          </TableCell>`
);

// tighter runtime / usage / balance cells
replaceOnce(
  "usage-cell-pad",
  `<TableCell className="min-w-[150px]">`,
  `<TableCell className="min-w-[130px] py-2">`
);
replaceOnce(
  "runtime-cell-pad",
  `<TableCell className="align-middle min-w-[160px]">`,
  `<TableCell className="min-w-[120px] py-2 align-middle">`
);

// page workspace gap if present
replaceOnce(
  "page-workspace",
  `<PageWorkspace>`,
  `<PageWorkspace className="gap-3">`
);

// table head cells slightly smaller via class on Table
replaceOnce(
  "table-open",
  `<Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t("供应商")}</TableHead>`,
  `<Table className="text-xs">
                <TableHeader>
                  <TableRow className="hover:bg-transparent">
                    <TableHead className="h-9">{t("供应商")}</TableHead>`
);

// remaining heads compact - only if previous worked, do bulk simple replacements carefully
[
  [`<TableHead>{t("类型")}</TableHead>`, `<TableHead className="h-9">{t("类型")}</TableHead>`],
  [`<TableHead>{t("密钥")}</TableHead>`, `<TableHead className="h-9">{t("密钥")}</TableHead>`],
  [`<TableHead>{t("模型路由")}</TableHead>`, `<TableHead className="h-9">{t("模型路由")}</TableHead>`],
  [`<TableHead>{t("余额")}</TableHead>`, `<TableHead className="h-9">{t("余额")}</TableHead>`],
  [`<TableHead>{t("今日用量")}</TableHead>`, `<TableHead className="h-9">{t("今日用量")}</TableHead>`],
  [`<TableHead>{t("运行状态")}</TableHead>`, `<TableHead className="h-9">{t("运行状态")}</TableHead>`],
  [`<TableHead>{t("连通性")}</TableHead>`, `<TableHead className="h-9">{t("连通性")}</TableHead>`],
  [`<TableHead>{t("启用")}</TableHead>`, `<TableHead className="h-9">{t("启用")}</TableHead>`],
  [`<TableHead className="text-right">{t("操作")}</TableHead>`, `<TableHead className="h-9 text-right">{t("操作")}</TableHead>`],
].forEach(([from, to], i) => replaceOnce(`head-${i}`, from, to));

// compact capability card spacing
replaceOnce(
  "capability-header",
  `<CardHeader className="flex-row items-center justify-between gap-3 py-4">`,
  `<CardHeader className="flex-row items-center justify-between gap-3 py-3">`
);

fs.writeFileSync(path, src, "utf8");
console.log("done", {
  hasModelTooltip: src.includes("buildModelRoutesTooltip") || src.includes("模型路由")} · {api.modelSlugs.length}`),
  hasCompactRoutes: src.includes("+{api.modelSlugs.length - 1}"),
  hasSupplierTooltip: src.includes("break-all font-mono text-[11px]"),
});
