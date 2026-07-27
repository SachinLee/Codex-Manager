"use client";

import { Database, Shield, Zap } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useI18n } from "@/lib/i18n/provider";
import {
  formatCompactKeyLabel,
  formatReasoningGuardTarget,
  formatSessionIdForTable,
  formatModelEffortDisplay,
  isReasoningGuardConverted502,
  normalizeAggregateApiUrl,
  normalizeRequestType,
  RequestTypeBadge,
  resolveAccountDisplayNameById,
  resolveAggregateApiDisplayName,
  resolveAggregateApiDisplayNameById,
  resolveAggregateApiTooltipUrl,
  resolveDisplayRequestPath,
  resolveDisplayServiceTier,
  resolveFriendlyRequestPathLabel,
  resolveUpstreamDisplay,
  ServiceTierBadge,
} from "./page-helpers";
import type { CodexSession } from "@/lib/api/codex-launcher";
import type {
  AggregateApi,
  ApiKey,
  RequestLog,
  RequestLogFilterSummary,
} from "@/types";

export function SessionInfoCell({
  sessionId,
  conversationAnchor,
  session,
}: {
  sessionId: string;
  conversationAnchor: string;
  session?: CodexSession;
}) {
  const { t } = useI18n();
  const normalizedSessionId = String(sessionId || "").trim();
  const normalizedConversationAnchor = String(conversationAnchor || "").trim();
  if (!normalizedSessionId && !normalizedConversationAnchor) {
    return <span className="text-muted-foreground">-</span>;
  }

  const title = String(session?.title || "").trim();
  const cwd = String(session?.cwd || "").trim();
  const displayTitle = normalizedSessionId
    ? title || (session ? t("无标题会话") : t("未匹配会话"))
    : t("未记录会话 ID");
  const shortSessionId = normalizedSessionId
    ? formatSessionIdForTable(normalizedSessionId)
    : "-";

  return (
    <Tooltip>
      <TooltipTrigger render={<div />} className="block min-w-0 text-left">
        <div className="flex min-w-0 max-w-[220px] flex-col gap-0.5">
          <div
            className={
              title
                ? "min-w-0 truncate text-[11px] font-medium text-foreground"
                : "min-w-0 truncate text-[11px] font-medium text-muted-foreground italic"
            }
          >
            {displayTitle}
          </div>
          <div className="min-w-0 truncate font-mono text-[9px] text-muted-foreground">
            session: {shortSessionId}
          </div>
        </div>
      </TooltipTrigger>
      <TooltipContent className="max-w-md">
        <div className="flex min-w-[260px] flex-col gap-2">
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("会话标题")}</div>
            <div className="break-all text-[11px]">{displayTitle}</div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("会话 ID")}</div>
            <div className="break-all font-mono text-[11px]">
              {normalizedSessionId || "-"}
            </div>
          </div>
          {normalizedConversationAnchor ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("路由锚点")}</div>
              <div className="break-all font-mono text-[11px]">
                {normalizedConversationAnchor}
              </div>
            </div>
          ) : null}
          {cwd ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("工作目录")}</div>
              <div className="break-all font-mono text-[11px]">{cwd}</div>
            </div>
          ) : null}
          {normalizedSessionId && !session ? (
            <div className="text-[10px] text-background/70">
              {t("未在本地 Codex 会话列表中找到匹配标题")}
            </div>
          ) : null}
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

