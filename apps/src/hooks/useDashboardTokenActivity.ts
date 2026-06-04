"use client";

import { useQuery } from "@tanstack/react-query";
import { useDeferredDesktopActivation } from "@/hooks/useDeferredDesktopActivation";
import { useDesktopPageActive } from "@/hooks/useDesktopPageActive";
import { dashboardClient } from "@/lib/api/dashboard-client";
import { useAppStore } from "@/lib/store/useAppStore";
import type { DashboardTokenActivity } from "@/types";

export const DASHBOARD_TOKEN_ACTIVITY_QUERY_KEY = [
  "dashboard",
  "token-activity",
] as const;

export function useDashboardTokenActivity(enabled = true) {
  const serviceStatus = useAppStore((state) => state.serviceStatus);
  const isPageActive = useDesktopPageActive("/");
  const isServiceReady = serviceStatus.connected;
  const isQueryEnabled = useDeferredDesktopActivation(
    enabled && isServiceReady && isPageActive,
  );

  const query = useQuery<DashboardTokenActivity>({
    queryKey: [...DASHBOARD_TOKEN_ACTIVITY_QUERY_KEY, serviceStatus.addr],
    queryFn: () => dashboardClient.getTokenActivity(),
    enabled: isQueryEnabled,
    retry: 1,
    staleTime: 30_000,
  });

  return {
    ...query,
    isServiceReady,
  };
}
