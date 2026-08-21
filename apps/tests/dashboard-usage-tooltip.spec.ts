import { expect, test, type Locator, type Page } from "@playwright/test";

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

const MODEL_FIRST_BUCKET = {
  inputTokens: 1_000,
  cachedInputTokens: 200,
  cacheWriteInputTokens: 0,
  outputTokens: 999_000,
  reasoningOutputTokens: 0,
  totalTokens: 1_000_000,
  estimatedCostUsd: 1.25,
  requestCount: 11,
  successCount: 11,
  errorCount: 0,
};

const MODEL_SECOND_BUCKET = {
  ...MODEL_FIRST_BUCKET,
  inputTokens: 0,
  cachedInputTokens: 1_000,
  totalTokens: 2_000_000,
  estimatedCostUsd: 2.5,
  requestCount: 22,
  successCount: 22,
};
const MODEL_THIRD_BUCKET = {
  ...MODEL_FIRST_BUCKET,
  cachedInputTokens: -200,
  totalTokens: 1_500_000,
  estimatedCostUsd: 1.875,
  requestCount: 16,
  successCount: 16,
};

const TOTAL_FIRST_BUCKET = {
  ...MODEL_FIRST_BUCKET,
  inputTokens: 10_000,
  cachedInputTokens: 15_000,
  totalTokens: 3_000_000,
  estimatedCostUsd: 3.75,
  requestCount: 99,
  successCount: 99,
};

const TOTAL_SECOND_BUCKET = {
  ...MODEL_SECOND_BUCKET,
  inputTokens: 20_000,
  cachedInputTokens: 5_000,
  totalTokens: 6_000_000,
  estimatedCostUsd: 7.5,
  requestCount: 198,
  successCount: 198,
};

const TOTAL_THIRD_BUCKET = {
  ...MODEL_THIRD_BUCKET,
  inputTokens: 15_000,
  cachedInputTokens: 3_000,
  totalTokens: 4_500_000,
  estimatedCostUsd: 5.625,
  requestCount: 148,
  successCount: 148,
};

const FIRST_BUCKET_START = 1_735_689_600;
const HOUR_SECONDS = 3_600;

function seriesPoint(bucketStartTs: number, usage: Record<string, number>) {
  return {
    bucketStartTs,
    bucketEndTs: bucketStartTs + HOUR_SECONDS,
    usage,
  };
}

const ADMIN_USAGE_SUMMARY = {
  rangeStartTs: FIRST_BUCKET_START,
  rangeEndTs: FIRST_BUCKET_START + HOUR_SECONDS * 3,
  todayStartTs: FIRST_BUCKET_START,
  todayEndTs: FIRST_BUCKET_START + HOUR_SECONDS * 3,
  totalUsage: TOTAL_SECOND_BUCKET,
  todayUsage: TOTAL_SECOND_BUCKET,
  dailyUsage: [
    {
      dayStartTs: FIRST_BUCKET_START,
      dayEndTs: FIRST_BUCKET_START + HOUR_SECONDS * 3,
      usage: TOTAL_SECOND_BUCKET,
    },
  ],
  seriesBucketSeconds: HOUR_SECONDS,
  seriesUsage: [
    seriesPoint(FIRST_BUCKET_START, TOTAL_FIRST_BUCKET),
    seriesPoint(FIRST_BUCKET_START + HOUR_SECONDS, TOTAL_SECOND_BUCKET),
    seriesPoint(FIRST_BUCKET_START + HOUR_SECONDS * 2, TOTAL_THIRD_BUCKET),
  ],
  modelUsage: [
    {
      model: "model-cache-verified",
      usage: MODEL_SECOND_BUCKET,
      points: [
        seriesPoint(FIRST_BUCKET_START, MODEL_FIRST_BUCKET),
        seriesPoint(FIRST_BUCKET_START + HOUR_SECONDS, MODEL_SECOND_BUCKET),
        seriesPoint(FIRST_BUCKET_START + HOUR_SECONDS * 2, MODEL_THIRD_BUCKET),
      ],
    },
  ],
  users: [],
  openaiAccounts: [],
  aggregateApis: [],
};

