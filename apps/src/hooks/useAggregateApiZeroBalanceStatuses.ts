"use client";

import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { accountClient } from "@/lib/api/account-client";
import type { AggregateApiZeroBalanceStatus } from "@/types";

export function useAggregateApiZeroBalanceStatuses(enabled: boolean) {
  const query = useQuery({
    queryKey: ["aggregate-api-zero-balance-status"],
    queryFn: () => accountClient.listAggregateApiZeroBalanceStatuses(),
    enabled,
    staleTime: 1_000,
    refetchInterval: enabled ? 2_000 : false,
    refetchIntervalInBackground: false,
    retry: 1,
  });

  const byApiId = useMemo(
    () =>
      new Map<string, AggregateApiZeroBalanceStatus>(
        (query.data || []).map((item) => [item.aggregateApiId, item]),
      ),
    [query.data],
  );

  return { ...query, byApiId };
}
