"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Cable, Inbox, RefreshCw } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/skeleton";
import { getAppErrorMessage } from "@/lib/api/transport-errors";
import { accountClient } from "@/lib/api/account-client";
import { useI18n } from "@/lib/i18n/provider";
import type {
  AggregateApi,
  AggregateApiModelDiscoveryItem,
  AggregateApiModelDiscoveryResult,
} from "@/types";

function maskUrl(url: string): string {
  const candidates = [url.trim(), `https://${url.trim()}`];
  for (const candidate of candidates) {
    try {
      const parsed = new URL(candidate);
      parsed.username = "";
      parsed.password = "";
      parsed.search = "";
      parsed.hash = "";
      return parsed.toString().replace(/\/$/, "");
    } catch {
      // Fall through to the next candidate.
    }
  }
  return url.trim().replace(/[?#].*$/, "").replace(/\/+$/, "");
}

function formatDiscoveredAt(ts: number, t: (message: string) => string): string {
  if (!ts) return t("未知时间");
  return new Date(ts * 1000).toLocaleString();
}

interface AggregateApiModelDiscoveryDialogProps {
  apis: AggregateApi[];
  isActive: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAddModel: (api: AggregateApi, item: AggregateApiModelDiscoveryItem) => void;
}

export function AggregateApiModelDiscoveryDialog({
  apis,
  isActive,
  open,
  onOpenChange,
  onAddModel,
}: AggregateApiModelDiscoveryDialogProps) {
  const { t } = useI18n();
  const [discoveryByApiId, setDiscoveryByApiId] = useState<
    Record<string, AggregateApiModelDiscoveryResult>
  >({});
  const [loadingApiIds, setLoadingApiIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [isFetchingAll, setIsFetchingAll] = useState(false);
  const apiIds = useMemo(() => new Set(apis.map((api) => api.id)), [apis]);
  const activeRef = useRef(isActive);
  const apiIdsRef = useRef(apiIds);
  activeRef.current = isActive;
  apiIdsRef.current = apiIds;

  useEffect(() => {
    if (isActive) return;
    const frameId = window.requestAnimationFrame(() => {
      setDiscoveryByApiId({});
      setLoadingApiIds(new Set());
      setIsFetchingAll(false);
    });
    return () => window.cancelAnimationFrame(frameId);
  }, [isActive]);

  useEffect(() => {
    const frameId = window.requestAnimationFrame(() => {
      setDiscoveryByApiId((current) => {
        const staleIds = Object.keys(current).filter((id) => !apiIds.has(id));
        if (staleIds.length === 0) return current;
        const next = { ...current };
        for (const id of staleIds) delete next[id];
        return next;
      });
      setLoadingApiIds((current) => {
        const next = new Set(current);
        for (const id of next) {
          if (!apiIds.has(id)) next.delete(id);
        }
        return next;
      });
    });
    return () => window.cancelAnimationFrame(frameId);
  }, [apiIds]);

  const recordFailure = useCallback(
    (apiId: string, error: unknown): AggregateApiModelDiscoveryResult => ({
      apiId,
      ok: false,
      items: [],
      statusCode: 0,
      discoveredAt: Math.floor(Date.now() / 1000),
      message: getAppErrorMessage(error),
    }),
    [],
  );

  const discoverOne = useCallback(
    async (api: AggregateApi) => {
      setLoadingApiIds((current) => {
        const next = new Set(current);
        next.add(api.id);
        return next;
      });
      try {
        const result = await accountClient.discoverAggregateApiModels(api.id);
        if (!activeRef.current || !apiIdsRef.current.has(api.id)) return;
        setDiscoveryByApiId((current) => ({
          ...current,
          [api.id]: result,
        }));
      } catch (error) {
        if (!activeRef.current || !apiIdsRef.current.has(api.id)) return;
        setDiscoveryByApiId((current) => ({
          ...current,
          [api.id]: recordFailure(api.id, error),
        }));
      } finally {
        if (activeRef.current) {
          setLoadingApiIds((current) => {
            const next = new Set(current);
            next.delete(api.id);
            return next;
          });
        }
      }
    },
    [recordFailure],
  );

  const discoverAll = useCallback(
    async (targets: AggregateApi[]) => {
      setIsFetchingAll(true);
      const settled = await Promise.allSettled(
        targets.map((api) => accountClient.discoverAggregateApiModels(api.id)),
      );
      const next: Record<string, AggregateApiModelDiscoveryResult> = {};
      settled.forEach((outcome, index) => {
        const api = targets[index];
        if (!activeRef.current || !apiIdsRef.current.has(api.id)) return;
        if (outcome.status === "fulfilled") {
          next[api.id] = outcome.value;
        } else {
          next[api.id] = recordFailure(api.id, outcome.reason);
        }
      });
      if (activeRef.current) {
        setDiscoveryByApiId((current) => ({ ...current, ...next }));
        setIsFetchingAll(false);
      }
    },
    [recordFailure],
  );

  const hasActiveRequest =
    isFetchingAll || (isActive && loadingApiIds.size > 0);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[82vh] max-w-4xl overflow-hidden p-0">
        <DialogHeader className="border-b px-6 py-5">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
            <div>
              <DialogTitle className="flex items-center gap-2">
                <Cable className="h-4 w-4 text-primary" />
                {t("上游 API 模型")}
              </DialogTitle>
              <DialogDescription className="mt-1">
                {t(
                  "只读查询每个已保存聚合 API 的上游模型目录；结果仅保存在当前页面，不会写入模型目录或路由。",
                )}
              </DialogDescription>
            </div>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={!isActive || apis.length === 0 || hasActiveRequest}
              onClick={() => void discoverAll(apis)}
            >
              <RefreshCw
                className={`mr-1.5 h-4 w-4 ${isFetchingAll ? "animate-spin" : ""}`}
              />
              {t("获取全部 API 模型")}
            </Button>
          </div>
        </DialogHeader>
        <div className="max-h-[calc(82vh-132px)] overflow-y-auto p-4">
          {apis.length === 0 ? (
            <Empty className="min-h-32">
              <EmptyHeader>
                <EmptyTitle>{t("暂无已保存的聚合 API。")}</EmptyTitle>
              </EmptyHeader>
            </Empty>
          ) : (
            <div className="space-y-3">
              {apis.map((api) => {
                const result = discoveryByApiId[api.id];
                const isLoading = loadingApiIds.has(api.id);
                return (
                  <div
                    key={api.id}
                    className="rounded-lg border border-border/60 bg-background/40 p-3"
                  >
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-sm font-semibold">
                        {api.supplierName || api.providerType}
                      </span>
                      <Badge variant="outline">{api.providerType}</Badge>
                      <Badge variant="secondary">{api.id}</Badge>
                      <span
                        className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground"
                        title={maskUrl(api.url)}
                      >
                        {maskUrl(api.url)}
                      </span>
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        disabled={!isActive || isLoading || isFetchingAll}
                        onClick={() => void discoverOne(api)}
                      >
                        <RefreshCw
                          className={`mr-1.5 h-4 w-4 ${isLoading ? "animate-spin" : ""}`}
                        />
                        {t("获取模型")}
                      </Button>
                    </div>
                    <DiscoveryResultRow
                      api={api}
                      result={result}
                      isLoading={isLoading || isFetchingAll}
                      onAddModel={onAddModel}
                      t={t}
                    />
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function DiscoveryResultRow({
  api,
  result,
  isLoading,
  onAddModel,
  t,
}: {
  api: AggregateApi;
  result: AggregateApiModelDiscoveryResult | undefined;
  isLoading: boolean;
  onAddModel: (api: AggregateApi, item: AggregateApiModelDiscoveryItem) => void;
  t: (message: string, values?: Record<string, string | number>) => string;
}) {
  if (isLoading && !result) {
    return (
      <div className="mt-2 space-y-1.5">
        <Skeleton className="h-4 w-40" />
        <Skeleton className="h-4 w-64" />
      </div>
    );
  }
  if (!result) {
    return (
      <p className="mt-2 text-xs text-muted-foreground">
        {t("尚未查询此 API 的模型目录。")}
      </p>
    );
  }
  if (!result.ok) {
    return (
      <div className="mt-2 space-y-1">
        <p className="text-xs text-destructive">
          {t("查询失败")}
          {result.statusCode ? `（HTTP ${result.statusCode}）` : ""}
        </p>
        {result.message ? (
          <p className="break-words text-xs text-muted-foreground">
            {result.message}
          </p>
        ) : null}
      </div>
    );
  }
  if (result.items.length === 0) {
    return (
      <div className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
        <Inbox className="h-3.5 w-3.5" />
        {t("该 API 返回了空的模型目录。")}
        {result.message ? <span>{result.message}</span> : null}
      </div>
    );
  }
  return (
    <div className="mt-2">
      <p className="text-xs text-muted-foreground">
        {t("发现 {count} 个模型", { count: result.items.length })}
        <span className="ml-2">{formatDiscoveredAt(result.discoveredAt, t)}</span>
      </p>
      <div className="mt-2 flex flex-wrap gap-1.5">
        {result.items.map((item) => (
          <div
            key={`${result.apiId}:${item.id}`}
            className="flex max-w-full items-center gap-1 rounded-md border border-border/60 bg-background/40 p-1"
          >
            <Badge variant="outline" className="max-w-full truncate">
              {item.displayName ? `${item.displayName}（${item.id}）` : item.id}
            </Badge>
            <Button
              type="button"
              size="xs"
              variant="ghost"
              disabled={isLoading}
              onClick={() => onAddModel(api, item)}
            >
              {t("添加到模型与路由")}
            </Button>
          </div>
        ))}
      </div>
    </div>
  );
}
