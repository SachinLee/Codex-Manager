"use client";

import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useI18n } from "@/lib/i18n/provider";
import { formatTsFromSeconds } from "@/lib/utils/usage";
import type { AggregateApiCapabilityDiagnosticsResult } from "@/types/api-key";

import { CapabilityRoutingPanel } from "./capability-routing-panel";

type CapabilityDiagnosticsDialogProps = {
  result: AggregateApiCapabilityDiagnosticsResult | null;
  onOpenChange: (open: boolean) => void;
};

function probeBadgeVariant(status: string) {
  if (status === "supported") return "default" as const;
  if (status === "unsupported") return "destructive" as const;
  return "secondary" as const;
}

export function CapabilityDiagnosticsDialog({
  result,
  onOpenChange,
}: CapabilityDiagnosticsDialogProps) {
  const { t } = useI18n();

  return (
    <Dialog open={Boolean(result)} onOpenChange={onOpenChange}>
      {result ? (
        <DialogContent className="max-h-[82vh] max-w-3xl overflow-hidden p-0">
          <DialogHeader className="border-b px-6 py-5">
            <DialogTitle>{t("能力诊断")}</DialogTitle>
            <DialogDescription>
              {result.providerType} · {formatTsFromSeconds(result.diagnosedAt, "-")}
            </DialogDescription>
          </DialogHeader>
          <div className="max-h-[calc(82vh-92px)] overflow-y-auto px-6 py-5">
            <div className="grid gap-3 sm:grid-cols-3">
              <Metric label={t("模式")} value={result.nonMutating ? t("非变更诊断") : t("Live Smoke")} />
              <Metric label={t("耗时")} value={`${result.latencyMs}ms`} />
              <Metric label={t("探针")} value={String(result.probes.length)} />
            </div>
            <div className="mt-4 space-y-2">
              {result.probes.map((probe) => (
                <div key={probe.name} className="rounded-lg border bg-card/60 p-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="min-w-0">
                      <p className="font-mono text-sm font-medium">{probe.name}</p>
                      <p className="mt-1 text-xs text-muted-foreground">{probe.reason}</p>
                    </div>
                    <Badge variant={probeBadgeVariant(probe.status)}>{probe.status}</Badge>
                  </div>
                  <div className="mt-2 flex flex-wrap gap-2 text-xs text-muted-foreground">
                    {probe.httpStatus != null ? <span>HTTP {probe.httpStatus}</span> : null}
                    {probe.recommendedMode ? <span>{probe.recommendedMode}</span> : null}
                    <span>{probe.latencyMs}ms</span>
                  </div>
                  {probe.risk ? (
                    <p className="mt-2 text-xs text-amber-600 dark:text-amber-300">{probe.risk}</p>
                  ) : null}
                </div>
              ))}
            </div>
            <CapabilityRoutingPanel apiId={result.id} />
          </div>
        </DialogContent>
      ) : null}
    </Dialog>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border bg-muted/20 p-3">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="mt-1 text-sm font-medium">{value}</p>
    </div>
  );
}
