import { readFile } from "node:fs/promises";
import { expect, test, type Page } from "@playwright/test";

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

type JsonObject = Record<string, unknown>;

type MockState = {
  models: JsonObject[];
  upserts: JsonObject[];
  deletes: string[];
  importCalls: Array<{ method: string; params: JsonObject }>;
  initializeCalls: number;
};

const PRICED_MODELS: Record<string, [number, number, number]> = {
  "gpt-5.6-sol": [5_000_000, 5_000_000, 30_000_000],
  "gpt-5.6-terra": [2_500_000, 2_500_000, 15_000_000],
  "gpt-5.6-luna": [1_000_000, 1_000_000, 6_000_000],
  "gpt-5.5": [5_000_000, 500_000, 30_000_000],
  "gpt-5.4": [2_500_000, 250_000, 15_000_000],
  "gpt-5.4-mini": [750_000, 75_000, 4_500_000],
  "gpt-5.2": [1_750_000, 175_000, 14_000_000],
};

function builtinModel(
  slug: string,
  sortOrder: number,
  visibility: "list" | "hide" = "list",
): JsonObject {
  const rates = PRICED_MODELS[slug] ?? null;
  const price = rates
    ? {
        priceStatus: slug.startsWith("gpt-5.6") ? "estimated" : "official",
        priceSource: slug.startsWith("gpt-5.6")
          ? "user_provided_openai_gpt-5.6_2026-07-14_cached_at_input_rate"
          : "seed-2026-05-11",
        inputMicrousdPer1m: rates[0],
        cachedInputMicrousdPer1m: rates[1],
        outputMicrousdPer1m: rates[2],
      }
    : {
        priceStatus: "missing",
        priceSource: null,
        inputMicrousdPer1m: null,
        cachedInputMicrousdPer1m: null,
        outputMicrousdPer1m: null,
      };
  return {
    id: `builtin:${slug}`,
    slug,
    displayName: slug.toUpperCase(),
    description: `${slug} builtin`,
    provider: "openai",
    family: "gpt-5",
    category: "reasoning",
    tags: ["coding"],
    origin: "builtin",
    enabled: true,
    supportedInApi: true,
    visibility,
    sortOrder,
    contextWindow: slug.startsWith("gpt-5.6") ? 372_000 : 272_000,
    maxContextWindow: slug === "gpt-5.4" ? 1_000_000 : 272_000,
    defaultReasoningEffort: "medium",
    capabilities: {
      reasoningEfforts: ["low", "medium", "high", "xhigh"],
      inputModalities: ["text", "image"],
      supportsParallelToolCalls: true,
    },
    instructionsMode: "passthrough",
    instructionsText: null,
    builtinRevision: 2,
    userEdited: false,
    price,
    priceTiers: rates
      ? [
          {
            minInputTokens: 0,
            inputMicrousdPer1m: rates[0],
            cachedInputMicrousdPer1m: rates[1],
            outputMicrousdPer1m: rates[2],
          },
        ]
      : [],
    routes: [
      {
        id: `route:${slug}`,
        sourceKind: "account_pool",
        sourceId: "default",
        upstreamModel: slug,
        enabled: true,
        priority: 0,
        weight: 1,
      },
    ],
    permissionGroupIds: rates ? ["mg_default"] : [],
    createdAt: 1_770_000_000,
    updatedAt: 1_770_000_000,
  };
}

function freshModels(): JsonObject[] {
  return [
    builtinModel("gpt-5.6-sol", 1),
    builtinModel("gpt-5.6-terra", 2),
    builtinModel("gpt-5.6-luna", 3),
    builtinModel("gpt-5.5", 7),
    builtinModel("gpt-5.4", 16),
    builtinModel("gpt-5.4-mini", 23),
    builtinModel("gpt-5.2", 29),
    builtinModel("codex-auto-review", 43, "hide"),
  ];
}