export function AccountKeyInfoCell({
  log,
  accountLabel,
  accountNameMap,
  apiKeyMap,
  aggregateApiMap,
}: {
  log: RequestLog;
  accountLabel: string;
  accountNameMap: Map<string, string>;
  apiKeyMap: Map<string, ApiKey>;
  aggregateApiMap: Map<string, AggregateApi>;
}) {
  const { t } = useI18n();
  const displayAccount = accountLabel || log.accountId || "-";
  const hasNamedAccount =
    Boolean(accountLabel) &&
    accountLabel.trim() !== "" &&
    accountLabel !== log.accountId;
  const attemptedAccountLabels = log.attemptedAccountIds
    .map((accountId) => resolveAccountDisplayNameById(accountId, accountNameMap))
    .filter((value) => value.trim().length > 0);
  const initialAccountLabel = resolveAccountDisplayNameById(
    log.initialAccountId,
    accountNameMap,
  );
  const attemptedAggregateApiLabels = log.attemptedAggregateApiIds
    .map((aggregateApiId) =>
      resolveAggregateApiDisplayNameById(aggregateApiId, aggregateApiMap),
    )
    .filter((value) => value.trim().length > 0);
  const initialAggregateApiLabel = resolveAggregateApiDisplayNameById(
    log.initialAggregateApiId,
    aggregateApiMap,
  );
  const apiKey = apiKeyMap.get(log.keyId) || null;
  const apiKeyName = String(apiKey?.name || "").trim();
  const apiKeyDisplayName = apiKeyName || formatCompactKeyLabel(log.keyId);
  const aggregateApiById = apiKey?.aggregateApiId
    ? aggregateApiMap.get(apiKey.aggregateApiId) || null
    : null;
  const actualAggregateApi =
    log.actualSourceKind === "aggregate_api" && log.actualSourceId
      ? aggregateApiMap.get(log.actualSourceId) || null
      : null;
  const aggregateApiByUrl = (() => {
    const upstreamUrl = normalizeAggregateApiUrl(log.upstreamUrl);
    if (!upstreamUrl) return null;
    for (const aggregateApi of aggregateApiMap.values()) {
      if (normalizeAggregateApiUrl(aggregateApi.url) === upstreamUrl) {
        return aggregateApi;
      }
    }
    return null;
  })();
  const aggregateApi = actualAggregateApi || aggregateApiById || aggregateApiByUrl;
  const selectedAggregateApiId =
    log.actualSourceKind === "aggregate_api" && log.actualSourceId
      ? log.actualSourceId
      : aggregateApi?.id || "";
  const isAggregateApi = Boolean(
    log.actualSourceKind === "aggregate_api" ||
      log.aggregateApiSupplierName ||
      log.aggregateApiUrl ||
      aggregateApi,
  );
  const aggregateApiDisplayName = resolveAggregateApiDisplayName(log, aggregateApi, apiKey);
  const aggregateApiDisplayUrl = resolveAggregateApiTooltipUrl(log, aggregateApi, apiKey);
  const showAttemptHint =
    attemptedAccountLabels.length > 1 &&
    initialAccountLabel &&
    initialAccountLabel !== displayAccount;
  const showAggregateAttemptHint =
    attemptedAggregateApiLabels.length > 1 &&
    initialAggregateApiLabel &&
    String(log.initialAggregateApiId || "").trim() !== selectedAggregateApiId;

  if (isAggregateApi) {
    return (
      <Tooltip>
        <TooltipTrigger render={<div />} className="block text-left">
          <div className="flex max-w-[180px] flex-col gap-0.5 opacity-80">
            <div className="flex items-center gap-1">
              <Database className="h-3 w-3 text-primary" />
              <span className="truncate text-[11px] font-medium">
                {aggregateApiDisplayName}
              </span>
            </div>
            <div className="truncate font-mono text-[10px] leading-4 text-muted-foreground">
              {aggregateApiDisplayUrl}
            </div>
            <div className="flex items-center gap-1 text-[10px] leading-4 text-muted-foreground">
              <Shield className="h-3 w-3" />
              <span className={apiKeyName ? "truncate" : "font-mono"}>
                {apiKeyDisplayName}
              </span>
            </div>
            {showAggregateAttemptHint ? (
              <div className="text-[10px] leading-4 text-amber-500">
                {t("先试")} {initialAggregateApiLabel}
              </div>
            ) : null}
          </div>
        </TooltipTrigger>
        <TooltipContent className="max-w-sm">
          <div className="flex min-w-[240px] flex-col gap-2">
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("供应商名称")}</div>
              <div className="break-all font-mono text-[11px]">
                {aggregateApiDisplayName}
              </div>
            </div>
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">URL</div>
              <div className="break-all font-mono text-[11px]">
                {aggregateApiDisplayUrl}
              </div>
            </div>
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("密钥")}</div>
              <div className="break-all text-[11px]">{apiKeyDisplayName || "-"}</div>
            </div>
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("密钥 ID")}</div>
              <div className="break-all font-mono text-[11px]">{log.keyId || "-"}</div>
            </div>
            {attemptedAggregateApiLabels.length > 1 ? (
              <div className="space-y-0.5">
                <div className="text-[10px] text-background/70">{t("尝试链路")}</div>
                <div className="break-all font-mono text-[11px]">
                  {attemptedAggregateApiLabels.join(" -> ")}
                </div>
              </div>
            ) : null}
            {initialAggregateApiLabel ? (
              <div className="space-y-0.5">
                <div className="text-[10px] text-background/70">{t("首尝试渠道")}</div>
                <div className="break-all font-mono text-[11px]">
                  {initialAggregateApiLabel}
                </div>
              </div>
            ) : null}
          </div>
        </TooltipContent>
      </Tooltip>
    );
  }

  return (
    <Tooltip>
      <TooltipTrigger render={<div />} className="block text-left">
        <div className="flex flex-col gap-0.5 opacity-80">
          <div className="flex items-center gap-1">
            <Zap className="h-3 w-3 text-yellow-500" />
            <span className="max-w-[140px] truncate">{displayAccount}</span>
          </div>
          <div className="flex items-center gap-1 text-[10px] leading-4 text-muted-foreground">
            <Shield className="h-3 w-3" />
            <span className={apiKeyName ? "max-w-[140px] truncate" : "font-mono"}>
              {apiKeyDisplayName}
            </span>
          </div>
          {showAttemptHint ? (
            <div className="text-[10px] leading-4 text-amber-500">
              {t("先试")} {initialAccountLabel}
            </div>
          ) : null}
        </div>
      </TooltipTrigger>
      <TooltipContent className="max-w-sm">
        <div className="flex min-w-[240px] flex-col gap-2">
          {initialAccountLabel ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("首尝试账号")}</div>
              <div className="break-all font-mono text-[11px]">{initialAccountLabel}</div>
            </div>
          ) : null}
          {attemptedAccountLabels.length > 1 ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("尝试链路")}</div>
              <div className="break-all font-mono text-[11px]">
                {attemptedAccountLabels.join(" -> ")}
              </div>
            </div>
          ) : null}
          {hasNamedAccount ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("邮箱 / 名称")}</div>
              <div className="break-all font-mono text-[11px]">{accountLabel}</div>
            </div>
          ) : null}
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("账号 ID")}</div>
            <div className="break-all font-mono text-[11px]">{log.accountId || "-"}</div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("密钥")}</div>
            <div className="break-all text-[11px]">{apiKeyDisplayName || "-"}</div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("密钥 ID")}</div>
            <div className="break-all font-mono text-[11px]">{log.keyId || "-"}</div>
          </div>
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

