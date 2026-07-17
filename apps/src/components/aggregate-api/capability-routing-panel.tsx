"use client";

import { RefreshCw, RotateCcw } from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useAggregateApiCapabilities } from "@/hooks/useAggregateApiCapabilities";
import { useI18n } from "@/lib/i18n/provider";
import { getAppErrorMessage } from "@/lib/api/transport";
import { formatTsFromSeconds } from "@/lib/utils/usage";
import type { CapabilityRoutingMode, GatewayCapabilityOverrideState } from "@/types";

export function CapabilityRoutingPanel({ apiId }: { apiId: string }) {
  const { t } = useI18n();
  const { snapshot, attempts, setMode, setOverride, clearObservation, refresh } =
    useAggregateApiCapabilities(apiId);
  const data = snapshot.data;
  const item = data?.items[0];
  const onError = (error: unknown) => toast.error(getAppErrorMessage(error));

  return (
    <section className="mt-5 space-y-3 border-t pt-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold">{t("能力感知路由")}</h3>
          <p className="mt-1 text-xs text-muted-foreground">
            {t("按供应商能力保留原生请求，必要时仅执行安全降级。")}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Select
            value={data?.routingMode ?? "enforce"}
            onValueChange={(value) =>
              setMode.mutate((value ?? "enforce") as CapabilityRoutingMode, { onError })
            }
          >
            <SelectTrigger className="w-32"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="off">off</SelectItem>
              <SelectItem value="observe">observe</SelectItem>
              <SelectItem value="enforce">enforce</SelectItem>
            </SelectContent>
          </Select>
          <Button variant="outline" size="icon" onClick={() => void refresh()}>
            <RefreshCw className="h-4 w-4" />
          </Button>
        </div>
      </div>

      {snapshot.isLoading ? (
        <div className="rounded-lg border p-4 text-sm text-muted-foreground">{t("加载中")}</div>
      ) : item ? (
        <div className="rounded-lg border bg-muted/10 p-4">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <p className="break-all text-sm font-medium">{item.capabilityKey}</p>
              <div className="mt-2 flex flex-wrap gap-2">
                <Badge variant={item.effectiveState === "unsupported" ? "destructive" : "secondary"}>
                  {item.effectiveState}
                </Badge>
                <Badge variant="outline">{item.resolvedSource}</Badge>
                <Badge variant="outline">{item.confidence}</Badge>
              </div>
              <p className="mt-2 text-xs text-muted-foreground">
                {item.scope.upstreamModelPattern} · {item.scope.protocol}
                {item.expiresAt ? ` · ${formatTsFromSeconds(item.expiresAt, "-")}` : ""}
              </p>
            </div>
            <div className="flex items-center gap-2">
              <Select
                value={item.overrideState}
                onValueChange={(value) =>
                  setOverride.mutate(
                    {
                      upstreamModelPattern: item.scope.upstreamModelPattern,
                      protocol: item.scope.protocol,
                      capabilityKey: item.capabilityKey,
                      state: (value ?? "auto") as GatewayCapabilityOverrideState,
                    },
                    { onError },
                  )
                }
              >
                <SelectTrigger className="w-36"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="auto">auto</SelectItem>
                  <SelectItem value="supported">supported</SelectItem>
                  <SelectItem value="unsupported">unsupported</SelectItem>
                </SelectContent>
              </Select>
              <Button
                variant="outline"
                size="sm"
                disabled={item.observations.length === 0 || clearObservation.isPending}
                onClick={() =>
                  clearObservation.mutate(
                    {
                      upstreamModelPattern: item.scope.upstreamModelPattern,
                      protocol: item.scope.protocol,
                      capabilityKey: item.capabilityKey,
                    },
                    { onError },
                  )
                }
              >
                <RotateCcw className="mr-2 h-4 w-4" />{t("清除学习记录")}
              </Button>
            </div>
          </div>
          {item.observations.map((observation) => (
            <div key={`${observation.evidenceCode}-${observation.upstreamModelPattern}`} className="mt-3 rounded border px-3 py-2 text-xs">
              <span className="font-medium">{observation.evidenceCode}</span>
              <span className="ml-2 text-muted-foreground">
                {observation.state} · {observation.occurrenceCount} · {formatTsFromSeconds(observation.expiresAt, "-")}
              </span>
            </div>
          ))}
        </div>
      ) : null}

      <div className="space-y-2">
        <p className="text-xs font-medium text-muted-foreground">{t("最近路由尝试")}</p>
        {(attempts.data ?? []).slice(0, 8).map((attempt) => (
          <div key={attempt.id ?? `${attempt.traceId}-${attempt.attemptIndex}`} className="flex flex-wrap items-center justify-between gap-2 rounded border px-3 py-2 text-xs">
            <span>{attempt.phase} · {attempt.outcome} · {attempt.upstreamModel ?? "-"}</span>
            <span className="text-muted-foreground">{attempt.errorCode ?? attempt.httpStatus ?? "ok"}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
