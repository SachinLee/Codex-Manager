export interface RouteEvidenceSummary {
  kind: string;
  source: string;
  targetKind: string;
  targetId: string | null;
  confidence: string;
  reason: string;
  statusCode: number | null;
  retryAfterSecs: number | null;
  observedAt: number;
}

export interface GatewayPolicyActionSummary {
  id: string;
  owner: string;
  kind: string;
  targetKind: string;
  targetId: string;
  reason: string;
  createdAt: number;
  expiresAt: number;
  remainingSecs: number;
  sourceEvidence: RouteEvidenceSummary[];
}

export interface RequestLog {
  id: string;
  traceId: string;
  sessionId: string;
  conversationAnchor: string;
  keyId: string;
  accountId: string;
  initialAccountId: string;
  attemptedAccountIds: string[];
  initialAggregateApiId: string;
  attemptedAggregateApiIds: string[];
  requestPath: string;
  originalPath: string;
  adaptedPath: string;
  method: string;
  requestType: string;
  gatewayMode: string;
  routeStrategy: string;
  routeSource: string;
  routeEvidence: RouteEvidenceSummary[];
  policyActions: GatewayPolicyActionSummary[];
  path: string;
  clientModel: string;
  model: string;
  modelSource: string;
  upstreamModel: string;
  actualSourceKind: string;
  actualSourceId: string;
  clientReasoningEffort: string;
  reasoningEffort: string;
  reasoningSource: string;
  serviceTier: string;
  effectiveServiceTier: string;
  serviceTierSource: string;
  responseAdapter: string;
  canonicalSource: string;
  sizeRejectStage: string;
  upstreamUrl: string;
  aggregateApiSupplierName: string | null;
  aggregateApiUrl: string | null;
  statusCode: number | null;
  inputTokens: number | null;
  cachedInputTokens: number | null;
  cacheWriteInputTokens: number | null;
  outputTokens: number | null;
  totalTokens: number | null;
  reasoningOutputTokens: number | null;
  estimatedCostUsd: number | null;
  pricingContextBand: string;
  pricingBillingMode: string | null;
  longContextThresholdTokens: number | null;
  longContextThresholdInclusive: boolean | null;
  pricingMatchedRuleId: string | null;
  pricingMatchedPattern: string | null;
  pricingSource: string | null;
  pricingMatchQuality: string | null;
  pricingStatus: string | null;
  pricingCostSource: string | null;
  providerCostUsd: number | null;
  localEstimatedCostUsd: number | null;
  pricingVarianceUsd: number | null;
  plainInputCostUsd: number | null;
  cachedInputCostUsd: number | null;
  cacheWriteCostUsd: number | null;
  outputCostUsd: number | null;
  shortBaselineCostUsd: number | null;
  longContextUpliftUsd: number | null;
  guardEventCount: number;
  guardInternalRetryCount: number;
  guardBlockCount: number;
  guardRecoveredCount: number;
  guardRetryTotalTokens: number;
  guardRetryEstimatedCostUsd: number;
  guardLastAction: string | null;
  guardLastTargetToken: number | null;
  billableTotalTokens: number | null;
  billableEstimatedCostUsd: number | null;
  durationMs: number | null;
  firstResponseMs: number | null;
  error: string;
  createdAt: number | null;
}

export interface RequestLogListResult {
  items: RequestLog[];
  total: number;
  page: number;
  pageSize: number;
}

export interface RequestLogModelUsageStat {
  model: string;
  requestCount: number;
  successCount: number;
  errorCount: number;
  totalTokens: number;
  estimatedCostUsd: number;
  inputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  reasoningOutputTokens: number;
}

export interface RequestLogFilterSummary {
  totalCount: number;
  filteredCount: number;
  successCount: number;
  errorCount: number;
  totalTokens: number;
  totalCostUsd: number;
  guardRetryTotalTokens: number;
  guardRetryEstimatedCostUsd: number;
  longContextCount: number;
  longContextCostUsd: number;
  longContextUpliftUsd: number;
  legacyCandidateCount: number;
  modelStats: RequestLogModelUsageStat[];
  modelStatsTruncated: boolean;
}

export interface RequestLogListWithSummaryResult extends RequestLogListResult {
  summary: RequestLogFilterSummary;
}

export interface RequestLogTodaySummary {
  inputTokens: number;
  cachedInputTokens: number;
  cacheWriteInputTokens: number;
  outputTokens: number;
  reasoningOutputTokens: number;
  todayTokens: number;
  estimatedCost: number;
}
