"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { accountClient } from "@/lib/api/account-client";
import type { CapabilityRoutingMode, GatewayCapabilityOverrideState } from "@/types";

export function useAggregateApiCapabilities(apiId: string | null) {
  const queryClient = useQueryClient();
  const snapshotKey = ["aggregate-api-capabilities", apiId] as const;
  const attemptsKey = ["aggregate-api-capability-attempts", apiId] as const;
  const snapshot = useQuery({
    queryKey: snapshotKey,
    queryFn: () => accountClient.getAggregateApiCapabilities(apiId ?? ""),
    enabled: Boolean(apiId),
  });
  const attempts = useQuery({
    queryKey: attemptsKey,
    queryFn: () => accountClient.listAggregateApiCapabilityAttempts(apiId ?? "", 20),
    enabled: Boolean(apiId),
  });
  const refresh = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: snapshotKey }),
      queryClient.invalidateQueries({ queryKey: attemptsKey }),
    ]);
  };
  const setMode = useMutation({
    mutationFn: (mode: CapabilityRoutingMode) =>
      accountClient.setAggregateApiCapabilityRoutingMode(mode),
    onSuccess: refresh,
  });
  const setOverride = useMutation({
    mutationFn: (params: {
      upstreamModelPattern: string;
      protocol: string;
      capabilityKey: string;
      state: GatewayCapabilityOverrideState;
    }) => {
      if (!apiId) throw new Error("aggregate api id required");
      return accountClient.setAggregateApiCapabilityOverride({ apiId, ...params });
    },
    onSuccess: refresh,
  });
  const clearObservation = useMutation({
    mutationFn: (params: {
      upstreamModelPattern: string;
      protocol: string;
      capabilityKey: string;
    }) => {
      if (!apiId) throw new Error("aggregate api id required");
      return accountClient.clearAggregateApiCapabilityObservation({ apiId, ...params });
    },
    onSuccess: refresh,
  });
  return { snapshot, attempts, setMode, setOverride, clearObservation, refresh };
}
