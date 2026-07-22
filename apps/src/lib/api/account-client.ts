import { invoke, withAddr } from "./transport";
import {
  normalizeAccountList,
  normalizeAccountDailyUsageStats,
  normalizeAggregateApiBalanceRefreshResult,
  normalizeAggregateApiCreateResult,
  normalizeAggregateApiCapabilityDiagnosticsResult,
  normalizeAggregateApiDailyUsageStats,
  normalizeAggregateApiReasoningGuardStats,
  normalizeAggregateApiList,
  normalizeAggregateApiSecretResult,
  normalizeAggregateApiTestResult,
  normalizeAggregateApiRuntimeStatus,
  normalizeAggregateApiRuntimeStatusList,
  normalizeApiKeyCreateResult,
  normalizeApiKeyList,
  normalizeApiKeyUsageStats,
  normalizeLoginStartResult,
  normalizeUsageAggregateSummary,
  normalizeUsageList,
  normalizeUsageSnapshot,
} from "./normalize";
import {
  normalizeAggregateApiCapabilities,
  normalizeAggregateApiCapabilityAttempts,
} from "./aggregate-capabilities";
import {
  readChatgptAuthTokensRefreshAllResult,
  readChatgptAuthTokensRefreshResult,
  readCurrentAccessTokenAccountReadResult,
  readLoginStatusResult,
} from "./account-auth";
import {
  AccountExportResult,
  AccountImportResult,
  AccountWarmupResult,
  DeleteAccountsByStatusesResult,
  DeleteUnavailableFreeResult,
  readAccountExportResult,
  readAccountImportResult,
  readAccountWarmupResult,
  readDeleteAccountsByStatusesResult,
  readApiKeySecret,
  readDeleteUnavailableFreeResult,
} from "./account-maintenance";
import { unwrapUsageSnapshotPayload } from "./usage-response";
import {
  AccountListResult,
  AccountDailyUsageStat,
  AccountUsage,
  AggregateApi,
  AggregateApiBalanceRefreshResult,
  AggregateApiCreateResult,
  AggregateApiSecretResult,
  AggregateApiTestResult,
  AggregateApiCapabilityDiagnosticsResult,
  AggregateApiCapabilitiesResult,
  AggregateApiCapabilityAttempt,
  AggregateApiDailyUsageStat,
  AggregateApiReasoningGuardStat,
  AggregateApiRuntimeStatus,
  ApiKey,
  ApiKeyCreateResult,
  ApiKeyUsageStat,
  ChatgptAuthTokensRefreshAllResult,
  ChatgptAuthTokensRefreshResult,
  CurrentAccessTokenAccountReadResult,
  LoginStatusResult,
  LoginStartResult,
  UsageAggregateSummary,
  CapabilityRoutingMode,
  GatewayCapabilityOverrideState,
} from "../../types";

export interface AccountExportPayload {
  selectedAccountIds?: string[];
  exportMode?: "single" | "multiple";
}

export interface AccountWarmupPayload {
  accountIds?: string[];
  message?: string;
}

export interface AccountDeleteByStatusesPayload {
  statuses: string[];
}

export interface AccountSortUpdatePayload {
  accountId: string;
  sort: number;
}

interface LoginStartPayload {
  loginType?: string;
  openBrowser?: boolean;
  note?: string | null;
  tags?: string[] | string | null;
  workspaceId?: string | null;
}

interface AccountUpdatePayload {
  sort?: number | null;
  preferred?: boolean | null;
  status?: string | null;
  label?: string | null;
  note?: string | null;
  tags?: string[] | string | null;
  quotaCapacityPrimaryWindowTokens?: number | null;
  quotaCapacitySecondaryWindowTokens?: number | null;
}

interface ChatgptAuthTokensLoginPayload {
  accessToken: string;
  refreshToken?: string | null;
  idToken?: string | null;
  chatgptAccountId?: string | null;
  workspaceId?: string | null;
  chatgptPlanType?: string | null;
}

interface ApiKeyPayload {
  name?: string | null;
  modelSlug?: string | null;
  reasoningEffort?: string | null;
  serviceTier?: string | null;
  protocolType?: string | null;
  upstreamBaseUrl?: string | null;
  staticHeadersJson?: string | null;
  rotationStrategy?: string | null;
  aggregateApiId?: string | null;
  accountPlanFilter?: string | null;
  quotaLimitTokens?: number | null;
  customKey?: string | null;
}

