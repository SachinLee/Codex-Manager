import { expect, test } from "@playwright/test";

const SETTINGS_SNAPSHOT = {
  updateAutoCheck: true,
  closeToTrayOnClose: false,
  closeToTraySupported: false,
  lowTransparency: false,
  lightweightModeOnCloseToTray: false,
  codexCliGuideDismissed: true,
  webAccessPasswordConfigured: false,
  locale: "zh-CN",
  localeOptions: ["zh-CN", "en"],
  serviceAddr: "localhost:48760",
  serviceListenMode: "loopback",
  serviceListenModeOptions: ["loopback", "all_interfaces"],
  routeStrategy: "ordered",
  routeStrategyOptions: ["ordered", "balanced"],
  freeAccountMaxModel: "auto",
  freeAccountMaxModelOptions: ["auto", "gpt-5"],
  modelForwardRules: "",
  accountMaxInflight: 1,
  gatewayOriginator: "codex-cli",
  gatewayOriginatorDefault: "codex-cli",
  gatewayUserAgentVersion: "1.0.0",
  gatewayUserAgentVersionDefault: "1.0.0",
  gatewayResidencyRequirement: "",
  gatewayResidencyRequirementOptions: ["", "us"],
  pluginMarketMode: "builtin",
  pluginMarketSourceUrl: "",
  upstreamProxyUrl: "",
  upstreamStreamTimeoutMs: 600000,
  upstreamTotalTimeoutMs: 0,
  sseKeepaliveIntervalMs: 15000,
  backgroundTasks: {
    usagePollingEnabled: true,
    usagePollIntervalSecs: 30,
    gatewayKeepaliveEnabled: true,
    gatewayKeepaliveIntervalSecs: 180,
    tokenRefreshPollingEnabled: true,
    tokenRefreshPollIntervalSecs: 60,
    usageRefreshWorkers: 4,
    httpWorkerFactor: 4,
    httpWorkerMin: 8,
    httpStreamWorkerFactor: 1,
    httpStreamWorkerMin: 2,
  },
  envOverrides: {},
  envOverrideCatalog: [],
  envOverrideReservedKeys: [],
  envOverrideUnsupportedKeys: [],
  theme: "tech",
  appearancePreset: "classic",
};

const AGGREGATE_API = {
  id: "agg-usage-refresh",
  providerType: "compatible",
  supplierName: "Usage Refresh Supplier",
  sort: 0,
  url: "https://usage-refresh.invalid/v1",
  authType: "apikey",
  status: "active",
  balanceQueryEnabled: false,
  modelSlugs: ["gpt-5.4"],
};

const AGGREGATE_APIS = [
  AGGREGATE_API,
  {
    ...AGGREGATE_API,
    id: "agg-sort-a",
    supplierName: "Stable tie B",
    sort: 5,
  },
  {
    ...AGGREGATE_API,
    id: "agg-sort-b",
    supplierName: "Stable tie A",
    sort: 5,
  },
  {
    ...AGGREGATE_API,
    id: "agg-sort-last",
    supplierName: "Last sort value",
    sort: 20,
  },
];

const USAGE_SNAPSHOTS = [
  {
    aggregateApiId: AGGREGATE_API.id,
    aggregateApiSupplierName: AGGREGATE_API.supplierName,
    aggregateApiUrl: AGGREGATE_API.url,
    requestCount: 19,
    inputTokens: 18_000_000,
    cachedInputTokens: 4_000_000,
    cacheWriteInputTokens: 0,
    billableInputTokens: 14_000_000,
    outputTokens: 1_000_000,
    totalTokens: 19_000_000,
    reasoningOutputTokens: 250_000,
    estimatedCostUsd: 19.27,
    guardRetryTotalTokens: 0,
    guardRetryEstimatedCostUsd: 0,
    billableTotalTokens: 19_000_000,
    billableEstimatedCostUsd: 19.27,
    cacheHitRate: 4 / 18,
  },
  {
    aggregateApiId: AGGREGATE_API.id,
    aggregateApiSupplierName: AGGREGATE_API.supplierName,
    aggregateApiUrl: AGGREGATE_API.url,
    requestCount: 48,
    inputTokens: 45_000_000,
    cachedInputTokens: 12_000_000,
    cacheWriteInputTokens: 0,
    billableInputTokens: 33_000_000,
    outputTokens: 3_000_000,
    totalTokens: 48_000_000,
    reasoningOutputTokens: 750_000,
    estimatedCostUsd: 48.35,
    guardRetryTotalTokens: 0,
    guardRetryEstimatedCostUsd: 0,
    billableTotalTokens: 48_000_000,
    billableEstimatedCostUsd: 48.35,
    cacheHitRate: 12 / 45,
  },
  {
    aggregateApiId: AGGREGATE_API.id,
    aggregateApiSupplierName: AGGREGATE_API.supplierName,
    aggregateApiUrl: AGGREGATE_API.url,
    requestCount: 52,
    inputTokens: 49_000_000,
    cachedInputTokens: 14_000_000,
    cacheWriteInputTokens: 0,
    billableInputTokens: 35_000_000,
    outputTokens: 3_000_000,
    totalTokens: 52_000_000,
    reasoningOutputTokens: 800_000,
    estimatedCostUsd: 52.75,
    guardRetryTotalTokens: 0,
    guardRetryEstimatedCostUsd: 0,
    billableTotalTokens: 52_000_000,
    billableEstimatedCostUsd: 52.75,
    cacheHitRate: 14 / 49,
  },
];

