import type { ManagedModelSourceModel } from "@/types/model";

export interface ApiKey {
  id: string;
  name: string;
  model: string;
  modelSlug: string;
  reasoningEffort: string;
  serviceTier: string;
  rotationStrategy: string;
  aggregateApiId: string | null;
  accountPlanFilter: string | null;
  aggregateApiUrl: string | null;
  quotaLimitTokens: number | null;
  protocol: string;
  clientType: string;
  authScheme: string;
  upstreamBaseUrl: string;
  staticHeadersJson: string;
  status: string;
  createdAt: number | null;
  lastUsedAt: number | null;
}

export interface ApiKeyCreateResult {
  id: string;
  key: string;
}

export interface AggregateApi {
  id: string;
  providerType: string;
  supplierName: string | null;
  sort: number;
  url: string;
  authType: string;
  authParams: Record<string, unknown> | null;
  action: string | null;
  modelOverride: string | null;
  costMultiplier: number;
  dailySpendLimitUsd: number | null;
  status: string;
  createdAt: number | null;
  updatedAt: number | null;
  lastTestAt: number | null;
  lastTestStatus: string | null;
  lastTestError: string | null;
  balanceQueryEnabled: boolean;
  balanceQueryTemplate: string | null;
  balanceQueryBaseUrl: string | null;
  balanceQueryUserId: string | null;
  balanceQueryConfigJson: string | null;
  lastBalanceAt: number | null;
  lastBalanceStatus: string | null;
  lastBalanceError: string | null;
  lastBalanceJson: string | null;
  modelSlugs: string[];
}

export interface AggregateApiCreateResult {
  id: string;
  key: string;
}

export interface AggregateApiSecretResult {
  id: string;
  key: string;
  authType: string;
  username: string | null;
  password: string | null;
}

export interface AggregateApiTestResult {
  id: string;
  ok: boolean;
  statusCode: number | null;
  message: string | null;
  testedAt: number;
  latencyMs: number;
}

export type AggregateApiCapabilityStatus =
  | "supported"
  | "unsupported"
  | "unknown"
  | "not_tested";

export interface AggregateApiCapabilityProbeResult {
  name: string;
  status: AggregateApiCapabilityStatus;
  reason: string;
  httpStatus: number | null;
  risk: string | null;
  recommendedMode: string | null;
  latencyMs: number;
}

export interface AggregateApiCapabilityDiagnosticsResult {
  id: string;
  providerType: string;
  diagnosedAt: number;
  latencyMs: number;
  nonMutating: boolean;
  liveSmoke: boolean;
  probes: AggregateApiCapabilityProbeResult[];
}

export interface AggregateApiBalanceSnapshot {
  isValid: boolean;
  invalidMessage: string | null;
  remaining: number | null;
  unit: string | null;
  planName: string | null;
  total: number | null;
  used: number | null;
  extra: Record<string, unknown> | null;
}

export interface AggregateApiBalanceRefreshResult {
  id: string;
  ok: boolean;
  balance: AggregateApiBalanceSnapshot | null;
  message: string | null;
  queriedAt: number;
  latencyMs: number;
}

export interface AggregateApiDailyUsageStat {
  aggregateApiId: string;
  aggregateApiSupplierName: string | null;
  aggregateApiUrl: string | null;
  requestCount: number;
  inputTokens: number;
  cachedInputTokens: number;
  cacheWriteInputTokens: number;
  billableInputTokens: number;
  outputTokens: number;
  totalTokens: number;
  reasoningOutputTokens: number;
  estimatedCostUsd: number;
  guardRetryTotalTokens: number;
  guardRetryEstimatedCostUsd: number;
  billableTotalTokens: number;
  billableEstimatedCostUsd: number;
  cacheHitRate: number;
}

export interface AggregateApiReasoningGuardStat {
  aggregateApiId: string;
  aggregateApiSupplierName: string | null;
  aggregateApiUrl: string | null;
  totalRequestCount: number;
  eventCount: number;
  affectedRequestCount: number;
  matchRate: number;
  internalRetryCount: number;
  internalRetryRequestCount: number;
  retryRecoveryCount: number;
  retryRecoveryRate: number;
  blockCount: number;
  blockedRequestCount: number;
  blockRate: number;
  observeOnlyCount: number;
  bypassAfterConsecutiveCount: number;
  guardInputTokens: number;
  guardCachedInputTokens: number;
  guardOutputTokens: number;
  guardTotalTokens: number;
  guardReasoningOutputTokens: number;
  guardEstimatedCostUsd: number;
  lastTargetToken: number | null;
  lastEventAt: number | null;
}

export interface AggregateApiSupplierModel {
  supplierKey: string;
  providerType: string;
  upstreamModel: string;
  displayName: string | null;
  status: string;
  createdAt: number;
  updatedAt: number;
}

export interface AggregateApiSupplierModelImportResult {
  imported: number;
  items: ManagedModelSourceModel[];
}

export interface ApiKeyUsageStat {
  keyId: string;
  todayTokens: number;
  todayEstimatedCostUsd: number;
  totalTokens: number;
  estimatedCostUsd: number;
}