interface AggregateApiPayload {
  providerType?: string | null;
  supplierName?: string | null;
  sort?: number | null;
  status?: string | null;
  url?: string | null;
  key?: string | null;
  authType?: string | null;
  authCustomEnabled?: boolean | null;
  authParams?: Record<string, unknown> | null;
  actionCustomEnabled?: boolean | null;
  action?: string | null;
  modelOverride?: string | null;
  costMultiplier?: number | null;
  dailySpendLimitUsd?: number | null;
  clearDailySpendLimitUsd?: boolean | null;
  username?: string | null;
  password?: string | null;
  balanceQueryEnabled?: boolean | null;
  balanceQueryTemplate?: string | null;
  balanceQueryBaseUrl?: string | null;
  balanceQueryAccessToken?: string | null;
  balanceQueryUserId?: string | null;
  balanceQueryConfigJson?: string | null;
}

const MAX_IMPORT_RPC_BODY_BYTES = 4 * 1024 * 1024;
const MAX_IMPORT_ERROR_ITEMS = 50;

/**
 * 函数 `createEmptyImportResult`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * 无
 *
 * # 返回
 * 返回函数执行结果
 */
function createEmptyImportResult(): AccountImportResult {
  return {
    total: 0,
    created: 0,
    updated: 0,
    failed: 0,
    errors: [],
    importedAccountIds: [],
  };
}

/**
 * 函数 `estimateImportRequestBytes`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - contents: 参数 contents
 *
 * # 返回
 * 返回函数执行结果
 */
function estimateImportRequestBytes(contents: string[]): number {
  return new Blob([JSON.stringify({ contents })]).size;
}

/**
 * 函数 `splitImportContents`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - contents: 参数 contents
 *
 * # 返回
 * 返回函数执行结果
 */
function splitImportContents(contents: string[]): string[][] {
  const chunks: string[][] = [];
  let current: string[] = [];

  for (const content of contents) {
    const next = current.concat(content);
    if (current.length > 0 && estimateImportRequestBytes(next) > MAX_IMPORT_RPC_BODY_BYTES) {
      chunks.push(current);
      current = [content];
      if (estimateImportRequestBytes(current) > MAX_IMPORT_RPC_BODY_BYTES) {
        throw new Error("单条导入内容过大，请拆分后重试");
      }
      continue;
    }

    current = next;
  }

  if (current.length > 0) {
    chunks.push(current);
  }

  return chunks;
}

/**
 * 函数 `mergeImportResult`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - target: 参数 target
 * - source: 参数 source
 * - indexOffset: 参数 indexOffset
 *
 * # 返回
 * 返回函数执行结果
 */
function mergeImportResult(
  target: AccountImportResult,
  source: AccountImportResult,
  indexOffset: number
) {
  target.total = (target.total || 0) + (source.total || 0);
  target.created = (target.created || 0) + (source.created || 0);
  target.updated = (target.updated || 0) + (source.updated || 0);
  target.failed = (target.failed || 0) + (source.failed || 0);
  const importedAccountIds = source.importedAccountIds || [];
  if (!target.importedAccountIds) {
    target.importedAccountIds = [];
  }
  for (const accountId of importedAccountIds) {
    const normalizedAccountId = String(accountId || "").trim();
    if (
      normalizedAccountId &&
      !target.importedAccountIds.includes(normalizedAccountId)
    ) {
      target.importedAccountIds.push(normalizedAccountId);
    }
  }

  const errors = source.errors || [];
  if (!target.errors) {
    target.errors = [];
  }
  for (const error of errors) {
    if (target.errors.length >= MAX_IMPORT_ERROR_ITEMS) {
      break;
    }
    target.errors.push({
      index: (error.index || 0) + indexOffset,
      message: error.message || "",
    });
  }
}

/**
 * 函数 `importAccountContents`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - contents: 参数 contents
 *
 * # 返回
 * 返回函数执行结果
 */
