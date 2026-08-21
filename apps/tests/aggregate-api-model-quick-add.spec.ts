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
  id: "agg-quick-add",
  providerType: "compatible",
  supplierName: "Quick Add Supplier",
  sort: 0,
  url: "https://quick-add.invalid/v1",
  authType: "apikey",
  status: "active",
  balanceQueryEnabled: false,
  modelSlugs: [],
};

test("administrator adds a discovered aggregate model through the prefilled route dialog", async ({
  page,
}) => {
  let addRouteParams: unknown;

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
    if (method === "aggregateApi/models/discover") {
      await ok({
        apiId: AGGREGATE_API.id,
        ok: true,
        items: [{ id: "remote-gpt", displayName: "Remote GPT" }],
        statusCode: 200,
        discoveredAt: 1_770_000_000,
        message: null,
      });
      return;
    }
    if (method === "apikey/managedModelAddAggregateRouteV2") {
      addRouteParams = payload?.params;
      await ok({
        created: false,
        routeAction: "created",
        model: {
          id: "custom:remote-gpt",
          slug: "remote-gpt",
          displayName: "Remote GPT",
        },
      });
      return;
    }
    if (
      method === "aggregateApi/runtimeStatus/list" ||
      method === "aggregateApi/zeroBalanceStatus/list" ||
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
  await page.getByRole("button", { name: "发现上游模型" }).click();

  const discoveryDialog = page.getByRole("dialog").filter({ hasText: "上游 API 模型" });
  await discoveryDialog.getByRole("button", { name: "获取模型" }).click();
  await expect(discoveryDialog.getByText("Remote GPT（remote-gpt）", { exact: true })).toBeVisible();
  await discoveryDialog.getByRole("button", { name: "添加到模型与路由" }).click();

  const quickAddDialog = page.getByRole("dialog").filter({ hasText: "添加到模型与路由" });
  await expect(quickAddDialog.getByLabel("模型标识（Slug）")).toHaveValue("remote-gpt");
  await expect(quickAddDialog.getByLabel("显示名称")).toHaveValue("Remote GPT");
  await expect(quickAddDialog.getByText(AGGREGATE_API.id, { exact: true })).toBeVisible();
  await expect(quickAddDialog.getByText("remote-gpt", { exact: true })).toBeVisible();

  await expect(quickAddDialog.getByLabel("模型标识（Slug）")).toHaveAttribute(
    "aria-required",
    "true",
  );

  await quickAddDialog.getByLabel("显示名称").press("Enter");

  await expect.poll(() => addRouteParams).toEqual({
    slug: "remote-gpt",
    displayName: "Remote GPT",
    aggregateApiId: AGGREGATE_API.id,
    upstreamModel: "remote-gpt",
  });

  await expect(page.getByText("模型与路由已添加", { exact: true })).toBeVisible();
});
