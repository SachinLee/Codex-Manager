import { invoke, withAddr } from "./transport";
import {
  buildApiKeyUpdateInvokePayload,
  type ApiKeyUpdatePayload,
} from "./api-key-update-payload";
import {
  normalizeAccountList,
  normalizeAccountDailyUsageStats,
  normalizeAggregateApiBalanceRefreshResult,
  normalizeAggregateApiCreateResult,
  normalizeAggregateApiCapabilityDiagnosticsResult,
  normalizeAggregateApiDailyUsageStats,
  normalizeModelDailyUsageStats,
  normalizeAggregateApiReasoningGuardStats,
  normalizeAggregateApiList,
  normalizeAggregateApiModelDiscoveryResult,
  normalizeAggregateApiSecretResult,
  normalizeAggregateApiTestResult,
  normalizeAggregateApiRuntimeStatus,
  normalizeAggregateApiRuntimeStatusList,
  normalizeAggregateApiHealthDetail,
  normalizeAggregateApiZeroBalanceStatus,
  normalizeAggregateApiZeroBalanceStatusList,
  normalizeAggregateApiHealthList,
  normalizeAggregateApiProbeCostList,
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
  normalizeAccountProxyUrlTestListResult,
  normalizeProxyDiagnosticTestListResult,
  normalizeProxySpeedTestListResult,
  normalizeProxyTestJobState,
} from "./proxy-normalize";
import {
  type AccountProxySource,
  readAccountProxySettings,
  type AccountProxySettings,
  type AccountProxySetPayload,
  type AccountProxyTestPayload,
} from "./account-proxy-settings";
export type {
  AccountProxySettings,
  AccountProxySetPayload,
  AccountProxySource,
  AccountProxyTestPayload,
};
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
  AggregateApiModelDiscoveryResult,
  AggregateApiTestResult,
  AggregateApiCapabilityDiagnosticsResult,
  AggregateApiCapabilitiesResult,
  AggregateApiCapabilityAttempt,
  AggregateApiDailyUsageStat,
  ModelDailyUsageStat,
  AggregateApiReasoningGuardStat,
  AggregateApiRuntimeStatus,
  AggregateApiZeroBalanceStatus,
  AggregateApiHealthConfig,
  AggregateApiHealthDetail,
  AggregateApiHealthSummary,
  AggregateApiProbeCostSummary,
  ApiKey,
  ApiKeyCreateResult,
  ApiKeyUsageStat,
  ChatgptAuthTokensRefreshAllResult,
  ChatgptAuthTokensRefreshResult,
  CurrentAccessTokenAccountReadResult,
  LoginStatusResult,
  LoginStartResult,
  LoginType,
  AccountProxyUrlTestListResult,
  ProxyDiagnosticTestListResult,
  ProxySpeedTestListResult,
  ProxyTestJobState,
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

export interface AccountProxyLatencyTestPayload {
  accountId: string;
}

export interface CfStyleConfig {
  downloadPreset?: "all" | "100kb" | "1mb" | "10mb" | "25mb" | null;
  uploadPreset?:
    | "all"
    | "100kb"
    | "1mb"
    | "10mb"
    | "25mb"
    | "50mb"
    | null;
  timeoutSecs?: number;
  runUpload?: boolean | null;
}

export interface AccountProxyCloudflareSpeedTestPayload {
  accountId: string;
  config?: CfStyleConfig | null;
}

export interface AccountProxySpeedTestPayload {
  accountId: string;
  providerId?: string | null;
  fileSizeId?: string | null;
  diagnosticProviderId?: string | null;
  diagnosticFileSizeId?: string | null;
}

export interface AccountDeleteByStatusesPayload {
  statuses: string[];
}

export interface AccountSortUpdatePayload {
  accountId: string;
  sort: number;
}

export interface AccountUsageRefreshResult {
  ok: boolean;
  source: string;
  accountId: string | null;
  processed: number;
  total: number;
  message: string | null;
}