async function importAccountContents(contents: string[]): Promise<AccountImportResult> {
  const batches = splitImportContents(contents);
  if (batches.length === 0) {
    return createEmptyImportResult();
  }

  const merged = createEmptyImportResult();
  let processed = 0;
  for (const batch of batches) {
    const imported = readAccountImportResult(
      await invoke<unknown>("service_account_import", withAddr({ contents: batch }))
    );
    mergeImportResult(merged, imported, processed);
    processed += batch.length;
  }

  return merged;
}

export const accountClient = {
  async list(): Promise<AccountListResult> {
    const result = await invoke<unknown>("service_account_list", withAddr());
    return normalizeAccountList(result);
  },
  delete: (accountId: string) =>
    invoke("service_account_delete", withAddr({ accountId })),
  deleteMany: (accountIds: string[]) =>
    invoke("service_account_delete_many", withAddr({ accountIds })),
  deleteUnavailableFree: async (): Promise<DeleteUnavailableFreeResult> =>
    readDeleteUnavailableFreeResult(
      await invoke<unknown>("service_account_delete_unavailable_free", withAddr())
    ),
  deleteByStatuses: async (
    params: AccountDeleteByStatusesPayload
  ): Promise<DeleteAccountsByStatusesResult> =>
    readDeleteAccountsByStatusesResult(
      await invoke<unknown>(
        "service_account_delete_by_statuses",
        withAddr({
          statuses: Array.isArray(params?.statuses) ? params.statuses : [],
        })
      )
    ),
  updateSort: (accountId: string, sort: number) =>
    invoke("service_account_update", withAddr({ accountId, sort })),
  updateSorts: (updates: AccountSortUpdatePayload[]) =>
    invoke(
      "service_account_update_sorts",
      withAddr({
        updates: updates.map((update) => ({
          accountId: update.accountId,
          sort: update.sort,
        })),
      })
    ),
  updateProfile: (accountId: string, params: AccountUpdatePayload) =>
    invoke(
      "service_account_update",
      withAddr({
        accountId,
        sort: typeof params.sort === "number" ? params.sort : null,
        preferred: typeof params.preferred === "boolean" ? params.preferred : null,
        status: params.status || null,
        label: params.label ?? null,
        note: params.note ?? null,
        tags: Array.isArray(params.tags)
          ? params.tags
              .map((item: string) => String(item || "").trim())
              .filter(Boolean)
              .join(",")
          : params.tags ?? null,
        quotaCapacityPrimaryWindowTokens:
          typeof params.quotaCapacityPrimaryWindowTokens === "number"
            ? params.quotaCapacityPrimaryWindowTokens
            : null,
        quotaCapacitySecondaryWindowTokens:
          typeof params.quotaCapacitySecondaryWindowTokens === "number"
            ? params.quotaCapacitySecondaryWindowTokens
            : null,
      })
    ),
  setPreferred: (accountId: string) =>
    invoke("service_account_update", withAddr({ accountId, preferred: true })),
  clearPreferred: (accountId: string) =>
    invoke("service_account_update", withAddr({ accountId, preferred: false })),
  disableAccount: (accountId: string) =>
    invoke("service_account_update", withAddr({ accountId, status: "disabled" })),
  enableAccount: (accountId: string) =>
    invoke("service_account_update", withAddr({ accountId, status: "active" })),
  import: importAccountContents,
  async importByDirectory(): Promise<AccountImportResult> {
    const picked = readAccountImportResult(
      await invoke<unknown>("service_account_import_by_directory", withAddr())
    );
    if (picked?.canceled || !Array.isArray(picked?.contents) || picked.contents.length === 0) {
      return picked;
    }

    const imported = await importAccountContents(picked.contents);
    return {
      ...imported,
      canceled: false,
      directoryPath: picked.directoryPath || "",
      fileCount: picked.fileCount || picked.contents.length,
    };
  },
  async importByFile(): Promise<AccountImportResult> {
    const picked = readAccountImportResult(
      await invoke<unknown>("service_account_import_by_file", withAddr())
    );
    if (picked?.canceled || !Array.isArray(picked?.contents) || picked.contents.length === 0) {
      return picked;
    }

    const imported = await importAccountContents(picked.contents);
    return {
      ...imported,
      canceled: false,
      fileCount: picked.fileCount || picked.contents.length,
    };
  },
  export: async (params?: AccountExportPayload): Promise<AccountExportResult> =>
    readAccountExportResult(await invoke<unknown>(
      "service_account_export_by_account_files",
      withAddr({
        selectedAccountIds: Array.isArray(params?.selectedAccountIds)
          ? params?.selectedAccountIds
          : [],
        exportMode: params?.exportMode || "multiple",
      })
    )),
  warmup: async (params?: AccountWarmupPayload): Promise<AccountWarmupResult> =>
    readAccountWarmupResult(
      await invoke<unknown>(
        "service_account_warmup",
        withAddr({
          accountIds: Array.isArray(params?.accountIds) ? params.accountIds : [],
          message: params?.message || "hi",
        }),
      ),
    ),

  async getUsage(accountId: string): Promise<AccountUsage | null> {
    const result = await invoke<unknown>(
      "service_usage_read",
      withAddr({ accountId, account_id: accountId })
    );
    return normalizeUsageSnapshot(unwrapUsageSnapshotPayload(result));
  },
  async getLatestUsage(): Promise<AccountUsage | null> {
    const result = await invoke<unknown>("service_usage_read", withAddr());
    return normalizeUsageSnapshot(unwrapUsageSnapshotPayload(result));
  },
  async listUsage(): Promise<AccountUsage[]> {
    const result = await invoke<unknown>("service_usage_list", withAddr());
    return normalizeUsageList(result);
  },
  refreshUsage: (accountId?: string) => {
    const targetAccountId = accountId?.trim();
    return invoke(
      "service_usage_refresh",
      withAddr(
        targetAccountId
          ? { accountId: targetAccountId, account_id: targetAccountId }
          : {}
      )
    );
  },
  async aggregateUsage(): Promise<UsageAggregateSummary> {
    const result = await invoke<unknown>("service_usage_aggregate", withAddr());
    return normalizeUsageAggregateSummary(result);
  },
  async listAccountDailyUsageStats(params?: {
    dayStartTs?: number;
    dayEndTs?: number;
  }): Promise<AccountDailyUsageStat[]> {
    const result = await invoke<unknown>(
      "service_requestlog_account_daily_usage",
      withAddr(params)
    );
    return normalizeAccountDailyUsageStats(result);
  },

  async startLogin(params: LoginStartPayload): Promise<LoginStartResult> {
    const result = await invoke<unknown>(
      "service_login_start",
      withAddr({
        loginType: params?.loginType || "chatgpt",
        openBrowser: params?.openBrowser ?? true,
        note: params?.note || null,
        tags: Array.isArray(params?.tags)
          ? params.tags
              .map((item: string) => String(item || "").trim())
              .filter(Boolean)
              .join(",")
          : params?.tags || null,
        workspaceId: params?.workspaceId || null,
      })
    );
    return normalizeLoginStartResult(result);
  },
  async getLoginStatus(loginId: string): Promise<LoginStatusResult> {
    const result = await invoke<unknown>("service_login_status", withAddr({ loginId }));
    return readLoginStatusResult(result);
  },
  completeLogin: (state: string, code: string, redirectUri: string) =>
    invoke("service_login_complete", withAddr({ state, code, redirectUri })),
  loginWithChatgptAuthTokens: (params: ChatgptAuthTokensLoginPayload) =>
    invoke("service_login_chatgpt_auth_tokens", withAddr({
      accessToken: params.accessToken,
      refreshToken: params.refreshToken || null,
      idToken: params.idToken || null,
      chatgptAccountId: params.chatgptAccountId || null,
      workspaceId: params.workspaceId || null,
      chatgptPlanType: params.chatgptPlanType || null,
    })),
  async readCurrentAccessTokenAccount(
    refreshToken = false
  ): Promise<CurrentAccessTokenAccountReadResult> {
    const result = await invoke<unknown>(
      "service_account_read",
      withAddr({ refreshToken })
    );
    return readCurrentAccessTokenAccountReadResult(result);
  },
  logoutCurrentAccessTokenAccount: () =>
    invoke("service_account_logout", withAddr()),
  async refreshChatgptAuthTokens(
    accountId?: string
  ): Promise<ChatgptAuthTokensRefreshResult> {
    const targetAccountId = accountId?.trim() || null;
    const result = await invoke<unknown>(
      "service_chatgpt_auth_tokens_refresh",
      withAddr({
        accountId: targetAccountId,
        previousAccountId: targetAccountId,
      })
    );
    return readChatgptAuthTokensRefreshResult(result);
  },
  async refreshAllChatgptAuthTokens(): Promise<ChatgptAuthTokensRefreshAllResult> {
    const result = await invoke<unknown>(
      "service_chatgpt_auth_tokens_refresh_all",
      withAddr()
    );
    return readChatgptAuthTokensRefreshAllResult(result);
  },

  async listAggregateApis(): Promise<AggregateApi[]> {
    const result = await invoke<unknown>("service_aggregate_api_list", withAddr());
    return normalizeAggregateApiList(result);
  },
  async listAggregateApiRuntimeStatuses(): Promise<AggregateApiRuntimeStatus[]> {
    const result = await invoke<unknown>(
      "service_aggregate_api_runtime_status_list",
      withAddr()
    );
    return normalizeAggregateApiRuntimeStatusList(result);
  },
  async resetAggregateApiRuntimeStatus(apiId: string): Promise<AggregateApiRuntimeStatus> {
    const result = await invoke<unknown>(
      "service_aggregate_api_runtime_status_reset",
      withAddr({ id: apiId })
    );
    const status = normalizeAggregateApiRuntimeStatus(result);
    if (!status) throw new Error("Aggregate API runtime status reset returned no result");
    return status;
  },
  async createAggregateApi(params: AggregateApiPayload): Promise<AggregateApiCreateResult> {
    const result = await invoke<unknown>(
      "service_aggregate_api_create",
      withAddr({
        providerType: params.providerType || null,
        supplierName: params.supplierName || null,
        sort: typeof params.sort === "number" ? params.sort : null,
        status: params.status || null,
        url: params.url || null,
        key: params.key || null,
        authType: params.authType || null,
        authCustomEnabled:
          typeof params.authCustomEnabled === "boolean"
            ? params.authCustomEnabled
            : null,
        authParams: params.authParams || null,
        actionCustomEnabled:
          typeof params.actionCustomEnabled === "boolean"
            ? params.actionCustomEnabled
            : null,
        action: params.action ?? null,
        modelOverride:
          typeof params.modelOverride === "string" ? params.modelOverride : null,
        costMultiplier:
          typeof params.costMultiplier === "number" ? params.costMultiplier : null,
        dailySpendLimitUsd:
          typeof params.dailySpendLimitUsd === "number"
            ? params.dailySpendLimitUsd
            : null,
        username: params.username || null,
        password: params.password || null,
        balanceQueryEnabled:
          typeof params.balanceQueryEnabled === "boolean"
            ? params.balanceQueryEnabled
            : null,
        balanceQueryTemplate: params.balanceQueryTemplate || null,
        balanceQueryBaseUrl:
          typeof params.balanceQueryBaseUrl === "string"
            ? params.balanceQueryBaseUrl
            : null,
        balanceQueryAccessToken: params.balanceQueryAccessToken || null,
        balanceQueryUserId:
          typeof params.balanceQueryUserId === "string"
            ? params.balanceQueryUserId
            : null,
        balanceQueryConfigJson:
          typeof params.balanceQueryConfigJson === "string"
            ? params.balanceQueryConfigJson
            : null,
      })
    );
    return normalizeAggregateApiCreateResult(result);
  },
  updateAggregateApi: (apiId: string, params: AggregateApiPayload) =>
    invoke(
      "service_aggregate_api_update",
      withAddr({
        id: apiId,
        providerType: params.providerType || null,
        supplierName: params.supplierName || null,
        sort: typeof params.sort === "number" ? params.sort : null,
        status: params.status || null,
        url: params.url || null,
        key: params.key || null,
        authType: params.authType || null,
        authCustomEnabled:
          typeof params.authCustomEnabled === "boolean"
            ? params.authCustomEnabled
            : null,
        authParams: params.authParams || null,
        actionCustomEnabled:
          typeof params.actionCustomEnabled === "boolean"
            ? params.actionCustomEnabled
            : null,
        action: params.action ?? null,
        modelOverride:
          typeof params.modelOverride === "string" ? params.modelOverride : null,
        costMultiplier:
          typeof params.costMultiplier === "number" ? params.costMultiplier : null,
        dailySpendLimitUsd:
          typeof params.dailySpendLimitUsd === "number"
            ? params.dailySpendLimitUsd
            : params.clearDailySpendLimitUsd
              ? null
              : undefined,
        clearDailySpendLimitUsd:
          typeof params.clearDailySpendLimitUsd === "boolean"
            ? params.clearDailySpendLimitUsd
            : undefined,
        username: params.username || null,
        password: params.password || null,
        balanceQueryEnabled:
          typeof params.balanceQueryEnabled === "boolean"
            ? params.balanceQueryEnabled
            : null,
        balanceQueryTemplate: params.balanceQueryTemplate || null,
        balanceQueryBaseUrl:
          typeof params.balanceQueryBaseUrl === "string"
            ? params.balanceQueryBaseUrl
            : null,
        balanceQueryAccessToken: params.balanceQueryAccessToken || null,
        balanceQueryUserId:
          typeof params.balanceQueryUserId === "string"
            ? params.balanceQueryUserId
            : null,
        balanceQueryConfigJson:
          typeof params.balanceQueryConfigJson === "string"
            ? params.balanceQueryConfigJson
            : null,
      })
    ),
  deleteAggregateApi: (apiId: string) =>
    invoke("service_aggregate_api_delete", withAddr({ id: apiId })),
  async readAggregateApiSecret(apiId: string): Promise<AggregateApiSecretResult> {
    const result = await invoke<unknown>(
      "service_aggregate_api_read_secret",
      withAddr({ id: apiId })
    );
    return normalizeAggregateApiSecretResult(result);
  },
  async testAggregateApiConnection(apiId: string): Promise<AggregateApiTestResult> {
    const result = await invoke<unknown>(
      "service_aggregate_api_test_connection",
      withAddr({ id: apiId })
    );
    return normalizeAggregateApiTestResult(result);
  },
  async diagnoseAggregateApiCapabilities(
    apiId: string,
    options?: { liveSmoke?: boolean }
  ): Promise<AggregateApiCapabilityDiagnosticsResult> {
    const result = await invoke<unknown>(
      "service_aggregate_api_diagnose_capabilities",
      withAddr({ id: apiId, liveSmoke: options?.liveSmoke ?? false })
    );
    return normalizeAggregateApiCapabilityDiagnosticsResult(result);
  },
  async getAggregateApiCapabilities(apiId: string): Promise<AggregateApiCapabilitiesResult> {
    const result = await invoke<unknown>(
      "service_aggregate_api_capabilities_get",
      withAddr({ id: apiId })
    );
    return normalizeAggregateApiCapabilities(result);
  },
  async setAggregateApiCapabilityOverride(params: {
    apiId: string;
    upstreamModelPattern: string;
    protocol: string;
    capabilityKey: string;
    state: GatewayCapabilityOverrideState;
  }): Promise<AggregateApiCapabilitiesResult> {
    const result = await invoke<unknown>(
      "service_aggregate_api_capabilities_set_override",
      withAddr({
        id: params.apiId,
        upstreamModelPattern: params.upstreamModelPattern,
        protocol: params.protocol,
        capabilityKey: params.capabilityKey,
        state: params.state,
      })
    );
    return normalizeAggregateApiCapabilities(result);
  },
  async resetAggregateApiCapabilityOverride(params: {
    apiId: string;
    upstreamModelPattern: string;
    protocol: string;
    capabilityKey: string;
  }): Promise<AggregateApiCapabilitiesResult> {
    const result = await invoke<unknown>(
      "service_aggregate_api_capabilities_reset_override",
      withAddr({ id: params.apiId, ...params })
    );
    return normalizeAggregateApiCapabilities(result);
  },
  async clearAggregateApiCapabilityObservation(params: {
    apiId: string;
    upstreamModelPattern: string;
    protocol: string;
    capabilityKey: string;
  }): Promise<AggregateApiCapabilitiesResult> {
    const result = await invoke<unknown>(
      "service_aggregate_api_capabilities_clear_observation",
      withAddr({ id: params.apiId, ...params })
    );
    return normalizeAggregateApiCapabilities(result);
  },
  async listAggregateApiCapabilityAttempts(
    apiId: string,
    limit = 20
  ): Promise<AggregateApiCapabilityAttempt[]> {
    const result = await invoke<unknown>(
      "service_aggregate_api_capabilities_list_recent_attempts",
      withAddr({ id: apiId, limit })
    );
    return normalizeAggregateApiCapabilityAttempts(result);
  },
  async setAggregateApiCapabilityRoutingMode(
    routingMode: CapabilityRoutingMode
  ): Promise<CapabilityRoutingMode> {
    const result = await invoke<{ routingMode?: unknown }>(
      "service_aggregate_api_capabilities_set_mode",
      withAddr({ routingMode })
    );
    return result.routingMode === "off" || result.routingMode === "observe"
      ? result.routingMode
      : "enforce";
  },
  async listAggregateApiDailyUsageStats(params?: {
    dayStartTs?: number;
    dayEndTs?: number;
  }): Promise<AggregateApiDailyUsageStat[]> {
    const result = await invoke<unknown>(
      "service_requestlog_aggregate_api_daily_usage",
      withAddr(params)
    );
    return normalizeAggregateApiDailyUsageStats(result);
  },
  async listAggregateApiReasoningGuardStats(params?: {
    dayStartTs?: number;
    dayEndTs?: number;
  }): Promise<AggregateApiReasoningGuardStat[]> {
    const result = await invoke<unknown>(
      "service_requestlog_aggregate_api_reasoning_guard",
      withAddr(params)
    );
    return normalizeAggregateApiReasoningGuardStats(result);
  },
  async refreshAggregateApiBalance(apiId: string): Promise<AggregateApiBalanceRefreshResult> {
    const result = await invoke<unknown>(
      "service_aggregate_api_refresh_balance",
      withAddr({ id: apiId })
    );
    return normalizeAggregateApiBalanceRefreshResult(result);
  },
  async listApiKeys(): Promise<ApiKey[]> {
    const result = await invoke<unknown>("service_apikey_list", withAddr());
    return normalizeApiKeyList(result);
  },
  async createApiKey(params: ApiKeyPayload): Promise<ApiKeyCreateResult> {
    const result = await invoke<unknown>(
      "service_apikey_create",
      withAddr({
        name: params.name || null,
        modelSlug: params.modelSlug || null,
        reasoningEffort: params.reasoningEffort || null,
        serviceTier: params.serviceTier || null,
        protocolType: params.protocolType || null,
        upstreamBaseUrl: params.upstreamBaseUrl || null,
        staticHeadersJson: params.staticHeadersJson || null,
        rotationStrategy: params.rotationStrategy || null,
        aggregateApiId: params.aggregateApiId || null,
        accountPlanFilter: params.accountPlanFilter || null,
        quotaLimitTokens: params.quotaLimitTokens ?? null,
        customKey: params.customKey || null,
      })
    );
    return normalizeApiKeyCreateResult(result);
  },
  async listApiKeyUsageStats(): Promise<ApiKeyUsageStat[]> {
    const result = await invoke<unknown>("service_apikey_usage_stats", withAddr());
    return normalizeApiKeyUsageStats(result);
  },
  deleteApiKey: (keyId: string) =>
    invoke("service_apikey_delete", withAddr({ keyId })),
  updateApiKey: (keyId: string, params: ApiKeyPayload) => {
    const payload: Record<string, unknown> = {
      keyId,
      name: params.name || null,
      modelSlug: params.modelSlug || null,
      reasoningEffort: params.reasoningEffort || null,
      serviceTier: params.serviceTier || null,
      protocolType: params.protocolType || null,
      upstreamBaseUrl: params.upstreamBaseUrl || null,
      staticHeadersJson: params.staticHeadersJson || null,
      rotationStrategy: params.rotationStrategy || null,
      aggregateApiId: params.aggregateApiId || null,
      accountPlanFilter: params.accountPlanFilter || null,
    };
    if ("quotaLimitTokens" in params) {
      payload.quotaLimitTokens = params.quotaLimitTokens ?? null;
    }
    return invoke("service_apikey_update_model", withAddr(payload));
  },
  disableApiKey: (keyId: string) =>
    invoke("service_apikey_disable", withAddr({ keyId })),
  enableApiKey: (keyId: string) =>
    invoke("service_apikey_enable", withAddr({ keyId })),
  async readApiKeySecret(keyId: string): Promise<string> {
    const result = await invoke<unknown>(
      "service_apikey_read_secret",
      withAddr({ keyId })
    );
    return readApiKeySecret(result);
  },
};
