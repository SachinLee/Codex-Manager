import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const appsRoot = path.resolve(import.meta.dirname, "..");
const pageSectionsPath = path.join(
  appsRoot,
  "src",
  "app",
  "logs",
  "page-sections.tsx",
);
const logsPagePath = path.join(appsRoot, "src", "app", "logs", "page.tsx");
const pageHelpersPath = path.join(
  appsRoot,
  "src",
  "app",
  "logs",
  "page-helpers.tsx",
);
const pageCellsPath = path.join(
  appsRoot,
  "src",
  "app",
  "logs",
  "page-cells.tsx",
);

async function readPageSectionsSource() {
  return fs.readFile(pageSectionsPath, "utf8");
}

async function readLogsPageSource() {
  return fs.readFile(logsPagePath, "utf8");
}

async function readPageHelpersSource() {
  return fs.readFile(pageHelpersPath, "utf8");
}

async function readPageCellsSource() {
  return fs.readFile(pageCellsPath, "utf8");
}

function indexOfOrThrow(source, needle) {
  const index = source.indexOf(needle);
  assert.notEqual(index, -1, `missing source fragment: ${needle}`);
  return index;
}

test("request logs table keeps route details after token metrics", async () => {
  const source = await readPageSectionsSource();
  const tableStart = indexOfOrThrow(source, '<Table className="min-w-');
  const tableSource = source.slice(tableStart);

  const accountHeader = indexOfOrThrow(tableSource, '{t("账号 / 密钥")}');
  const modelHeader = indexOfOrThrow(tableSource, '{t("模型 / 推理 / 等级")}');
  const tokenHeader = indexOfOrThrow(tableSource, '{t("Token")}');
  const routeHeader = indexOfOrThrow(tableSource, '{t("类型 / 方法 / 路径")}');
  const errorHeader = indexOfOrThrow(tableSource, '{t("错误")}');

  assert.ok(accountHeader < modelHeader);
  assert.ok(modelHeader < tokenHeader);
  assert.ok(tokenHeader < routeHeader);
  assert.ok(routeHeader < errorHeader);
});

test("request logs refresh keeps request-log session-title lookup current", async () => {
  const source = await readLogsPageSource();

  assert.match(source, /const REQUEST_LOG_SESSION_LOOKUP_QUERY_KEY = \[/);
  assert.match(source, /serviceClient\.listRequestLogSessionTitles\(\{ limit: 2000 \}\)/);
  assert.match(
    source,
    /queryKey:\s*REQUEST_LOG_SESSION_LOOKUP_QUERY_KEY,[\s\S]*staleTime:\s*5_000,[\s\S]*refetchInterval:\s*5000,/,
  );
  assert.match(
    source,
    /queryClient\.invalidateQueries\(\{\s*queryKey:\s*REQUEST_LOG_SESSION_LOOKUP_QUERY_KEY,\s*\}\)/,
  );
});

test("request log summary cards show guard retry usage as a separate hint", async () => {
  const helpersSource = await readPageHelpersSource();
  const sectionsSource = await readPageSectionsSource();

  assert.match(helpersSource, /detail\?: ReactNode/);
  assert.match(helpersSource, /text-\[11px\] font-medium text-amber-500/);
  assert.match(
    sectionsSource,
    /summary\.guardRetryTotalTokens > 0[\s\S]*Guard \+\$\{formatCompactTokenAmount\(summary\.guardRetryTotalTokens\)\}/,
  );
  assert.match(
    sectionsSource,
    /summary\.guardRetryEstimatedCostUsd > 0[\s\S]*Guard \+\$\{formatUsdAmount\(summary\.guardRetryEstimatedCostUsd\)\}/,
  );
});

test("request logs use compact result metrics without model usage statistics", async () => {
  const sectionsSource = await readPageSectionsSource();
  const logsPageSource = await readLogsPageSource();

  assert.doesNotMatch(sectionsSource, /ModelUsageStatsCard/);
  assert.match(sectionsSource, /<RequestResultSummaryCard/);
  assert.match(sectionsSource, /title=\{t\("累计费用"\)\}/);
  assert.match(sectionsSource, /value=\{formatUsdAmount\(summary\.totalCostUsd\)\}/);
  assert.match(logsPageSource, /sessionTitleMap=\{requestLogSessionMap\}/);
  assert.match(sectionsSource, /<SessionInfoCell[\s\S]*session=\{sessionTitleMap\.get/);
  assert.match(sectionsSource, /formatOutputRate\(log\.outputTokens, log\.durationMs\)/);
  assert.match(sectionsSource, /formatCacheRate\(log\.inputTokens, log\.cachedInputTokens\)/);
  assert.match(sectionsSource, /formatUsdAmount\(log\.estimatedCostUsd\)/);
  assert.match(sectionsSource, /formatTableTokenAmount\(log\.cacheWriteInputTokens\)/);
  assert.match(sectionsSource, /log\.billableTotalTokens !== log\.totalTokens/);
  assert.match(sectionsSource, /\{t\("历史长上下文候选"\)\}/);
  assert.match(
    sectionsSource,
    /formatDuration\(log\.durationMs\)\}\/[\s\S]*getFirstResponseLatencyClass\(log\.firstResponseMs\)[\s\S]*formatDuration\(log\.firstResponseMs\)[\s\S]*formatOutputRate\(log\.outputTokens, log\.durationMs\)/,
  );
  assert.match(
    sectionsSource,
    /log\.billableEstimatedCostUsd !== log\.estimatedCostUsd/,
  );
});

test("request logs classify first-response latency without labeling local estimates", async () => {
  const helpersSource = await readPageHelpersSource();
  const sectionsSource = await readPageSectionsSource();

  assert.match(helpersSource, /value <= 10_000[\s\S]*text-emerald-600/);
  assert.match(helpersSource, /value <= 20_000[\s\S]*text-orange-600/);
  assert.match(helpersSource, /value <= 30_000[\s\S]*text-yellow-600/);
  assert.match(helpersSource, /text-red-600/);
  assert.match(helpersSource, /text-muted-foreground/);
  assert.match(sectionsSource, /getFirstResponseLatencyClass\(log\.firstResponseMs\)/);
  assert.doesNotMatch(sectionsSource, /pricingCostSource === "local_estimate"/);
});

test("request log session titles use readable primary and secondary typography", async () => {
  const cellsSource = await readPageCellsSource();

  assert.match(cellsSource, /text-xs leading-4 font-medium text-foreground/);
  assert.match(cellsSource, /font-mono text-\[10px\] leading-3 text-muted-foreground\/80/);
  assert.match(cellsSource, /truncate/);
  assert.match(cellsSource, /TooltipContent/);
});
