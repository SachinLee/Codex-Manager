from pathlib import Path
path = Path(r'apps/src/app/aggregate-api/page.tsx')
text = path.read_text(encoding='utf-8')

# 1) lucide icons
if 'RotateCcw' not in text:
    text = text.replace(
        '''  RefreshCw,
  ShieldCheck,
  Trash2,
  Unplug,
} from "lucide-react";''',
        '''  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Trash2,
  Unplug,
} from "lucide-react";''',
        1,
    )

# 2) imports
if 'useAggregateApiRuntimeStatuses' not in text:
    text = text.replace(
        'import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";\n',
        'import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";\n'
        'import { useAggregateApiRuntimeStatuses } from "@/hooks/useAggregateApiRuntimeStatuses";\n',
        1,
    )
if 'getAppErrorMessage' not in text:
    text = text.replace(
        'import { accountClient } from "@/lib/api/account-client";\n',
        'import { accountClient } from "@/lib/api/account-client";\n'
        'import { getAppErrorMessage } from "@/lib/api/transport";\n',
        1,
    )

# 3) helper formatCooldownRemaining after formatBalance
if 'function formatCooldownRemaining' not in text:
    marker = 'function formatBalance(snapshot: AggregateApiBalanceSnapshot | null): string {'
    idx = text.find(marker)
    if idx < 0:
        raise SystemExit('formatBalance not found')
    # find end of formatBalance function - next function or export
    end = text.find('\nexport default function AggregateApiPage', idx)
    helper = '''
function formatCooldownRemaining(cooldownUntil: number | null | undefined, nowSeconds: number): string {
  const remaining = Math.max(0, Number(cooldownUntil || 0) - nowSeconds);
  const minutes = Math.floor(remaining / 60);
  const seconds = remaining % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

'''
    text = text[:end] + helper + text[end:]

# 4) state + hook after diagnosticsResult
old_state = '''  const [diagnosticsResult, setDiagnosticsResult] =
    useState<AggregateApiCapabilityDiagnosticsResult | null>(null);

  const { data: aggregateApis = [], isLoading } = useQuery({
'''
new_state = '''  const [diagnosticsResult, setDiagnosticsResult] =
    useState<AggregateApiCapabilityDiagnosticsResult | null>(null);
  const [resetCooldownApi, setResetCooldownApi] = useState<AggregateApi | null>(null);

  const {
    byApiId: aggregateApiRuntimeStatusById,
    nowSeconds: runtimeStatusNowSeconds,
  } = useAggregateApiRuntimeStatuses(isQueryEnabled);

  const { data: aggregateApis = [], isLoading } = useQuery({
'''
if old_state not in text:
    raise SystemExit('state block not found')
text = text.replace(old_state, new_state, 1)

# 5) clear resetCooldownApi when page inactive
text = text.replace(
    '''      setModalOpen(false);
      setEditingId(null);
      setDeleteId(null);
      setRevealedSecrets({});
''',
    '''      setModalOpen(false);
      setEditingId(null);
      setDeleteId(null);
      setResetCooldownApi(null);
      setRevealedSecrets({});
''',
    1,
)

# 6) add reset mutation after balanceMutation or diagnosticsMutation - find toggleMutation or balanceMutation
if 'resetCooldownMutation' not in text:
    # insert after diagnosticsMutation block ends - find balanceMutation
    bal = '''  const balanceMutation = useMutation({
    mutationFn: (apiId: string) => accountClient.refreshAggregateApiBalance(apiId),
'''
    if bal not in text:
        raise SystemExit('balanceMutation not found')
    insert = '''  const resetCooldownMutation = useMutation({
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

'''
    text = text.replace(bal, insert + bal, 1)

