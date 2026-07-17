import type {
  AggregateApiCapabilitiesResult,
  AggregateApiCapabilityAttempt,
  CapabilityRoutingMode,
  GatewayCapabilityOverrideState,
  GatewayCapabilityState,
} from "@/types";

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function text(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function integer(value: unknown, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.trunc(value)
    : fallback;
}

function nullableInteger(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? Math.trunc(value) : null;
}

function routingMode(value: unknown): CapabilityRoutingMode {
  return value === "off" || value === "observe" ? value : "enforce";
}

function capabilityState(value: unknown): GatewayCapabilityState {
  return value === "supported" || value === "unsupported" ? value : "unknown";
}

function overrideState(value: unknown): GatewayCapabilityOverrideState {
  return value === "supported" || value === "unsupported" ? value : "auto";
}

export function normalizeAggregateApiCapabilities(
  payload: unknown,
): AggregateApiCapabilitiesResult {
  const source = record(payload);
  const items = Array.isArray(source.items) ? source.items : [];
  return {
    apiId: text(source.apiId),
    routingMode: routingMode(source.routingMode),
    routingModeOptions: Array.isArray(source.routingModeOptions)
      ? source.routingModeOptions.map(routingMode)
      : ["off", "observe", "enforce"],
    items: items.map((raw) => {
      const item = record(raw);
      const scope = record(item.scope);
      const observations = Array.isArray(item.observations) ? item.observations : [];
      return {
        capabilityKey: text(item.capabilityKey),
        effectiveState: capabilityState(item.effectiveState),
        resolvedSource: text(item.resolvedSource, "unknown"),
        confidence: text(item.confidence, "low"),
        expiresAt: nullableInteger(item.expiresAt),
        scope: {
          sourceKind: text(scope.sourceKind),
          sourceId: text(scope.sourceId),
          upstreamModelPattern: text(scope.upstreamModelPattern, "*"),
          protocol: text(scope.protocol, "responses"),
        },
        overrideState: overrideState(item.overrideState),
        observations: observations.map((rawObservation) => {
          const observation = record(rawObservation);
          return {
            state: capabilityState(observation.state),
            source: text(observation.source),
            confidence: text(observation.confidence, "low"),
            evidenceCode: text(observation.evidenceCode),
            lastObservedAt: integer(observation.lastObservedAt),
            expiresAt: integer(observation.expiresAt),
            occurrenceCount: integer(observation.occurrenceCount, 1),
            upstreamModelPattern: text(observation.upstreamModelPattern, "*"),
            protocol: text(observation.protocol, "responses"),
          };
        }),
      };
    }),
  };
}

export function normalizeAggregateApiCapabilityAttempts(
  payload: unknown,
): AggregateApiCapabilityAttempt[] {
  const source = record(payload);
  const items = Array.isArray(source.items) ? source.items : [];
  return items.map((raw) => {
    const item = record(raw);
    return {
      id: nullableInteger(item.id),
      traceId: text(item.traceId),
      attemptIndex: integer(item.attemptIndex),
      phase: text(item.phase, "native"),
      supplierName: text(item.supplierName) || null,
      upstreamModel: text(item.upstreamModel) || null,
      errorClass: text(item.errorClass) || null,
      errorCode: text(item.errorCode) || null,
      httpStatus: nullableInteger(item.httpStatus),
      durationMs: nullableInteger(item.durationMs),
      outcome: text(item.outcome),
      deliveryStarted: item.deliveryStarted === true,
      createdAt: integer(item.createdAt),
    };
  });
}