export function RequestRouteInfoCell({ log }: { log: RequestLog }) {
  const { t } = useI18n();
  const displayPath = resolveDisplayRequestPath(log) || "-";
  const displayPathLabel = resolveFriendlyRequestPathLabel(displayPath, t) || "-";
  const recordedPath = String(log.path || log.requestPath || "").trim();
  const originalPath = String(log.originalPath || "").trim();
  const adaptedPath = String(log.adaptedPath || "").trim();
  const gatewayMode = String(log.gatewayMode || "").trim().toLowerCase();
  const isCompactGatewayMode = gatewayMode === "compact";
  const upstreamUrl = String(log.upstreamUrl || "").trim();
  const upstreamDisplay = resolveUpstreamDisplay(upstreamUrl, t);
  const forwardedPath = adaptedPath && adaptedPath !== displayPath ? adaptedPath : "";
  const friendlyDisplayPath =
    isCompactGatewayMode
      ? t("上下文压缩")
      : displayPathLabel && displayPathLabel !== displayPath
        ? displayPathLabel
        : "";
  const requestType = normalizeRequestType(log.requestType);
  const canonicalSource = String(log.canonicalSource || "native_codex").trim();
  const sizeRejectStage = String(log.sizeRejectStage || "-").trim();
  const routeStrategy = String(log.routeStrategy || "").trim();
  const routeSource = String(log.routeSource || "").trim();

  return (
    <Tooltip>
      <TooltipTrigger render={<div />} className="block text-left">
        <div className="flex flex-col gap-0.5">
          <div className="flex items-center gap-1.5">
            <RequestTypeBadge requestType={requestType} />
            {isCompactGatewayMode ? (
              <Badge className="h-5 rounded-full border-amber-500/20 bg-amber-500/10 px-1.5 text-[10px] font-medium text-amber-500">
                {t("压缩")}
              </Badge>
            ) : null}
            <span className="font-bold text-primary">{log.method || "-"}</span>
          </div>
          <span className="max-w-[220px] truncate font-mono text-[11px] text-foreground">
            {displayPath}
          </span>
          {friendlyDisplayPath ? (
            <span className="max-w-[220px] truncate text-[10px] text-muted-foreground">
              {friendlyDisplayPath}
            </span>
          ) : null}
          {forwardedPath ? (
            <span className="max-w-[220px] truncate font-mono text-[10px] text-amber-500">
              -&gt; {forwardedPath}
            </span>
          ) : null}
          {upstreamDisplay ? (
            <span className="max-w-[220px] truncate font-mono text-[10px] text-cyan-500">
              =&gt; {upstreamDisplay}
            </span>
          ) : null}
        </div>
      </TooltipTrigger>
      <TooltipContent className="max-w-md">
        <div className="flex min-w-[280px] flex-col gap-2">
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("请求类型")}</div>
            <div className="font-mono text-[11px] uppercase">{requestType}</div>
          </div>
          {gatewayMode ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("网关模式")}</div>
              <div className="font-mono text-[11px]">{gatewayMode}</div>
            </div>
          ) : null}
          {routeStrategy ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("路由策略")}</div>
              <div className="font-mono text-[11px]">{routeStrategy}</div>
            </div>
          ) : null}
          {routeSource ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("路由来源")}</div>
              <div className="font-mono text-[11px]">{routeSource}</div>
            </div>
          ) : null}
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("规范来源")}</div>
            <div className="font-mono text-[11px]">{canonicalSource}</div>
          </div>
          {sizeRejectStage && sizeRejectStage !== "-" ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("大小拒绝阶段")}</div>
              <div className="font-mono text-[11px]">{sizeRejectStage}</div>
            </div>
          ) : null}
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("方法")}</div>
            <div className="font-mono text-[11px]">{log.method || "-"}</div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("显示名称")}</div>
            <div className="break-all text-[11px]">{displayPathLabel}</div>
          </div>
          {displayPath && displayPathLabel !== displayPath ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("原始路径")}</div>
              <div className="break-all font-mono text-[11px]">{displayPath}</div>
            </div>
          ) : null}
          {recordedPath && recordedPath !== displayPath ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("记录地址")}</div>
              <div className="break-all font-mono text-[11px]">{recordedPath}</div>
            </div>
          ) : null}
          {originalPath && originalPath !== displayPath ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("原始地址")}</div>
              <div className="break-all font-mono text-[11px]">{originalPath}</div>
            </div>
          ) : null}
          {forwardedPath ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("转发路径")}</div>
              <div className="break-all font-mono text-[11px]">{forwardedPath}</div>
            </div>
          ) : null}
          {log.responseAdapter ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("适配器")}</div>
              <div className="break-all font-mono text-[11px]">{log.responseAdapter}</div>
            </div>
          ) : null}
          {upstreamDisplay ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("上游")}</div>
              <div className="break-all font-mono text-[11px]">{upstreamDisplay}</div>
            </div>
          ) : null}
          {upstreamUrl ? (
            <div className="space-y-0.5">
              <div className="text-[10px] text-background/70">{t("上游地址")}</div>
              <div className="break-all font-mono text-[11px]">{upstreamUrl}</div>
            </div>
          ) : null}
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

