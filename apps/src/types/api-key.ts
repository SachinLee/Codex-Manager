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
  accountGroupFilter: string | null;
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

export interface AggregateApiRuntimeStatus {
  aggregateApiId: string;
  upstreamModel: string | null;
  isCoolingDown: boolean;
  consecutiveFailures: number;
  failureThreshold: number;
  cooldownUntil: number | null;
  remainingSecs: number;
  lastFailureAt: number | null;
  reason: string | null;
}

export type AggregateApiHealthState =
  | "unknown"
  | "healthy"
  | "degraded"
  | "unhealthy"
  | "cooldown"
  | "recovering";

export interface AggregateApiHealthConfig {
  aggregateApiId: string;
  enabled: boolean;
  probeIntervalSecs: number;
  probeTimeoutMs: number;
  probeModel: string | null;
  lastScheduledAt: number | null;
  nextProbeAt: number | null;
}

export interface AggregateApiHealthSummary {
  aggregateApiId: string;
  upstreamModel: string | null;
  protocol: string | null;
  state: AggregateApiHealthState;
  consecutiveFailures: number;
  failureThreshold: number;
  cooldownUntil: number | null;
  lastObservedAt: number | null;
  lastProbeAt: number | null;
  lastSuccessAt: number | null;
  lastFailureAt: number | null;
  latencyMs: number | null;
  httpStatus: number | null;
  errorCategory: string | null;
  errorReason: string | null;
  observationSource: string | null;
  activeProbeEnabled: boolean;
  probeModel: string | null;
  availableProbeModels: string[];
}

export interface AggregateApiHealthEvent {
  aggregateApiId: string;
  upstreamModel: string | null;
  protocol: string | null;
  trigger: string;
  outcome: string;
  stateBefore: string;
  stateAfter: string;
  errorCategory: string | null;
  httpStatus: number | null;
  latencyMs: number | null;
  reason: string | null;
  observedAt: number;
  cooldownUntil: number | null;
}

export interface AggregateApiHealthDetail {
  summary: AggregateApiHealthSummary;
  config: AggregateApiHealthConfig;
  states: AggregateApiHealthSummary[];
  events: AggregateApiHealthEvent[];
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

export type CapabilityRoutingMode = "off" | "observe" | "enforce";
export type GatewayCapabilityState = "supported" | "unsupported" | "unknown";
export type GatewayCapabilityOverrideState = "auto" | "supported" | "unsupported";

export interface AggregateApiCapabilityObservation {
  state: GatewayCapabilityState;
  source: "runtime" | "probe" | string;
  confidence: string;
  evidenceCode: string;
  lastObservedAt: number;
  expiresAt: number;
  occurrenceCount: number;
  upstreamModelPattern: string;
  protocol: string;
}

export interface AggregateApiEffectiveCapability {
  capabilityKey: string;
  effectiveState: GatewayCapabilityState;
  resolvedSource: string;
  confidence: string;
  expiresAt: number | null;
  scope: {
    sourceKind: string;
    sourceId: string;
    upstreamModelPattern: string;
    protocol: string;
  };
  overrideState: GatewayCapabilityOverrideState;
  observations: AggregateApiCapabilityObservation[];
}

export interface AggregateApiCapabilitiesResult {
  apiId: string;
  routingMode: CapabilityRoutingMode;
  routingModeOptions: CapabilityRoutingMode[];
  items: AggregateApiEffectiveCapability[];
}

export interface AggregateApiCapabilityAttempt {
  id: number | null;
  traceId: string;
  attemptIndex: number;
  phase: string;
  supplierName: string | null;
  upstreamModel: string | null;
  errorClass: string | null;
  errorCode: string | null;
  httpStatus: number | null;
  durationMs: number | null;
  outcome: string;
  deliveryStarted: boolean;
  createdAt: number;
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

export interface ApiKeyUsageStat {
  keyId: string;
  todayTokens: number;
  todayEstimatedCostUsd: number;
  totalTokens: number;
  estimatedCostUsd: number;
}