export interface LoginStartPayload {
  loginType: LoginType;
  openBrowser?: boolean;
  note?: string | null;
  tags?: string[] | string | null;
  groupName?: string | null;
  workspaceId?: string | null;
}

interface AccountUpdatePayload {
  sort?: number | null;
  preferred?: boolean | null;
  status?: string | null;
  label?: string | null;
  groupName?: string | null;
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

interface ApiKeyPayload extends ApiKeyUpdatePayload {
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
  enableConsecutiveFailureFreeze?: boolean | null;
  upstreamProtocol?: string | null;
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

function normalizeUsageRefreshResult(payload: unknown): AccountUsageRefreshResult {
  const source =
    payload && typeof payload === "object" && !Array.isArray(payload)
      ? (payload as Record<string, unknown>)
      : {};
  const toInteger = (value: unknown, fallback = 0) => {
    const parsed =
      typeof value === "number"
        ? value
        : typeof value === "string"
          ? Number.parseInt(value, 10)
          : Number.NaN;
    return Number.isFinite(parsed) ? Math.max(0, Math.trunc(parsed)) : fallback;
  };
  const toStringOrNull = (value: unknown) =>
    typeof value === "string" && value.trim() ? value.trim() : null;
  return {
    ok: source.ok === true,
    source: toStringOrNull(source.source) || "manual",
    accountId: toStringOrNull(source.accountId ?? source.account_id),
    processed: toInteger(source.processed),
    total: toInteger(source.total),
    message: toStringOrNull(source.message),
  };
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
  if (source.usageRefreshAccountIds !== undefined) {
    if (!target.usageRefreshAccountIds) {
      target.usageRefreshAccountIds = [];
    }
    for (const accountId of source.usageRefreshAccountIds) {
      const normalizedAccountId = String(accountId || "").trim();
      if (
        normalizedAccountId &&
        !target.usageRefreshAccountIds.includes(normalizedAccountId)
      ) {
        target.usageRefreshAccountIds.push(normalizedAccountId);
      }
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
  updateProfile: (accountId: string, params: AccountUpdatePayload) => {
    const payload: Record<string, unknown> = {
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
    };
    if (params.groupName !== undefined) {
      payload.groupName = params.groupName ?? "";
    }
    return invoke("service_account_update", withAddr(payload));
  },
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

  getProxySettings: async (
    accountId: string,
  ): Promise<AccountProxySettings> =>
    readAccountProxySettings(
      await invoke<unknown>(
        "service_account_proxy_get",
        withAddr({ accountId }),
      ),
    ),
  setProxySettings: async (
    params: AccountProxySetPayload,
  ): Promise<AccountProxySettings> =>
    readAccountProxySettings(
      await invoke<unknown>(
        "service_account_proxy_set",
        withAddr({
          accountId: params.accountId,
          enabled: params.enabled,
          source: params.source ?? null,
          proxyProfileId: params.proxyProfileId ?? null,
          proxyUrl: params.proxyUrl ?? null,
          status: params.status ?? null,
          latencyMs: params.latencyMs ?? null,
          lastError: params.lastError ?? null,
          ip: params.ip ?? null,
          countryCode: params.countryCode ?? null,
          countryName: params.countryName ?? null,
          regionName: params.regionName ?? null,
          cityName: params.cityName ?? null,
          geoCheckedAt: params.geoCheckedAt ?? null,
          geoError: params.geoError ?? null,
        }),
      ),
    ),
  clearProxySettings: async (
    accountId: string,
  ): Promise<AccountProxySettings> =>
    readAccountProxySettings(
      await invoke<unknown>(
        "service_account_proxy_clear",
        withAddr({ accountId }),
      ),
    ),
  testProxySettings: async (
    params: AccountProxyTestPayload,
  ): Promise<AccountProxySettings> =>
    readAccountProxySettings(
      await invoke<unknown>(
        "service_account_proxy_test",
        withAddr({
          accountId: params.accountId,
          enabled: params.enabled,
          source: params.source ?? null,
          proxyProfileId: params.proxyProfileId ?? null,
          proxyUrl: params.proxyUrl ?? null,
        }),
      ),
    ),
  latencyTestProxy: async (
    params: AccountProxyLatencyTestPayload,
  ): Promise<ProxyTestJobState> =>
    normalizeProxyTestJobState(
      await invoke<unknown>(
        "service_account_proxy_latency_test",
        withAddr({ accountId: params.accountId }),
      ),
    ),
  speedTestProxy: async (
    params: AccountProxySpeedTestPayload,
  ): Promise<ProxyTestJobState> =>
    normalizeProxyTestJobState(
      await invoke<unknown>(
        "service_account_proxy_speed_test",
        withAddr({
          accountId: params.accountId,
          providerId: params.providerId ?? null,
          fileSizeId: params.fileSizeId ?? null,
          diagnosticProviderId: params.diagnosticProviderId ?? null,
          diagnosticFileSizeId: params.diagnosticFileSizeId ?? null,
        }),
      ),
    ),
  cloudflareSpeedTestProxy: async (
    params: AccountProxyCloudflareSpeedTestPayload,
  ): Promise<ProxyTestJobState> =>
    normalizeProxyTestJobState(
      await invoke<unknown>(
        "service_account_proxy_cloudflare_speed_test",
        withAddr({
          accountId: params.accountId,
          config: params.config ?? null,
        }),
      ),
    ),
  getProxyTestJob: async (
    accountId: string,
    jobId: string,
  ): Promise<ProxyTestJobState> =>
    normalizeProxyTestJobState(
      await invoke<unknown>(
        "service_account_proxy_test_job",
        withAddr({ accountId, jobId }),
      ),
    ),
  cancelProxyTestJob: async (
    accountId: string,
    jobId: string,
  ): Promise<void> => {
    await invoke<unknown>(
      "service_account_proxy_cancel_test",
      withAddr({ accountId, jobId }),
    );
  },
  getAccountProxySpeedHistory: async (
    accountId: string,
    limit?: number,
  ): Promise<ProxySpeedTestListResult> =>
    normalizeProxySpeedTestListResult(
      await invoke<unknown>(
        "service_account_proxy_speed_test_history",
        withAddr({ accountId, limit: limit ?? null }),
      ),
    ),
  getAccountProxyLatencyHistory: async (
    accountId: string,
    limit?: number,
  ): Promise<AccountProxyUrlTestListResult> =>
    normalizeAccountProxyUrlTestListResult(
      await invoke<unknown>(
        "service_account_proxy_latency_test_history",
        withAddr({ accountId, limit: limit ?? null }),
      ),
    ),
  getAccountProxyDiagnosticsHistory: async (
    accountId: string,
    limit?: number,
  ): Promise<ProxyDiagnosticTestListResult> =>
    normalizeProxyDiagnosticTestListResult(
      await invoke<unknown>(
        "service_account_proxy_diagnostics_history",
        withAddr({ accountId, limit: limit ?? null }),
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
  async refreshUsage(accountId?: string): Promise<AccountUsageRefreshResult> {
    const targetAccountId = accountId?.trim();
    const result = await invoke<unknown>(
      "service_usage_refresh",
      withAddr(
        targetAccountId
          ? { accountId: targetAccountId, account_id: targetAccountId }
          : {}
      )
    );
    return normalizeUsageRefreshResult(result);
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
        groupName: params?.groupName || null,
        workspaceId: params?.workspaceId || null,
      })
    );
    return normalizeLoginStartResult(result);
  },
  async getLoginStatus(loginId: string): Promise<LoginStatusResult> {
    const result = await invoke<unknown>("service_login_status", withAddr({ loginId }));
    return readLoginStatusResult(result);
  },
  async cancelLogin(loginId: string): Promise<void> {
    await invoke<unknown>("service_login_cancel", withAddr({ loginId }));
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
  async listAggregateApiZeroBalanceStatuses(): Promise<AggregateApiZeroBalanceStatus[]> {
    const result = await invoke<unknown>(
      "service_aggregate_api_zero_balance_status_list",
      withAddr()
    );
    return normalizeAggregateApiZeroBalanceStatusList(result);
  },
  async resetAggregateApiZeroBalanceStatus(apiId: string): Promise<AggregateApiZeroBalanceStatus> {
    const result = await invoke<unknown>(
      "service_aggregate_api_zero_balance_status_reset",
      withAddr({ id: apiId })
    );
    const status = normalizeAggregateApiZeroBalanceStatus(result);
    if (!status) throw new Error("Aggregate API zero-balance status reset returned no result");
    return status;
  },
  async listAggregateApiHealth(): Promise<AggregateApiHealthSummary[]> {
    const result = await invoke<unknown>("service_aggregate_api_health_list", withAddr());
    return normalizeAggregateApiHealthList(result);
  },
  async listAggregateApiProbeCosts(startTs: number, endTs: number): Promise<AggregateApiProbeCostSummary[]> {
    const result = await invoke<unknown>("service_aggregate_api_health_costs", withAddr({ startTs, endTs }));
    return normalizeAggregateApiProbeCostList(result);
  },
  async getAggregateApiHealth(apiId: string): Promise<AggregateApiHealthDetail> {
    const result = await invoke<unknown>("service_aggregate_api_health_get", withAddr({ id: apiId, limit: 50 }));
    return normalizeAggregateApiHealthDetail(result);
  },
  async updateAggregateApiHealthConfig(apiId: string, config: Pick<AggregateApiHealthConfig, "enabled" | "probeIntervalSecs" | "probeTimeoutMs" | "probeModel">): Promise<AggregateApiHealthConfig> {
    const result = await invoke<unknown>("service_aggregate_api_health_config_update", withAddr({ id: apiId, enabled: config.enabled, intervalSecs: config.probeIntervalSecs, timeoutMs: config.probeTimeoutMs, probeModel: config.probeModel }));
    return normalizeAggregateApiHealthDetail({ config: result }).config;
  },
  async probeAggregateApiHealth(apiId: string, model?: string | null): Promise<AggregateApiTestResult> {
    const result = await invoke<unknown>("service_aggregate_api_health_probe", withAddr({ id: apiId, model: model || null }));
    return normalizeAggregateApiTestResult(result);
  },
  async resetAggregateApiHealth(apiId: string): Promise<AggregateApiHealthSummary> {
    const result = await invoke<unknown>("service_aggregate_api_health_reset", withAddr({ id: apiId }));
    return normalizeAggregateApiHealthList({ items: [result] })[0];
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
        enableConsecutiveFailureFreeze:
          typeof params.enableConsecutiveFailureFreeze === "boolean"
            ? params.enableConsecutiveFailureFreeze
            : null,
        upstreamProtocol:
          typeof params.upstreamProtocol === "string"
            ? params.upstreamProtocol
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
        enableConsecutiveFailureFreeze:
          typeof params.enableConsecutiveFailureFreeze === "boolean"
            ? params.enableConsecutiveFailureFreeze
            : null,
        upstreamProtocol:
          typeof params.upstreamProtocol === "string"
            ? params.upstreamProtocol
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
  async discoverAggregateApiModels(apiId: string): Promise<AggregateApiModelDiscoveryResult> {
    const result = await invoke<unknown>(
      "service_aggregate_api_models_discover",
      withAddr({ id: apiId })
    );
    return normalizeAggregateApiModelDiscoveryResult(result);
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
  async listModelDailyUsageStats(params?: {
    dayStartTs?: number;
    dayEndTs?: number;
  }): Promise<ModelDailyUsageStat[]> {
    const result = await invoke<unknown>(
      "service_requestlog_model_daily_usage",
      withAddr(params)
    );
    return normalizeModelDailyUsageStats(result);
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
        accountGroupFilter: params.accountGroupFilter || null,
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
  updateApiKey: (keyId: string, params: ApiKeyPayload) =>
    invoke(
      "service_apikey_update_model",
      withAddr(buildApiKeyUpdateInvokePayload(keyId, params)),
    ),
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