export function ErrorInfoCell({ log }: { log: RequestLog }) {
  const { t } = useI18n();
  const isReasoningGuard = isReasoningGuardConverted502(log);
  const error = log.error;
  const text = String(error || "").trim();
  const guardTarget = formatReasoningGuardTarget(log);
  const hasGuardRetry = log.guardInternalRetryCount > 0;
  const hasGuardEvent = log.guardEventCount > 0;
  if (!text && !isReasoningGuard && !hasGuardEvent) {
    return <span className="text-muted-foreground">-</span>;
  }
  const displayText = isReasoningGuard ? text || guardTarget : text || guardTarget;

  return (
    <Tooltip>
      <TooltipTrigger render={<div />} className="block text-left">
        {isReasoningGuard ? (
          <Badge className="mb-1 h-5 rounded-full border-amber-500/20 bg-amber-500/10 px-1.5 text-[10px] font-medium text-amber-500">
            {guardTarget} -&gt; 502
          </Badge>
        ) : hasGuardRetry ? (
          <Badge className="mb-1 h-5 rounded-full border-emerald-500/20 bg-emerald-500/10 px-1.5 text-[10px] font-medium text-emerald-500">
            Guard retry {log.guardInternalRetryCount}
          </Badge>
        ) : hasGuardEvent ? (
          <Badge className="mb-1 h-5 rounded-full border-amber-500/20 bg-amber-500/10 px-1.5 text-[10px] font-medium text-amber-500">
            Guard {log.guardLastAction || log.guardEventCount}
          </Badge>
        ) : null}
        <span className="block max-w-[220px] truncate font-medium text-red-400">
          {displayText}
        </span>
      </TooltipTrigger>
      <TooltipContent className="max-w-md">
        <div className="flex max-w-[360px] flex-col gap-2">
          {isReasoningGuard ? (
            <div className="text-[11px] text-background/80">
              {t("这是 Reasoning Guard 被网关保护转换成的 502，不是真实上游 502。")}
            </div>
          ) : null}
          {hasGuardEvent ? (
            <div className="grid gap-1 text-[11px] text-background/80">
              <div>{t("Guard 事件")}: {log.guardEventCount}</div>
              <div>{t("内部重试")}: {log.guardInternalRetryCount}</div>
              <div>{t("阻断")}: {log.guardBlockCount}</div>
              <div>{t("恢复")}: {log.guardRecoveredCount}</div>
              <div>{t("最近动作")}: {log.guardLastAction || "-"}</div>
              <div>{t("最近 token")}: {log.guardLastTargetToken || "-"}</div>
            </div>
          ) : null}
          <div className="break-all font-mono text-[11px]">{displayText}</div>
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

export function ModelEffortCell({ log }: { log: RequestLog }) {
  const { t } = useI18n();
  const clientModel = String(log.clientModel || "").trim() || "-";
  const model = String(log.model || "").trim();
  const modelSource = String(log.modelSource || "").trim() || "-";
  const upstreamModel = String(log.upstreamModel || "").trim();
  const actualSourceKind = String(log.actualSourceKind || "").trim();
  const actualSourceId = String(log.actualSourceId || "").trim();
  const clientReasoningEffort = String(log.clientReasoningEffort || "").trim() || "-";
  const effort = String(log.reasoningEffort || "").trim();
  const reasoningSource = String(log.reasoningSource || "").trim() || "-";
  const clientServiceTier = resolveDisplayServiceTier(log.serviceTier);
  const effectiveServiceTier = resolveDisplayServiceTier(
    log.effectiveServiceTier || log.serviceTier,
  );
  const serviceTierSource = String(log.serviceTierSource || "").trim() || "-";
  const badgeServiceTier =
    effectiveServiceTier !== "auto" ? effectiveServiceTier : clientServiceTier;
  const display = formatModelEffortDisplay(log);
  const forwardedModel = upstreamModel && upstreamModel !== model ? upstreamModel : "";

  return (
    <Tooltip>
      <TooltipTrigger render={<div />} className="block text-left">
        <div className="flex flex-col gap-1">
          <span className="block max-w-[200px] truncate font-medium text-foreground">
            {display}
          </span>
          {forwardedModel ? (
            <span className="block max-w-[200px] truncate font-mono text-[10px] text-amber-500">
              {t("转发")} {forwardedModel}
            </span>
          ) : null}
          <ServiceTierBadge serviceTier={badgeServiceTier} />
        </div>
      </TooltipTrigger>
      <TooltipContent className="max-w-sm">
        <div className="flex min-w-[220px] flex-col gap-2">
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("客户端模型")}</div>
            <div className="break-all font-mono text-[11px]">{clientModel}</div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("最终平台模型")}</div>
            <div className="break-all font-mono text-[11px]">{model || "-"}</div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("模型来源")}</div>
            <div className="break-all font-mono text-[11px]">{modelSource}</div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("上游模型")}</div>
            <div className="break-all font-mono text-[11px]">{upstreamModel || "-"}</div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("实际来源")}</div>
            <div className="break-all font-mono text-[11px]">
              {actualSourceKind && actualSourceId
                ? `${actualSourceKind}:${actualSourceId}`
                : actualSourceKind || actualSourceId || "-"}
            </div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("客户端推理")}</div>
            <div className="break-all font-mono text-[11px]">{clientReasoningEffort}</div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("最终推理")}</div>
            <div className="break-all font-mono text-[11px]">{effort || "-"}</div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("推理来源")}</div>
            <div className="break-all font-mono text-[11px]">{reasoningSource}</div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("客户端显式服务等级")}</div>
            <div className="break-all font-mono text-[11px]">{clientServiceTier}</div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("最终生效服务等级")}</div>
            <div className="break-all font-mono text-[11px]">{effectiveServiceTier}</div>
          </div>
          <div className="space-y-0.5">
            <div className="text-[10px] text-background/70">{t("服务等级来源")}</div>
            <div className="break-all font-mono text-[11px]">{serviceTierSource}</div>
          </div>
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

