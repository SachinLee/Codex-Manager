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
  sseKeepaliveIntervalMs: 15000,
  backgroundTasks: {
    usagePollingEnabled: true,
    usagePollIntervalSecs: 600,
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

test("request logs display session title, total duration, first-response latency, and output rate", async ({
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
        body: JSON.stringify({
          jsonrpc: "2.0",
          id,
          result,
        }),
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
    if (method === "account/list") {
      await ok({ items: [], total: 0, page: 1, pageSize: 10 });
      return;
    }
    if (method === "apikey/list" || method === "aggregateApi/list") {
      await ok({ items: [] });
      return;
    }
    if (method === "codexSession/list") {
      await ok([
        {
          sessionId: "session-duration-1",
          title:
            "请求日志会话标题展示和长标题截断验证用的非常长会话标题",
          createdAt: 1769999000,
          updatedAt: 1770000000,
          cwd: "D:/my-works/codex-extends/Codex-Manager",
          archived: false,
        },
      ]);
      return;
    }
    if (method === "requestlog/list") {
      await ok({
        items: [
          {
            trace_id: "trace-duration-1",
            session_id: "session-duration-1",
            conversation_anchor: "pck:v1:47cafe1234567890",
            key_id: "key-duration-1",
            request_path: "/v1/responses",
            original_path: "/v1/responses",
            adapted_path: "/v1/chat/completions",
            method: "POST",
            request_type: "http",
            gateway_mode: "compact",
            model: "gpt-5.3-codex",
            upstream_model: "gpt-5.4-openai-compact",
            upstream_url: "https://chatgpt.com/backend-api/codex/responses",
            status_code: 200,
            duration_ms: 2345,
            first_response_ms: 340,
            input_tokens: 120,
            cached_input_tokens: 30,
            output_tokens: 34,
            total_tokens: 154,
            estimated_cost_usd: 0.000432,
            created_at: 1770000000,
          },
        ],
        total: 1,
        page: 1,
        pageSize: 10,
      });
      return;
    }
    if (method === "requestlog/summary") {
      await ok({
        totalCount: 1,
        filteredCount: 1,
        successCount: 1,
        errorCount: 0,
        totalTokens: 154,
        totalCostUsd: 0.000432,
      });
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

  await page.goto("/logs/");

  await expect(page.getByRole("columnheader", { name: "会话" })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "用时 / 首响 / 输出速率" })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "缓存率" })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "费用" })).toBeVisible();
  await expect(page.getByText("2.3s/340ms/14.50 tok/s")).toBeVisible();
  await expect(page.getByText("25%")).toBeVisible();
  await expect(page.getByText("$0.0004")).toBeVisible();
  await expect(
    page.getByText("请求日志会话标题展示和长标题截断验证用的非常长会话标题"),
  ).toBeVisible();
  await expect(page.getByText("session-duration-1")).toBeVisible();
  await expect(page.getByText("/v1/responses")).toBeVisible();
  await expect(page.getByText("压缩", { exact: true })).toBeVisible();
  await expect(page.getByText("-> /v1/chat/completions")).toBeVisible();
  await expect(
    page.getByText("=> chatgpt.com/backend-api/codex/responses"),
  ).toBeVisible();
  await expect(page.getByText("转发 gpt-5.4-openai-compact")).toBeVisible();
});
