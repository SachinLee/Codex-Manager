from pathlib import Path

# ---- page-sections.tsx ----
path = Path(r'apps/src/app/logs/page-sections.tsx')
text = path.read_text(encoding='utf-8')

# imports
old_imp = '''import { formatUsdAmount } from "@/lib/utils/billing";
import {
  AccountKeyInfoCell,
  ErrorInfoCell,
  ModelEffortCell,
  RequestRouteInfoCell,
} from "./page-cells";
import {
  formatCompactTokenAmount,
  formatDuration,
  formatTableTokenAmount,
  getStatusBadge,
  searchFieldPlaceholder,
  type SearchField,
  type StatusFilter,
  type TimeRangePreset,
  type TranslateFn,
  resolveAccountDisplayName,
  resolveDisplayedStatusCode,
  SummaryCard,
} from "./page-helpers";
import type { AggregateApi, ApiKey, RequestLog, RequestLogFilterSummary } from "@/types";'''

new_imp = '''import { formatCacheRate, formatUsdAmount } from "@/lib/utils/billing";
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
import type { AggregateApi, ApiKey, RequestLog, RequestLogFilterSummary } from "@/types";'''

if old_imp not in text:
    raise SystemExit('imports block not found')
text = text.replace(old_imp, new_imp, 1)

# props destructure - add codexSessionMap after aggregateApiMap
old_props_use = '''  aggregateApiMap,
  clearMutationPending,'''
new_props_use = '''  aggregateApiMap,
  codexSessionMap,
  clearMutationPending,'''
if old_props_use not in text:
    raise SystemExit('props use block not found')
text = text.replace(old_props_use, new_props_use, 1)

old_props_type = '''  aggregateApiMap: Map<string, AggregateApi>;
  clearMutationPending: boolean;'''
new_props_type = '''  aggregateApiMap: Map<string, AggregateApi>;
  codexSessionMap: Map<string, CodexSession>;
  clearMutationPending: boolean;'''
if old_props_type not in text:
    raise SystemExit('props type block not found')
text = text.replace(old_props_type, new_props_type, 1)

# Insert ModelUsageStatsCard before request detail card
marker = '''      <Card className="glass-card mission-panel overflow-hidden gap-0 py-0 shadow-sm">
        <CardHeader className="flex min-h-1 items-center border-b border-border/40 bg-[var(--table-section-bg)] py-3">
          <div className="flex w-full flex-col gap-2 lg:flex-row lg:items-center lg:justify-between">
            <div className="min-w-0">
              <CardTitle className="text-[15px] font-semibold">
                {t("请求明细")}'''
if marker not in text:
    raise SystemExit('detail card marker not found')
model_usage = '''      <ModelUsageStatsCard
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

'''
text = text.replace(marker, model_usage + marker, 1)

# Replace table header/body for additive columns
old_header = '''              <TableHeader>
                <TableRow>
                  <TableHead className="h-12 w-[150px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                    {t("时间")}
                  </TableHead>
                  <TableHead className="w-[224px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                    {t("账号 / 密钥")}
                  </TableHead>
                  <TableHead className="w-[220px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                    {t("模型 / 推理 / 等级")}
                  </TableHead>
                  <TableHead className="w-[92px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                    {t("状态")}
                  </TableHead>
                  <TableHead className="w-[128px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                    {t("用时 / 首响")}
                  </TableHead>
                  <TableHead className="w-[148px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                    {t("Token")}
                  </TableHead>
                  <TableHead className="w-[240px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                    {t("类型 / 方法 / 路径")}
                  </TableHead>
                  <TableHead className="w-[240px] px-4 text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
                    {t("错误")}
                  </TableHead>
                </TableRow>
              </TableHeader>'''

new_header = '''              <TableHeader>
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
              </TableHeader>'''

if old_header not in text:
    raise SystemExit('table header not found')
text = text.replace(old_header, new_header, 1)
text = text.replace(
    '<Table className="min-w-[1500px] table-fixed">',
    '<Table className="min-w-[1934px] table-fixed">',
    1,
)

# skeleton rows - expand to 11 cells
old_skel = '''                {isLogsLoading ? (
                  Array.from({ length: 10 }).map((_, index) => (
                    <TableRow key={index}>
                      <TableCell><Skeleton className="h-4 w-32" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-40" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-24" /></TableCell>
                      <TableCell><Skeleton className="h-6 w-12 rounded-full" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-12" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-12" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-16" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-28" /></TableCell>
                    </TableRow>
                  ))
                ) : logs.length === 0 ? ('''

# read actual skel from file around loading
# flexible: find and replace data row instead

