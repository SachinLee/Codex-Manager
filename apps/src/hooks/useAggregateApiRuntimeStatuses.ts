"use client";

import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { accountClient } from "@/lib/api/account-client";
import type { AggregateApiRuntimeStatus } from "@/types";

export function useAggregateApiRuntimeStatuses(enabled: boolean) {
  const [nowSeconds, setNowSeconds] = useState(() => Math.floor(Date.now() / 1000));
  const query = useQuery({
    queryKey: ["aggregate-api-runtime-status"],
    queryFn: () => accountClient.listAggregateApiRuntimeStatuses(),
    enabled,
    staleTime: 1_000,
    refetchInterval: enabled ? 2_000 : false,
    refetchIntervalInBackground: false,
    retry: 1,
  });

  useEffect(() => {
    if (!enabled) return;
    const timer = window.setInterval(() => setNowSeconds(Math.floor(Date.now() / 1000)), 1_000);
    return () => window.clearInterval(timer);
  }, [enabled]);

  const byApiId = useMemo(
    () => new Map<string, AggregateApiRuntimeStatus>(
      (query.data || []).map((item) => [item.aggregateApiId, item]),
    ),
    [query.data],
  );

  return { ...query, byApiId, nowSeconds };
}
