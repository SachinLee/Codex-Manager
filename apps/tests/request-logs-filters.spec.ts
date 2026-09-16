import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";

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

const SESSION_TITLE = "请求日志独立筛选验证会话标题";

type RpcParams = Record<string, unknown>;

interface RpcState {
  listParams: RpcParams[];
  emptyResult?: boolean;
}

async function mockLogsRpc(page: Page, state: RpcState) {
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
    const params = (payload?.params ?? {}) as RpcParams;
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
    if (method === "aggregateApi/list") {
      await ok({ items: [] });
      return;
    }
    if (method === "apikey/list") {
      await ok({
        items: [
          { id: "key-alpha", name: "Alpha 密钥" },
          { id: "key-beta", name: "" },
        ],
      });
      return;
    }
    if (method === "apikey/managedModelListV2") {
      await ok({
        items: [
          {
            id: "m-gpt-5-codex",
            slug: "gpt-5-codex",
            displayName: "GPT-5 Codex",
            description: null,
            provider: null,
            family: null,
            category: null,
            tags: [],
            origin: "custom",
            enabled: true,
            supportedInApi: true,
            visibility: "list",
            sortOrder: 1,
            contextWindow: null,
            maxContextWindow: null,
            defaultReasoningEffort: null,
            capabilities: {},
            instructionsMode: "none",
            instructionsText: null,
            fastPolicy: "none",
            builtinRevision: null,
            userEdited: false,
            price: {},
            priceTiers: [],
            routes: [],
            permissionGroupIds: [],
            fallbackModelSlugs: [],
            createdAt: 0,
            updatedAt: 0,
          },
        ],
        stats: {
          total: 1,
          enabled: 1,
          builtin: 0,
          custom: 1,
          priceMissing: 0,
          missingRoute: 0,
        },
      });
      return;
    }
    if (method === "requestlog/sessionTitles") {
      await ok([
        {
          sessionId: "session-filter-1",
          title: SESSION_TITLE,
          parentSessionId: null,
          parentTitle: null,
          cwd: null,
          source: "codex",
        },
      ]);
      return;
    }
    if (method === "requestlog/list_with_summary") {
      state.listParams.push(params);
      if (state.emptyResult) {
        await ok({
          items: [],
          total: 0,
          page: Number(params.page) || 1,
          pageSize: Number(params.pageSize) || 10,
          summary: {
            totalCount: 0,
            filteredCount: 0,
            successCount: 0,
            errorCount: 0,
            totalTokens: 0,
            totalCostUsd: 0,
            inputTokens: 0,
            cachedInputTokens: 0,
          },
        });
        return;
      }
      await ok({
        items: [
          {
            trace_id: "trace-filter-1",
            session_id: "session-filter-1",
            key_id: "key-alpha",
            request_path: "/v1/responses",
            method: "POST",
            request_type: "http",
            model: "gpt-5-codex",
            status_code: 200,
            duration_ms: 1200,
            first_response_ms: 220,
            input_tokens: 120,
            cached_input_tokens: 30,
            output_tokens: 34,
            total_tokens: 154,
            estimated_cost_usd: 0.000432,
            created_at: 1770000000,
          },
        ],
        total: 1,
        page: Number(params.page) || 1,
        pageSize: Number(params.pageSize) || 10,
        summary: {
          totalCount: 1,
          filteredCount: 1,
          successCount: 1,
          errorCount: 0,
          totalTokens: 154,
          totalCostUsd: 0.000432,
          inputTokens: 120,
          cachedInputTokens: 30,
        },
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
        inputTokens: 120,
        cachedInputTokens: 30,
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
}

function lastListParams(state: RpcState): RpcParams {
  return state.listParams[state.listParams.length - 1] ?? {};
}

test("request logs expose independent title, model and api-key filters", async ({ page }) => {
  const state: RpcState = { listParams: [] };
  await mockLogsRpc(page, state);
  await page.goto("/logs/");

  await page.getByRole("button", { name: "展开筛选" }).click();

  // 三个条件各自独立渲染，不再需要先选择查询类型。
  const titleInput = page.getByLabel("标题", { exact: true });
  const modelSelect = page.getByRole("combobox", { name: "模型", exact: true });
  const keySelect = page.getByRole("combobox", { name: "平台密钥", exact: true });
  await expect(titleInput).toBeVisible();
  await expect(modelSelect).toBeVisible();
  await expect(keySelect).toBeVisible();
  await expect(page.getByRole("combobox", { name: "全部字段" })).toHaveCount(0);

  // 未设置任何条件时不传显式字段，保持既有结果。
  await expect.poll(() => state.listParams.length).toBeGreaterThan(0);
  expect(lastListParams(state).sessionIds).toEqual([]);
  expect(lastListParams(state).model).toBeNull();
  expect(lastListParams(state).keyId).toBeNull();

  // 模型下拉来自平台模型目录。
  await modelSelect.click();
  await page.getByRole("option", { name: "gpt-5-codex", exact: true }).click();
  await expect.poll(() => lastListParams(state).model).toBe("gpt-5-codex");

  // 平台密钥下拉展示密钥名称。
  await keySelect.click();
  await page.getByRole("option", { name: "Alpha 密钥", exact: true }).click();
  await expect.poll(() => lastListParams(state).keyId).toBe("key-alpha");

  // 标题经会话标题侧车解析成会话 ID 后交给服务端。
  await titleInput.fill("独立筛选");
  await expect
    .poll(() => lastListParams(state).sessionIds)
    .toEqual(["session-filter-1"]);

  // 三个条件同时生效，属于 AND 组合。
  const combined = lastListParams(state);
  expect(combined.sessionIds).toEqual(["session-filter-1"]);
  expect(combined.model).toBe("gpt-5-codex");
  expect(combined.keyId).toBe("key-alpha");
});

test("legacy structured query bookmarks remain server-side filters", async ({ page }) => {
  const state: RpcState = { listParams: [] };
  await mockLogsRpc(page, state);

  await page.goto("/logs/?query=model%3Agpt-5-codex");

  await expect
    .poll(() => lastListParams(state).query)
    .toBe("model:gpt-5-codex");
  expect(lastListParams(state).sessionIds).toEqual([]);
  expect(lastListParams(state).model).toBeNull();
});

test("request log summary shows cache rate for the filtered result", async ({ page }) => {
  const state: RpcState = { listParams: [] };
  await mockLogsRpc(page, state);
  await page.goto("/logs/");

  await page.getByRole("button", { name: "展开筛选" }).click();

  // inputTokens=120 / cachedInputTokens=30 → 25%
  const cacheCard = page.getByText("当前筛选结果中的缓存率", { exact: true });
  await expect(cacheCard).toBeVisible();
  await expect(cacheCard.locator("xpath=ancestor::div[1]")).toContainText("25%");
});

test("request log cache rate falls back to dash without input tokens", async ({ page }) => {
  const state: RpcState = { listParams: [], emptyResult: true };
  await mockLogsRpc(page, state);
  await page.goto("/logs/");

  await page.getByRole("button", { name: "展开筛选" }).click();

  const cacheCard = page.getByText("当前筛选结果中的缓存率", { exact: true });
  await expect(cacheCard).toBeVisible();
  // 零输入 Token 显示 "-"，不能伪装成已计算的 0%。
  await expect(cacheCard.locator("xpath=ancestor::div[1]")).toContainText("-");
  await expect(cacheCard.locator("xpath=ancestor::div[1]")).not.toContainText("0%");
});