export function buildSummaryPlaceholder(
  logs: RequestLog[],
): RequestLogFilterSummary {
  const successCount = logs.filter((item) => {
    const statusCode = item.statusCode ?? 0;
    return statusCode >= 200 && statusCode < 300 && !String(item.error || "").trim();
  }).length;
  const errorCount = logs.filter((item) => {
    const statusCode = item.statusCode;
    return Boolean(String(item.error || "").trim()) || (statusCode != null && statusCode >= 400);
  }).length;
  const guardRetryTotalTokens = logs.reduce(
    (sum, item) => sum + Math.max(0, item.guardRetryTotalTokens || 0),
    0,
  );
  const guardRetryEstimatedCostUsd = logs.reduce(
    (sum, item) => sum + Math.max(0, item.guardRetryEstimatedCostUsd || 0),
    0,
  );
  const totalTokens = logs.reduce(
    (sum, item) =>
      sum +
      Math.max(
        0,
        item.billableTotalTokens ??
          (item.totalTokens || 0) + Math.max(0, item.guardRetryTotalTokens || 0),
      ),
    0,
  );
  const totalCostUsd = logs.reduce(
    (sum, item) =>
      sum +
      Math.max(
        0,
        item.billableEstimatedCostUsd ??
          (item.estimatedCostUsd || 0) +
            Math.max(0, item.guardRetryEstimatedCostUsd || 0),
      ),
    0,
  );
  const longContextLogs = logs.filter(
    (item) => item.pricingContextBand === "long",
  );
  const longContextCostUsd = longContextLogs.reduce(
    (sum, item) => sum + Math.max(0, item.estimatedCostUsd || 0),
    0,
  );
  const longContextUpliftUsd = longContextLogs.reduce(
    (sum, item) => sum + Math.max(0, item.longContextUpliftUsd || 0),
    0,
  );

  const modelMap = new Map<
    string,
    {
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
  >();
  for (const item of logs) {
    const model = String(item.model || "").trim() || "(unknown)";
    const current = modelMap.get(model) || {
      model,
      requestCount: 0,
      successCount: 0,
      errorCount: 0,
      totalTokens: 0,
      estimatedCostUsd: 0,
      inputTokens: 0,
      cachedInputTokens: 0,
      outputTokens: 0,
      reasoningOutputTokens: 0,
    };
    const statusCode = item.statusCode ?? 0;
    const isSuccess =
      statusCode >= 200 && statusCode < 300 && !String(item.error || "").trim();
    const isError =
      Boolean(String(item.error || "").trim()) ||
      (item.statusCode != null && item.statusCode >= 400);
    current.requestCount += 1;
    if (isSuccess) current.successCount += 1;
    if (isError) current.errorCount += 1;
    current.totalTokens += Math.max(0, item.totalTokens || 0);
    current.estimatedCostUsd += Math.max(0, item.estimatedCostUsd || 0);
    current.inputTokens += Math.max(0, item.inputTokens || 0);
    current.cachedInputTokens += Math.max(0, item.cachedInputTokens || 0);
    current.outputTokens += Math.max(0, item.outputTokens || 0);
    current.reasoningOutputTokens += Math.max(0, item.reasoningOutputTokens || 0);
    modelMap.set(model, current);
  }
  const modelStats = Array.from(modelMap.values()).sort((left, right) => {
    if (right.estimatedCostUsd !== left.estimatedCostUsd) {
      return right.estimatedCostUsd - left.estimatedCostUsd;
    }
    if (right.totalTokens !== left.totalTokens) {
      return right.totalTokens - left.totalTokens;
    }
    return left.model.localeCompare(right.model);
  });

  return {
    totalCount: logs.length,
    filteredCount: logs.length,
    successCount,
    errorCount,
    totalTokens,
    totalCostUsd,
    guardRetryTotalTokens,
    guardRetryEstimatedCostUsd,
    longContextCount: longContextLogs.length,
    longContextCostUsd,
    longContextUpliftUsd,
    legacyCandidateCount: logs.filter(
      (item) => item.pricingContextBand === "legacy_candidate",
    ).length,
    modelStats,
    modelStatsTruncated: false,
  };
}