old_row_start = '''                  logs.map((log) => (
                    <TableRow key={log.id} className="group text-xs hover:bg-muted/20">
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
                        {getStatusBadge(resolveDisplayedStatusCode(log))}
                      </TableCell>
                      <TableCell className="px-4 py-3 align-top font-mono">
                        <span
                          className="text-xs text-primary"
                          title={t("首响表示从请求开始到首个上游响应片段的耗时")}
                        >
                          {formatDuration(log.durationMs)}/{formatDuration(log.firstResponseMs)}
                        </span>
                      </TableCell>
                      <TableCell className="px-4 py-3 align-top">
                        <div className="flex flex-col gap-0.5 text-[10px] text-muted-foreground">
                          <span>{t("总")} {formatTableTokenAmount(log.totalTokens)}</span>
                          <span>{t("输入")} {formatTableTokenAmount(log.inputTokens)}</span>
                          <span className="opacity-60">
                            {t("缓存")} {formatTableTokenAmount(log.cachedInputTokens)}
                          </span>
                        </div>
                      </TableCell>
                      <TableCell className="px-4 py-3 align-top">
                        <RequestRouteInfoCell log={log} />
                      </TableCell>
                      <TableCell className="px-4 py-3 text-left align-top">
                        <ErrorInfoCell log={log} />
                      </TableCell>
                    </TableRow>
                  ))'''

new_row = '''                  logs.map((log) => (
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
                        <div className="flex flex-col gap-1">
                          {getStatusBadge(resolveDisplayedStatusCode(log))}
                          {isReasoningGuardConverted502(log) ? (
                            <span
                              className="w-fit rounded border border-amber-500/25 bg-amber-500/10 px-1.5 py-0.5 text-[10px] text-amber-700 dark:text-amber-300"
                              title={t("这是 Reasoning Guard 被网关保护转换成的 502，不是真实上游 502。")}
                            >
                              {formatReasoningGuardTarget(log)} -&gt; 502
                            </span>
                          ) : log.guardInternalRetryCount > 0 ? (
                            <span
                              className="w-fit rounded border border-sky-500/25 bg-sky-500/10 px-1.5 py-0.5 text-[10px] text-sky-700 dark:text-sky-300"
                              title={t("Guard 命中后已在网关内部重试并恢复。")}
                            >
                              Guard retry {log.guardInternalRetryCount}
                            </span>
                          ) : null}
                        </div>
                      </TableCell>
                      <TableCell className="px-4 py-3 align-top font-mono">
                        <div className="flex flex-col gap-0.5">
                          <span
                            className="text-xs text-primary"
                            title={t("首响表示从请求开始到首个上游响应片段的耗时")}
                          >
                            {formatDuration(log.durationMs)}/{formatDuration(log.firstResponseMs)}
                          </span>
                          <span className="text-[10px] text-muted-foreground">
                            {formatOutputRate(log, t)}
                          </span>
                        </div>
                      </TableCell>
                      <TableCell className="px-4 py-3 align-top">
                        <div className="flex flex-col gap-0.5 text-[10px] text-muted-foreground">
                          <span>{t("总")} {formatTableTokenAmount(log.totalTokens)}</span>
                          <span>{t("输入")} {formatTableTokenAmount(log.inputTokens)}</span>
                          <span className="opacity-60">
                            {t("缓存")} {formatTableTokenAmount(log.cachedInputTokens)}
                          </span>
                          {log.guardRetryTotalTokens > 0 ? (
                            <span className="text-amber-600 dark:text-amber-300">
                              Guard +{formatTableTokenAmount(log.guardRetryTotalTokens)}
                            </span>
                          ) : null}
                        </div>
                      </TableCell>
                      <TableCell className="px-4 py-3 align-top text-xs text-muted-foreground">
                        {formatCacheRate(log.cachedInputTokens, log.inputTokens)}
                      </TableCell>
                      <TableCell className="px-4 py-3 align-top">
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
                              {t("长上下文")}
                              {log.longContextUpliftUsd != null
                                ? ` +${formatUsdAmount(log.longContextUpliftUsd)}`
                                : ""}
                            </span>
                          ) : log.pricingContextBand === "single_tier" ? (
                            <span className="w-fit rounded border border-border/60 bg-muted/40 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                              {t("单档价格")}
                            </span>
                          ) : log.pricingContextBand === "legacy_candidate" ? (
                            <span className="w-fit rounded border border-orange-500/25 bg-orange-500/10 px-1.5 py-0.5 text-[10px] text-orange-600 dark:text-orange-300">
                              {t("兼容候选")}
                            </span>
                          ) : null}
                          {log.guardRetryEstimatedCostUsd > 0 ? (
                            <span className="text-[10px] text-amber-600 dark:text-amber-300">
                              Guard +{formatUsdAmount(log.guardRetryEstimatedCostUsd)}
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
                  ))'''