const MODEL_USAGE = {
  model: "gpt-5.4",
  requestCount: 1,
  inputTokens: 1_000_000,
  cachedInputTokens: 200_000,
  cacheWriteInputTokens: 0,
  billableInputTokens: 800_000,
  outputTokens: 100_000,
  totalTokens: 1_100_000,
  reasoningOutputTokens: 50_000,
  estimatedCostUsd: 1.25,
  cacheHitRate: 0.2,
};

test("aggregate API usage refreshes while active and resumes after keep-alive navigation", async ({
  page,
}) => {
  let usageVersion = 0;
  let aggregateUsageCallCount = 0;
  let modelUsageCallCount = 0;

  await page.route("**/api/runtime**", async (route) => {
    await route.fulfill({
      contentType: "application/json; charset=utf-8",
      body: JSON.stringify({
        mode: "web-gateway",
        rpcBaseUrl: "/api/rpc",
        canManageService: false,
        canSelfUpdate: false,
        canCloseToTray: false,
        canOpenLocalDir: false,
        canUseBrowserFileImport: true,
        canUseBrowserDownloadExport: true,
      }),
    });
  });

  await page.route("**/api/rpc**", async (route) => {
    const payload = route.request().postDataJSON();
    const method = typeof payload?.method === "string" ? payload.method : "";
    const id = payload?.id ?? 1;

    const ok = (result: unknown) =>
      route.fulfill({
        contentType: "application/json; charset=utf-8",
        body: JSON.stringify({ jsonrpc: "2.0", id, result }),
      });

    if (method === "appSettings/get") {
      await ok(SETTINGS_SNAPSHOT);
      return;
    }
    if (method === "initialize") {
      await ok({
        userAgent: "codex_cli_rs/0.1.19",
        codexHome: "C:/Users/Test/.codex",
        platformFamily: "windows",
        platformOs: "windows",
      });
      return;
    }
    if (method === "accountManager/session/current") {
      await ok({
        mode: "none",
        currentUser: null,
        role: "system_admin",
        permissions: ["system:admin"],
        distributionEnabled: false,
      });
      return;
    }
    if (method === "aggregateApi/list") {
      await ok({ items: AGGREGATE_APIS });
      return;
    }
    if (method === "aggregateApi/runtimeStatus/list") {
      await ok({ items: [] });
      return;
    }
    if (method === "gateway/concurrencyRecommendation/get") {
      await ok({
        usageRefreshWorkers: 4,
        httpWorkerFactor: 4,
        httpWorkerMin: 8,
        httpStreamWorkerFactor: 1,
        httpStreamWorkerMin: 2,
        accountMaxInflight: 1,
      });
      return;
    }
    if (method === "requestlog/aggregate_api_daily_usage") {
      aggregateUsageCallCount += 1;
      await ok({ items: [USAGE_SNAPSHOTS[usageVersion]] });
      return;
    }
    if (method === "requestlog/model_daily_usage") {
      modelUsageCallCount += 1;
      await ok({ items: [{ ...MODEL_USAGE, requestCount: modelUsageCallCount }] });
      return;
    }

    await route.fulfill({
      status: 500,
      contentType: "application/json; charset=utf-8",
      body: JSON.stringify({
        jsonrpc: "2.0",
        id,
        error: {
          code: -32000,
          message: `Unhandled RPC method in test: ${method}`,
        },
      }),
    });
  });

  const waitForUsageResponses = () =>
    Promise.all([
      page.waitForResponse(
        (response) => {
          if (!response.url().includes("/api/rpc")) return false;
          const payload = response.request().postDataJSON();
          return payload?.method === "requestlog/aggregate_api_daily_usage";
        },
        { timeout: 2_000 },
      ),
      page.waitForResponse(
        (response) => {
          if (!response.url().includes("/api/rpc")) return false;
          const payload = response.request().postDataJSON();
          return payload?.method === "requestlog/model_daily_usage";
        },
        { timeout: 2_000 },
      ),
    ]);

  await page.goto("/aggregate-api/");

  const costMetric = page.locator(".console-metric").filter({ hasText: "今日费用" });
  const aggregateRow = page
    .locator("tbody tr")
    .filter({ hasText: AGGREGATE_API.supplierName })
    .first();
  const modelUsageRow = page
    .locator(".glass-card")
    .filter({ hasText: "今日模型用量" })
    .locator("tbody tr")
    .filter({ hasText: MODEL_USAGE.model });
  const modelUsageCard = page.locator(".glass-card").filter({ hasText: "今日模型用量" });
  const apiTable = page
    .locator(".glass-card")
    .filter({ hasText: "上游连接" })
    .locator("table");

  await expect(costMetric.getByText("$19.27", { exact: true })).toBeVisible();
  await expect(aggregateRow.getByText("19M tok", { exact: true })).toBeVisible();
  await expect(aggregateRow.getByText(/\$19\.27 · cache/)).toBeVisible();
  await expect(modelUsageRow).toHaveCount(0);
  await expect(apiTable.locator("tbody tr > td:first-child")).toHaveText([
    "0",
    "5",
    "5",
    "20",
  ]);
  await expect(apiTable.locator("tbody tr > td:nth-child(2)")).toContainText([
    AGGREGATE_API.supplierName,
    "Stable tie B",
    "Stable tie A",
    "Last sort value",
  ]);

  await modelUsageCard.getByRole("button", { name: "展开", exact: true }).click();
  await expect(modelUsageRow).toBeVisible();

  usageVersion = 1;
  await expect(costMetric.getByText("$48.35", { exact: true })).toBeVisible({
    timeout: 8_000,
  });
  await expect(aggregateRow.getByText("48M tok", { exact: true })).toBeVisible();
  await expect(aggregateRow.getByText(/\$48\.35 · cache/)).toBeVisible();
  expect(aggregateUsageCallCount).toBeGreaterThanOrEqual(2);
  await expect.poll(() => modelUsageCallCount).toBeGreaterThanOrEqual(2);

  usageVersion = 2;
  const callsBeforeFocus = aggregateUsageCallCount;
  const modelCallsBeforeFocus = modelUsageCallCount;
  const focusResponses = waitForUsageResponses();
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      get: () => "hidden",
    });
    window.dispatchEvent(new Event("visibilitychange"));
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      get: () => "visible",
    });
    window.dispatchEvent(new Event("visibilitychange"));
    Reflect.deleteProperty(document, "visibilityState");
  });
  await focusResponses;
  await expect(costMetric.getByText("$52.75", { exact: true })).toBeVisible();
  await expect(aggregateRow.getByText("52M tok", { exact: true })).toBeVisible();
  await expect(
    modelUsageRow.getByText(String(modelUsageCallCount), { exact: true }),
  ).toBeVisible();
  expect(aggregateUsageCallCount).toBeGreaterThan(callsBeforeFocus);
  expect(modelUsageCallCount).toBeGreaterThan(modelCallsBeforeFocus);

  usageVersion = 1;
  const callsBeforeReconnect = aggregateUsageCallCount;
  const modelCallsBeforeReconnect = modelUsageCallCount;
  const reconnectResponses = waitForUsageResponses();
  await page.evaluate(() => {
    window.dispatchEvent(new Event("offline"));
    window.dispatchEvent(new Event("online"));
  });
  await reconnectResponses;
  await expect(costMetric.getByText("$48.35", { exact: true })).toBeVisible();
  await expect(aggregateRow.getByText("48M tok", { exact: true })).toBeVisible();
  await expect(
    modelUsageRow.getByText(String(modelUsageCallCount), { exact: true }),
  ).toBeVisible();
  expect(aggregateUsageCallCount).toBeGreaterThan(callsBeforeReconnect);
  expect(modelUsageCallCount).toBeGreaterThan(modelCallsBeforeReconnect);

  await page.getByRole("link", { name: "系统设置", exact: true }).click();
  await expect(page).toHaveURL(/\/settings\/$/);
  await expect(page.getByText("系统设置", { exact: true }).last()).toBeVisible();

  await page.waitForTimeout(500);
  const inactiveAggregateUsageCallCount = aggregateUsageCallCount;
  const inactiveModelUsageCallCount = modelUsageCallCount;
  await page.waitForTimeout(5_500);
  expect(aggregateUsageCallCount).toBe(inactiveAggregateUsageCallCount);
  expect(modelUsageCallCount).toBe(inactiveModelUsageCallCount);

  usageVersion = 2;
  const callsBeforeReturn = aggregateUsageCallCount;
  await page.getByRole("link", { name: /聚合\s*API/ }).click();
  await expect(page).toHaveURL(/\/aggregate-api\/$/);
  await expect(costMetric.getByText("$52.75", { exact: true })).toBeVisible({
    timeout: 2_000,
  });
  await expect(aggregateRow.getByText("52M tok", { exact: true })).toBeVisible();
  expect(aggregateUsageCallCount).toBeGreaterThan(callsBeforeReturn);
  await expect
    .poll(() => modelUsageCallCount, { timeout: 2_000 })
    .toBeGreaterThan(inactiveModelUsageCallCount);
});