async function hoverBucketWithText(
  page: Page,
  chart: Locator,
  expectedText: string,
) {
  const chartSurface = chart.getByRole("application");
  const bounds = await chartSurface.boundingBox();
  if (!bounds) {
    throw new Error("Usage trend chart has no layout bounds");
  }

  const tooltip = page.locator('[data-slot="chart-tooltip"]');
  for (const ratio of [0.08, 0.2, 0.35, 0.5, 0.65, 0.8, 0.92]) {
    await chartSurface.hover({
      position: {
        x: bounds.width * ratio,
        y: bounds.height * 0.35,
      },
    });
    await page.waitForTimeout(100);
    if (
      await tooltip
        .getByText(expectedText, { exact: true })
        .isVisible()
        .catch(() => false)
    ) {
      return tooltip;
    }
  }

  throw new Error(`Could not activate the usage bucket containing ${expectedText}`);
}

test("dashboard model usage tooltip shows bucket cost and cache rate without changing metrics", async ({
  page,
}) => {
  await page.route("**/api/runtime*", async (route) => {
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

  await page.route("**/api/rpc*", async (route) => {
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
        version: "0.5.3",
        userAgent: "codex_cli_rs/0.1.19",
        codexHome: "/tmp/.codex",
        platformFamily: "linux",
        platformOs: "linux",
      });
      return;
    }
    if (method === "accountManager/session/current") {
      await ok({
        mode: "server",
        role: "system_admin",
        currentUser: null,
        permissions: ["*"],
        distributionEnabled: true,
      });
      return;
    }
    if (method === "codexProfile/get") {
      await ok({
        codexHome: "/tmp/.codex",
        mode: "gateway",
        selectedAccountId: null,
        selectedApiKeyId: "key-dashboard",
        gatewayBaseUrl: "http://localhost:48760/v1",
        warnings: [],
      });
      return;
    }
    if (method === "startup/snapshot") {
      await ok({
        accounts: [],
        accountSummary: {
          accountCount: 0,
          availableCount: 0,
          lowQuotaCount: 0,
          primaryRemainPercent: null,
          secondaryRemainPercent: null,
          lastRefreshedAt: null,
        },
        usageSnapshots: [],
        usageAggregateSummary: {},
        apiKeys: [],
        apiModels: { models: [] },
        manualPreferredAccountId: "",
        requestLogTodaySummary: {},
        requestLogs: [],
      });
      return;
    }
    if (method === "dashboard/adminUsageSummary") {
      await ok(ADMIN_USAGE_SUMMARY);
      return;
    }

    await route.fulfill({
      status: 500,
      contentType: "application/json; charset=utf-8",
      body: JSON.stringify({
        jsonrpc: "2.0",
        id,
        error: { code: -32000, message: `Unhandled RPC method in test: ${method}` },
      }),
    });
  });

  await page.goto("/");

  const chart = page.getByLabel("模型用量趋势图");
  await expect(chart).toBeVisible();

  const modelTooltip = await hoverBucketWithText(page, chart, "1.00M");
  await expect(modelTooltip).toContainText("model-cache-verified");
  await expect(modelTooltip).toContainText("费用");
  await expect(modelTooltip).toContainText("$1.25");
  await expect(modelTooltip).toContainText("缓存率");
  await expect(modelTooltip).toContainText("20%");
  const zeroInputTooltip = await hoverBucketWithText(page, chart, "2.00M");
  await expect(zeroInputTooltip).toContainText("$2.50");
  await expect(zeroInputTooltip).toContainText("0%");

  const negativeCacheTooltip = await hoverBucketWithText(page, chart, "1.50M");
  await expect(negativeCacheTooltip).toContainText("$1.88");
  await expect(negativeCacheTooltip).toContainText("0%");

  await page.getByRole("button", { name: "请求数", exact: true }).click();
  const requestTooltip = await hoverBucketWithText(page, chart, "11");
  await expect(requestTooltip).toContainText("$1.25");
  await expect(requestTooltip).toContainText("20%");
  await page.getByRole("button", { name: "Token", exact: true }).click();

  await page.getByRole("button", { name: "全部模型", exact: true }).click();
  const totalTooltip = await hoverBucketWithText(page, chart, "3.00M");
  await expect(totalTooltip).toContainText("$3.75");
  await expect(totalTooltip).toContainText("100%");
});