# 7) metrics: cooling count - find activeCount routedCount failedCount
# locate where these are defined
if 'coolingCount' not in text:
    # find failedCount line
    import re
    m = re.search(r'const failedCount = aggregateApis\.filter\(\(api\) => api\.lastTestStatus === "failed"\)\.length;', text)
    if not m:
        raise SystemExit('failedCount not found')
    insert_after = m.group(0)
    text = text.replace(
        insert_after,
        insert_after + '''
  const coolingCount = aggregateApis.filter((api) => {
    const runtime = aggregateApiRuntimeStatusById.get(api.id);
    return Boolean(
      runtime?.isCoolingDown && Number(runtime.cooldownUntil || 0) > runtimeStatusNowSeconds,
    );
  }).length;''',
        1,
    )
    # add metric card - change grid maybe to 5 or replace one
    text = text.replace(
        '''          <MetricCard title={t("测试失败")} value={failedCount} icon={Unplug} tone="rose" />
        </section>''',
        '''          <MetricCard title={t("测试失败")} value={failedCount} icon={Unplug} tone="rose" />
          <MetricCard title={t("冷却中")} value={coolingCount} icon={RotateCcw} tone="amber" />
        </section>''',
        1,
    )
    text = text.replace(
        'grid grid-cols-2 gap-2 lg:grid-cols-4',
        'grid grid-cols-2 gap-2 lg:grid-cols-5',
        1,
    )

# 8) table header add 运行状态 column
text = text.replace(
    '''                    <TableHead>{t("余额")}</TableHead>
                    <TableHead>{t("连通性")}</TableHead>
                    <TableHead>{t("启用")}</TableHead>''',
    '''                    <TableHead>{t("余额")}</TableHead>
                    <TableHead>{t("运行状态")}</TableHead>
                    <TableHead>{t("连通性")}</TableHead>
                    <TableHead>{t("启用")}</TableHead>''',
    1,
)
text = text.replace('colSpan={8}', 'colSpan={9}')

# 9) row: compute runtime + status cell
old_row_start = '''                    filteredApis.map((api) => {
                      const revealed = revealedSecrets[api.id];
                      const balance = parseBalanceSnapshot(api);
                      const testError = String(api.lastTestError || "").trim();
                      return (
'''
new_row_start = '''                    filteredApis.map((api) => {
                      const revealed = revealedSecrets[api.id];
                      const balance = parseBalanceSnapshot(api);
                      const testError = String(api.lastTestError || "").trim();
                      const runtimeStatus = aggregateApiRuntimeStatusById.get(api.id);
                      const isCoolingDown = Boolean(
                        runtimeStatus?.isCoolingDown &&
                          Number(runtimeStatus.cooldownUntil || 0) > runtimeStatusNowSeconds,
                      );
                      return (
'''
if old_row_start not in text:
    raise SystemExit('row start not found')
text = text.replace(old_row_start, new_row_start, 1)

old_balance_cell_end = '''                          </TableCell>
                          <TableCell>
                            <div className="space-y-1">
                              {api.lastTestStatus === "failed" && testError ? (
'''
new_status_and_conn = '''                          </TableCell>
                          <TableCell className="align-middle min-w-[160px]">
                            {isCoolingDown && runtimeStatus ? (
                              <Tooltip>
                                <TooltipTrigger
                                  render={<div />}
                                  className="flex flex-col items-start gap-1"
                                >
                                  <Badge className="w-fit border-amber-500/20 bg-amber-500/10 text-amber-600 dark:text-amber-300">
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
                                  </Button>
                                </TooltipTrigger>
                                <TooltipContent className="max-w-sm space-y-1 text-xs">
                                  <div className="grid gap-1">
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
                              <div className="flex flex-col items-start gap-1">
                                <Badge className="w-fit border-emerald-500/20 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300">
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
                          <TableCell>
                            <div className="space-y-1">
                              {api.lastTestStatus === "failed" && testError ? (
'''
if old_balance_cell_end not in text:
    raise SystemExit('balance/connectivity cell junction not found')
text = text.replace(old_balance_cell_end, new_status_and_conn, 1)

# 10) ConfirmDialog for reset cooldown
if 'resetCooldownApi' in text and '解除冷却' not in text[text.find('ConfirmDialog'):]:
    pass
old_delete_dialog = '''      <ConfirmDialog
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
'''
new_dialogs = '''      <ConfirmDialog
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
'''
if old_delete_dialog not in text:
    raise SystemExit('delete dialog block not found')
text = text.replace(old_delete_dialog, new_dialogs, 1)

# skeleton colspan if any
# check MetricCard tone amber exists - if not, use rose/violet
if 'tone="amber"' in text:
    # check PageWorkspace MetricCard allowed tones
    pass

path.write_text(text, encoding='utf-8')
print('aggregate-api runtime status UI restored')
print('has hook', 'useAggregateApiRuntimeStatuses' in text)
print('has cooling column', t('运行状态') if False else '运行状态' in text)
print('has reset dialog', 'resetCooldownApi' in text and '确认解除' in text)