if old_row_start not in text:
    raise SystemExit('data row not found')
text = text.replace(old_row_start, new_row, 1)

# fix skeleton to have 11 columns - find current skeleton block
import re
skel_pat = re.compile(
    r'\{isLogsLoading \? \(\s*Array\.from\(\{ length: 10 \}\)\.map\(\(_, index\) => \(\s*<TableRow key=\{index\}>.*?</TableRow>\s*\)\)\s*\) : logs\.length === 0 \? \(',
    re.S,
)
new_skel = '''{isLogsLoading ? (
                  Array.from({ length: 10 }).map((_, index) => (
                    <TableRow key={index}>
                      <TableCell><Skeleton className="h-4 w-28" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-32" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-32" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-24" /></TableCell>
                      <TableCell><Skeleton className="h-6 w-12 rounded-full" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-12" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-12" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-10" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-16" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-28" /></TableCell>
                      <TableCell><Skeleton className="h-4 w-28" /></TableCell>
                    </TableRow>
                  ))
                ) : logs.length === 0 ? ('''
m = skel_pat.search(text)
if not m:
    raise SystemExit('skeleton block not found')
text = text[:m.start()] + new_skel + text[m.end():]

# empty row colspan
text = text.replace('colSpan={8}', 'colSpan={11}')
text = text.replace('colSpan={9}', 'colSpan={11}')

path.write_text(text, encoding='utf-8')
print('page-sections patched')

# ---- page.tsx ----
page = Path(r'apps/src/app/logs/page.tsx')
pt = page.read_text(encoding='utf-8')
if 'codexSessionMap' not in pt:
    # add import for CodexSession if needed
    if 'type CodexSession' not in pt and 'CodexSession' not in pt:
        pt = pt.replace(
            'import { codexLauncherClient } from "@/lib/api/codex-launcher";',
            'import {\n  codexLauncherClient,\n  type CodexSession,\n} from "@/lib/api/codex-launcher";',
        )
        if 'import {\n  codexLauncherClient,\n  type CodexSession,\n} from "@/lib/api/codex-launcher";' not in pt:
            # try alternate import style
            import_lines = [l for l in pt.splitlines() if 'codex-launcher' in l]
            print('codex import lines', import_lines)
            if not import_lines:
                # add near top after other imports
                pt = pt.replace(
                    'import { RequestLogsTabContent } from "./page-sections";',
                    'import type { CodexSession } from "@/lib/api/codex-launcher";\nimport { RequestLogsTabContent } from "./page-sections";',
                )

    insert_after = '''  const aggregateApiMap = useMemo(() => {
    return new Map(
      (aggregateApisResult || []).map((aggregateApi) => [
        aggregateApi.id,
        aggregateApi,
      ]),
    );
  }, [aggregateApisResult]);

  const logs = logsResult?.items || [];'''

    insert_new = '''  const aggregateApiMap = useMemo(() => {
    return new Map(
      (aggregateApisResult || []).map((aggregateApi) => [
        aggregateApi.id,
        aggregateApi,
      ]),
    );
  }, [aggregateApisResult]);

  const codexSessionMap = useMemo(() => {
    return new Map<string, CodexSession>(
      (codexSessions || [])
        .map((session) => {
          const id = String(session.sessionId || "").trim();
          return id ? ([id, session] as const) : null;
        })
        .filter((entry): entry is readonly [string, CodexSession] => entry != null),
    );
  }, [codexSessions]);

  const logs = logsResult?.items || [];'''

    if insert_after not in pt:
        raise SystemExit('aggregateApiMap block not found in page.tsx')
    pt = pt.replace(insert_after, insert_new, 1)

    prop_old = '''            aggregateApiMap={aggregateApiMap}
            clearMutationPending={clearMutation.isPending}'''
    prop_new = '''            aggregateApiMap={aggregateApiMap}
            codexSessionMap={codexSessionMap}
            clearMutationPending={clearMutation.isPending}'''
    if prop_old not in pt:
        raise SystemExit('RequestLogsTabContent props not found')
    pt = pt.replace(prop_old, prop_new, 1)

    # ensure modelStatsTruncated in fallback summary
    if 'modelStatsTruncated' not in pt:
        pt = pt.replace(
            '    modelStats: [],',
            '    modelStats: [],\n    modelStatsTruncated: false,',
        )

    page.write_text(pt, encoding='utf-8')
    print('page.tsx patched')
else:
    print('page.tsx already has codexSessionMap')
