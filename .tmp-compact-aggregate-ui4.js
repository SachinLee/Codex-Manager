const fs = require("fs");
const path = "apps/src/app/aggregate-api/page.tsx";
let src = fs.readFileSync(path, "utf8");
const hasCrlf = src.includes("\r\n");
const n = (s) => s.replace(/\r\n/g, "\n");
const den = (s) => (hasCrlf ? s.replace(/\n/g, "\r\n") : s);
let work = n(src);
function replaceOnce(label, from, to) {
  from = n(from);
  to = n(to);
  if (!work.includes(from)) {
    console.log("MISS", label);
    return false;
  }
  work = work.replace(from, to);
  console.log("OK", label);
  return true;
}

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
                                {t("未配置模型路由")}
                              </Badge>
                            )}
                          </TableCell>`
);

replaceOnce(
  "type-cell",
  `<TableCell>
                            <Badge variant="secondary">
                              {api.providerType === "compatible"
                                ? t("通用兼容（Codex + Claude）")
                                : PROVIDER_LABELS[api.providerType] || api.providerType}
                            </Badge>
                          </TableCell>`,
  `<TableCell className="py-2">
                            <Badge variant="secondary" className="h-5 text-[10px]">
                              {api.providerType === "compatible"
                                ? t("通用兼容（Codex + Claude）")
                                : PROVIDER_LABELS[api.providerType] || api.providerType}
                            </Badge>
                          </TableCell>`
);

replaceOnce(
  "runtime-normal",
  `<div className="flex flex-col items-start gap-1">
                                <Badge className="w-fit border-emerald-500/20 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300">
                                  {t("正常")}
                                </Badge>
                                {runtimeStatus && runtimeStatus.consecutiveFailures > 0 ? (
                                  <span className="text-[10px] text-muted-foreground">
                                    {t("连续失败")} {runtimeStatus.consecutiveFailures}/
                                    {runtimeStatus.failureThreshold}
                                  </span>
                                ) : null}
                              </div>`,
  `<div className="flex flex-col items-start gap-0.5">
                                <Badge className="h-5 w-fit border-emerald-500/20 bg-emerald-500/10 text-[10px] text-emerald-600 dark:text-emerald-300">
                                  {t("正常")}
                                </Badge>
                                {runtimeStatus && runtimeStatus.consecutiveFailures > 0 ? (
                                  <span className="text-[10px] text-muted-foreground">
                                    {t("连续失败")} {runtimeStatus.consecutiveFailures}/
                                    {runtimeStatus.failureThreshold}
                                  </span>
                                ) : null}
                              </div>`
);

replaceOnce(
  "runtime-cooling",
  `<Badge className="w-fit border-amber-500/20 bg-amber-500/10 text-amber-600 dark:text-amber-300">
                                    {t("冷却中")}{" "}
                                    {formatCooldownRemaining(
                                      runtimeStatus.cooldownUntil,
                                      runtimeStatusNowSeconds,
                                    )}
                                  </Badge>
                                  <span className="text-[10px] text-muted-foreground">
                                    {t("连续失败")} {runtimeStatus.consecutiveFailures}/
                                    {runtimeStatus.failureThreshold}
                                  </span>
                                  <Button
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className="h-6 w-fit gap-1 px-1 text-[10px] text-amber-700 hover:text-amber-800 dark:text-amber-300 dark:hover:text-amber-200"
                                    disabled={!isServiceReady || resetCooldownMutation.isPending}
                                    onClick={() => setResetCooldownApi(api)}
                                  >
                                    <RotateCcw className="h-3 w-3" />
                                    {t("解除冷却")}
                                  </Button>`,
  `<Badge className="h-5 w-fit border-amber-500/20 bg-amber-500/10 text-[10px] text-amber-600 dark:text-amber-300">
                                    {t("冷却中")}{" "}
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
                                  </div>`
);

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

fs.writeFileSync(path, den(work));
console.log({
  hasCompactRoutes: work.includes("+{api.modelSlugs.length - 1}"),
  hasSupplierTooltip: work.includes("break-all font-mono text-[11px]"),
  hasMissingRouteI18n: work.includes('t("未配置模型路由")'),
  hasConnectivityRow: work.includes("flex items-center gap-1.5"),
});