function catalogResult(models: JsonObject[]) {
  return {
    items: models,
    stats: {
      total: models.length,
      enabled: models.filter((model) => model.enabled === true).length,
      builtin: models.filter((model) => model.origin === "builtin").length,
      custom: models.filter((model) => model.origin === "custom").length,
      priceMissing: models.filter(
        (model) => (model.price as JsonObject)?.priceStatus === "missing",
      ).length,
      missingRoute: models.filter(
        (model) =>
          !Array.isArray(model.routes) ||
          !model.routes.some((route) => (route as JsonObject).enabled === true),
      ).length,
    },
  };
}

function importedModel(): JsonObject {
  return {
    ...builtinModel("imported-local", 60),
    id: "custom:imported-local",
    displayName: "Imported Local",
    origin: "custom",
    builtinRevision: null,
    userEdited: true,
    price: {
      priceStatus: "custom",
      priceSource: "local-import",
      inputMicrousdPer1m: 1_000_000,
      cachedInputMicrousdPer1m: 100_000,
      outputMicrousdPer1m: 5_000_000,
    },
    permissionGroupIds: [],
  };
}

async function installMockRuntime(page: Page): Promise<MockState> {
  const state: MockState = {
    models: freshModels(),
    upserts: [],
    deletes: [],
    importCalls: [],
    initializeCalls: 0,
  };

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
    const params =
      payload?.params && typeof payload.params === "object"
        ? (payload.params as JsonObject)
        : {};
    const ok = (result: unknown) =>
      route.fulfill({
        contentType: "application/json; charset=utf-8",
        body: JSON.stringify({ jsonrpc: "2.0", id, result }),
      });

    if (method === "appSettings/get") {
      await ok(SETTINGS_SNAPSHOT);
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
    if (method === "initialize") {
      state.initializeCalls += 1;
      await ok({
        version: "0.4.0",
        userAgent: "codex_cli_rs/0.1.19",
        codexHome: "/tmp/.codex",
        platformFamily: "linux",
        platformOs: "linux",
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
    if (method === "aggregateApi/list") {
      await ok({
        items: [
          {
            id: "agg-1",
            supplierName: "Aggregate Test",
            providerType: "openai_compat",
            baseUrl: "https://aggregate.invalid/v1",
            status: "enabled",
          },
        ],
      });
      return;
    }
    if (method === "apikey/managedModelListV2") {
      await ok(catalogResult(state.models));
      return;
    }
    if (method === "apikey/managedModelGetV2") {
      const model = state.models.find((item) => item.slug === params.slug);
      await ok(model ?? null);
      return;
    }
    if (method === "apikey/managedModelUpsertV2") {
      state.upserts.push(structuredClone(params));
      const inputModel = structuredClone((params.model ?? {}) as JsonObject);
      const slug = String(inputModel.slug ?? "");
      const previousSlug = String(params.previousSlug ?? "");
      const existing = state.models.find(
        (item) => item.slug === previousSlug || item.slug === slug,
      );
      const saved = {
        ...inputModel,
        id: existing?.id ?? `custom:${slug}`,
        createdAt: existing?.createdAt ?? 1_770_000_100,
        updatedAt: 1_770_000_100,
      };
      state.models = state.models.filter(
        (item) => item.slug !== previousSlug && item.slug !== slug,
      );
      state.models.push(saved);
      await ok(saved);
      return;
    }
    if (method === "apikey/managedModelDeleteV2") {
      const slug = String(params.slug ?? "");
      state.deletes.push(slug);
      const model = state.models.find((item) => item.slug === slug);
      if (model?.origin === "builtin") {
        model.enabled = false;
        model.userEdited = true;
        model.updatedAt = 1_770_000_200;
      } else {
        state.models = state.models.filter((item) => item.slug !== slug);
      }
      await ok(null);
      return;
    }
    if (
      method === "apikey/managedModelImportPreviewV2" ||
      method === "apikey/managedModelImportCommitV2"
    ) {
      state.importCalls.push({ method, params: structuredClone(params) });
      const isCommit = method.endsWith("CommitV2");
      if (isCommit && !state.models.some((item) => item.slug === "imported-local")) {
        state.models.push(importedModel());
      }
      await ok({
        added: ["imported-local"],
        updated: [],
        conflicts: [],
        skipped: [],
        errors: [],
        ignoredFields: ["base_instructions", "unknown_field"],
        committed: isCommit ? 1 : 0,
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

  return state;
}

test("编辑器不依赖后续动画帧即可载入目标模型", async ({ page }) => {
  await installMockRuntime(page);
  await page.goto("/models/");
  await expect(
    page.getByRole("main").getByRole("heading", { name: "模型管理" }),
  ).toBeVisible();

  await page.evaluate(() => {
    const testWindow = window as typeof window & {
      __nativeRequestAnimationFrame?: typeof window.requestAnimationFrame;
    };
    testWindow.__nativeRequestAnimationFrame = window.requestAnimationFrame;
    window.requestAnimationFrame = () => 1;
  });
  await page
    .getByRole("button", { name: "编辑模型 gpt-5.4", exact: true })
    .click();
  await expect(page.getByLabel("模型标识（Slug）")).toHaveValue("gpt-5.4");
  await expect(page.getByLabel("显示名称")).toHaveValue("GPT-5.4");
  await expect(page.getByLabel("描述")).toHaveValue("gpt-5.4 builtin");
  await expect(page.getByLabel("提供方")).toHaveValue("openai");
  await expect(page.getByLabel("模型系列")).toHaveValue("gpt-5");
  await expect(page.getByLabel("模型分类")).toHaveValue("reasoning");
  await expect(page.getByLabel("标签")).toHaveValue("coding");
  await expect(page.getByLabel("标签")).toHaveAttribute(
    "placeholder",
    "例如：编程, 推理",
  );
  await expect(page.getByLabel("排序")).toHaveValue("16");
  await expect(page.getByLabel("上下文窗口", { exact: true })).toHaveValue(
    "272000",
  );
  await expect(page.getByLabel("最大上下文窗口", { exact: true })).toHaveValue(
    "1000000",
  );
  await expect(page.getByLabel("默认推理强度")).toHaveValue("medium");
  await expect(page.getByRole("combobox", { name: "可见性" })).toBeVisible();
});

test("模型目录 V2 完成本地管理、原子保存、导入和主动导出", async ({ page }) => {
  const state = await installMockRuntime(page);

  await page.goto("/models/");
  await expect(
    page.getByRole("main").getByRole("heading", { name: "模型管理" }),
  ).toBeVisible();

  const rows = page.getByRole("main").locator("tbody tr");
  await expect(rows).toHaveCount(7);
  await expect(page.locator("tr", { hasText: "gpt-5.6-sol" })).toContainText(
    "5 / 5 / 30",
  );
  await expect(page.getByText("codex-auto-review", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "远端并入" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "清理远端旧模型" })).toHaveCount(0);
  const initializeCallsBeforeExport = state.initializeCalls;

  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "导出到本地 Codex 缓存" }).click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("models_cache.json");
  const downloadPath = await download.path();
  expect(downloadPath).not.toBeNull();
  const cache = JSON.parse(await readFile(downloadPath!, "utf8"));
  expect(cache.models).toHaveLength(7);
  expect(
    cache.models.every(
      (model: JsonObject) => model.base_instructions === "",
    ),
  ).toBe(true);
  expect(state.initializeCalls).toBe(initializeCallsBeforeExport + 1);

  await page.getByRole("button", { name: "新增自定义模型" }).click();
  await page.getByLabel("模型标识（Slug）").fill("my-custom-model");
  await page.getByLabel("显示名称").fill("My Custom Model");
  await page.getByLabel("描述").fill("local managed model");

  await page.getByRole("tab", { name: "价格" }).click();
  await page.locator("#price-input").fill("2.5");
  await page.locator("#price-cached").fill("0.25");
  await page.locator("#price-output").fill("15");
  await page.locator("#price-long-threshold").fill("272000");
  await page.locator("#price-long-input").fill("5");
  await page.locator("#price-long-cached").fill("0.5");
  await page.locator("#price-long-output").fill("22.5");

  await page.getByRole("tab", { name: "路由" }).click();
  await page.getByRole("button", { name: "添加聚合路由" }).click();
  await expect(page.getByRole("combobox", { name: "来源类型" })).toHaveCount(2);
  await expect(page.getByRole("switch", { name: "启用路由" })).toHaveCount(2);
  await page.locator("#route-source-1").click();
  await page.getByRole("option", { name: "Aggregate Test" }).click();
  await page.locator("#route-model-1").fill("upstream-custom-v1");

  await page.getByRole("tab", { name: "指令策略" }).click();
  await page.getByRole("combobox", { name: "指令模式" }).click();
  await page.getByRole("option", { name: "兜底" }).click();
  await page.locator("#model-instructions-text").fill("Use the local policy.");
  await page.getByRole("button", { name: "保存模型" }).click();

  const customRow = page.locator("tr", { hasText: "my-custom-model" });
  await expect(customRow).toBeVisible();
  expect(state.upserts).toHaveLength(1);
  const atomicSave = state.upserts[0];
  expect(atomicSave.previousSlug).toBeNull();
  const savedModel = atomicSave.model as JsonObject;
  expect(savedModel.price).toEqual({
    priceStatus: "custom",
    priceSource: "local-ui",
    inputMicrousdPer1m: 2_500_000,
    cachedInputMicrousdPer1m: 250_000,
    outputMicrousdPer1m: 15_000_000,
  });
  expect(savedModel.priceTiers).toEqual([
    {
      minInputTokens: 0,
      inputMicrousdPer1m: 2_500_000,
      cachedInputMicrousdPer1m: 250_000,
      outputMicrousdPer1m: 15_000_000,
    },
    {
      minInputTokens: 272_000,
      inputMicrousdPer1m: 5_000_000,
      cachedInputMicrousdPer1m: 500_000,
      outputMicrousdPer1m: 22_500_000,
    },
  ]);
  expect(savedModel.instructionsMode).toBe("fallback");
  expect(savedModel.instructionsText).toBe("Use the local policy.");
  expect(savedModel.routes).toEqual(
    expect.arrayContaining([
      expect.objectContaining({
        sourceKind: "account_pool",
        sourceId: "default",
        upstreamModel: "my-custom-model",
      }),
      expect.objectContaining({
        sourceKind: "aggregate_api",
        sourceId: "agg-1",
        upstreamModel: "upstream-custom-v1",
      }),
    ]),
  );

  await page.getByRole("button", { name: "禁用模型 gpt-5.6-sol" }).click();
  await page.getByRole("button", { name: "删除", exact: true }).click();
  const builtinRow = page.locator("tr", { hasText: "gpt-5.6-sol" });
  await expect(builtinRow).toBeVisible();
  await expect(builtinRow).toContainText("已禁用");

  await page
    .getByRole("button", { name: "删除模型 my-custom-model" })
    .click();
  await page.getByRole("button", { name: "删除", exact: true }).click();
  await expect(customRow).toHaveCount(0);
  expect(state.deletes).toEqual(["gpt-5.6-sol", "my-custom-model"]);

  await page.getByRole("button", { name: "从本地 JSON 导入" }).click();
  const importDialog = page.getByRole("dialog");
  await importDialog.getByLabel("JSON", { exact: true }).fill(
    JSON.stringify({
      models: [
        {
          slug: "imported-local",
          display_name: "Imported Local",
          base_instructions: "must be ignored",
          unknown_field: true,
        },
      ],
    }),
  );
  await importDialog.getByRole("button", { name: "预览导入" }).click();
  await expect(importDialog.getByText("base_instructions", { exact: true })).toBeVisible();

  await importDialog.getByRole("combobox").click();
  await page.getByRole("option", { name: "replace_custom" }).click();
  await importDialog.getByRole("button", { name: "预览导入" }).click();
  await importDialog.getByRole("button", { name: "提交导入" }).click();
  await expect(page.locator("tr", { hasText: "imported-local" })).toBeVisible();

  expect(
    state.importCalls.map((call) => [call.method, call.params.conflictStrategy]),
  ).toEqual([
    ["apikey/managedModelImportPreviewV2", "keep_existing"],
    ["apikey/managedModelImportPreviewV2", "replace_custom"],
    ["apikey/managedModelImportCommitV2", "replace_custom"],
  ]);
});
