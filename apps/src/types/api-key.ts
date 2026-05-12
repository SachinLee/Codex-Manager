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

export interface AggregateApiBalanceResult {
  id: string;
  ok: boolean;
  provider: string;
  remaining: number | null;
  used: number | null;
  total: number | null;
  unit: string | null;
  planName: string | null;
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
  billableInputTokens: number;
  outputTokens: number;
  totalTokens: number;
  reasoningOutputTokens: number;
  estimatedCostUsd: number;
  cacheHitRate: number;
}

export interface ApiKeyUsageStat {
  keyId: string;
  totalTokens: number;
  estimatedCostUsd: number;
}
