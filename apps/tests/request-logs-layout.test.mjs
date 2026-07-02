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

async function readPageSectionsSource() {
  return fs.readFile(pageSectionsPath, "utf8");
}

async function readLogsPageSource() {
  return fs.readFile(logsPagePath, "utf8");
}

async function readPageHelpersSource() {
  return fs.readFile(pageHelpersPath, "utf8");
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

test("request logs refresh keeps Codex session lookup current", async () => {
  const source = await readLogsPageSource();

  assert.match(source, /const REQUEST_LOG_SESSION_LOOKUP_QUERY_KEY = \[/);
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
