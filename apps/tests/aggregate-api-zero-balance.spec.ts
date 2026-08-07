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
  upstreamStreamTimeoutMs: 600_000,
  upstreamTotalTimeoutMs: 0,
  sseKeepaliveIntervalSecs: 15_000,
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
  id: "agg-zero-balance",
  providerType: "compatible",
  supplierName: "Zero Balance Supplier",
  sort: 0,
  url: "https://zero-balance.invalid/v1",
  authType: "apikey",
  status: "active",
  balanceQueryEnabled: true,
  modelSlugs: ["gpt-5.6"],
};

test("administrator releases a zero-balance route block without resetting cooldown", async ({
  page,
}) => {
  let zeroBalanceStatus = {
    aggregateApiId: AGGREGATE_API.id,
    state: "zero_balance_blocked",
    observedAt: 1_700_000_000,
    releasedAt: null,
    updatedAt: 1_700_000_000,
  };
  let resetParams: unknown;

  await page.route("**/api/runtime**", async (route) => {
    await route.fulfill({
      contentType: "application/json; charset=utf-8",
      body: JSON.stringify({
        mode: "web-gateway",
        rpcBaseUrl: "/api/rpc",
        canManageService: true,
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
      await ok({ items: [AGGREGATE_API] });
      return;
    }
    if (method === "aggregateApi/zeroBalanceStatus/list") {
      await ok({ items: [zeroBalanceStatus] });
      return;
    }
    if (method === "aggregateApi/zeroBalanceStatus/reset") {
      resetParams = payload?.params;
      zeroBalanceStatus = {
        ...zeroBalanceStatus,
        state: "manually_released",
        releasedAt: 1_700_000_100,
        updatedAt: 1_700_000_100,
      };
      await ok(zeroBalanceStatus);
      return;
    }
    if (
      method === "aggregateApi/runtimeStatus/list" ||
      method === "aggregateApi/health/list" ||
      method === "aggregateApi/health/costs" ||
      method === "requestlog/aggregate_api_daily_usage" ||
      method === "requestlog/model_daily_usage"
    ) {
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

  await page.goto("/aggregate-api/");

  const aggregateRow = page
    .locator("tbody tr")
    .filter({ hasText: AGGREGATE_API.supplierName })
    .first();
  await expect(aggregateRow.getByText("余额为 0", { exact: false })).toBeVisible();
  const releaseButton = aggregateRow.getByRole("button", { name: "解除余额禁用" });
  await expect(releaseButton).toBeEnabled();

  await releaseButton.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("解除后仅撤销零余额路由排除", { exact: false })).toBeVisible();
  await dialog.getByRole("button", { name: "确认解除余额禁用" }).click();

  await expect
    .poll(() => (resetParams as { id?: unknown } | undefined)?.id)
    .toBe(AGGREGATE_API.id);
  await expect(aggregateRow.getByText("已手动放行", { exact: false })).toBeVisible();
  await expect(releaseButton).toHaveCount(0);
});
