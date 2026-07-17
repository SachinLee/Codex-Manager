"use client";

import { useEffect, useMemo, useState } from "react";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button, buttonVariants } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ManagedModelPayload, ModelPriceRuleUpsertPayload } from "@/lib/api/account-client";
import type { ModelPriceRuleEntry } from "@/lib/api/account-client";
import { useI18n } from "@/lib/i18n/provider";
import { ManagedModelInfo } from "@/types";

type TranslateFn = ReturnType<typeof useI18n>["t"];

interface ModelCatalogModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  model?: ManagedModelInfo | null;
  nextSortIndex: number;
  isSaving?: boolean;
  onSave: (payload: ManagedModelPayload) => Promise<ManagedModelInfo | null>;
  onSavePriceRule?: (payload: ModelPriceRuleUpsertPayload) => Promise<void>;
  priceBillingMode?: "standard" | "priority";
  onPriceBillingModeChange?: (value: "standard" | "priority") => void;
  priceRule?: ModelPriceRuleEntry | null;
}

interface ModelCatalogDraft {
  slug: string;
  displayName: string;
  description: string;
  sourceKind: string;
  userEdited: boolean;
  supportedInApi: boolean;
  sortIndex: string;
  priority: string;
  visibility: string;
  defaultReasoningLevel: string;
  advancedJson: string;
  inputPricePer1m: string;
  cachedInputPricePer1m: string;
  cacheWritePricePer1m: string;
  outputPricePer1m: string;
  longContextThresholdTokens: string;
  longContextThresholdInclusive: boolean;
  longContextInputPricePer1m: string;
  longContextCachedInputPricePer1m: string;
  longContextCacheWritePricePer1m: string;
  longContextOutputPricePer1m: string;
}

const EDITABLE_ADVANCED_KEYS = [
  "supportedReasoningLevels",
  "shellType",
  "additionalSpeedTiers",
  "serviceTiers",
  "defaultServiceTier",
  "availabilityNux",
  "upgrade",
  "upgradeInfo",
  "baseInstructions",
  "modelMessages",
  "supportsReasoningSummaries",
  "defaultReasoningSummary",
  "supportVerbosity",
  "defaultVerbosity",
  "applyPatchToolType",
  "webSearchToolType",
  "truncationPolicy",
  "supportsParallelToolCalls",
  "supportsImageDetailOriginal",
  "contextWindow",
  "autoCompactTokenLimit",
  "effectiveContextWindowPercent",
  "experimentalSupportedTools",
  "inputModalities",
  "minimalClientVersion",
  "supportsSearchTool",
  "availableInPlans",
];

const UNSET_SELECT_VALUE = "__unset__";

const VISIBILITY_OPTIONS = [
  { value: "list", label: "list" },
  { value: "hide", label: "hide" },
] as const;

const REASONING_EFFORT_OPTIONS = [
  { value: "low", label: "low" },
  { value: "medium", label: "medium" },
  { value: "high", label: "high" },
  { value: "xhigh", label: "xhigh" },
] as const;

function normalizeOptionalSelectValue(value: string | null): string {
  if (!value || value === UNSET_SELECT_VALUE) {
    return "";
  }
  if (value === "hidden") {
    return "hide";
  }
  return value;
}

function normalizeVisibilityValue(value: string | null | undefined): string {
  const normalized = String(value || "").trim().toLowerCase();
  if (!normalized) {
    return "";
  }
  if (normalized === "hidden") {
    return "hide";
  }
  return normalized;
}

function toPrettyJson(value: unknown): string {
  if (
    !value ||
    (typeof value === "object" &&
      !Array.isArray(value) &&
      Object.keys(value).length === 0)
  ) {
    return "";
  }
  if (Array.isArray(value) && value.length === 0) {
    return "";
  }
  return JSON.stringify(value, null, 2);
}

function parseOptionalNumber(text: string, label: string, t: TranslateFn): number {
  const parsed = Number(text.trim() || "0");
  if (!Number.isFinite(parsed)) {
    throw new Error(`${label} ${t("必须是数字")}`);
  }
  return parsed;
}

function parseJsonObject(
  text: string,
  label: string,
  t: TranslateFn,
): Record<string, unknown> {
  const trimmed = text.trim();
  if (!trimmed) {
    return {};
  }
  try {
    const parsed = JSON.parse(trimmed);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      throw new Error(t("必须是对象"));
    }
    return parsed as Record<string, unknown>;
  } catch (error) {
    throw new Error(
      `${label} ${t("不是有效 JSON 对象")}: ${
        error instanceof Error ? error.message : String(error)
      }`,
    );
  }
}

function buildAdvancedJson(model: ManagedModelInfo | null | undefined): string {
  if (!model) {
    return toPrettyJson({
      inputModalities: ["text", "image"],
      supportedReasoningLevels: [],
      additionalSpeedTiers: [],
      serviceTiers: [],
      defaultServiceTier: null,
      experimentalSupportedTools: [],
      availableInPlans: [],
    });
  }

  const advanced = Object.fromEntries(
    Object.entries(model).filter(([key]) =>
      EDITABLE_ADVANCED_KEYS.includes(key),
    ),
  );
  const extra = Object.fromEntries(
    Object.entries(model).filter(
      ([key]) =>
        ![
          "slug",
          "displayName",
          "description",
          "sourceKind",
          "userEdited",
          "supportedInApi",
          "sortIndex",
          "updatedAt",
          "priority",
          "visibility",
          "defaultReasoningLevel",
          ...EDITABLE_ADVANCED_KEYS,
        ].includes(key),
    ),
  );
  return toPrettyJson({ ...advanced, ...extra });
}

function buildDraft(
  model: ManagedModelInfo | null | undefined,
  nextSortIndex: number,
  priceRule?: ModelPriceRuleEntry | null,
): ModelCatalogDraft {
  return {
    slug: model?.slug || "",
    displayName: model?.displayName || "",
    description: model?.description || "",
    sourceKind: model?.sourceKind || "custom",
    userEdited: model?.userEdited ?? true,
    supportedInApi: model?.supportedInApi ?? false,
    sortIndex: String(model?.sortIndex ?? nextSortIndex),
    priority: String(model?.priority ?? 0),
    visibility: normalizeVisibilityValue(model?.visibility),
    defaultReasoningLevel: model?.defaultReasoningLevel || "",
    advancedJson: buildAdvancedJson(model),
    inputPricePer1m: priceRule?.inputPricePer1m != null ? String(priceRule.inputPricePer1m) : "",
    cachedInputPricePer1m: priceRule?.cachedInputPricePer1m != null ? String(priceRule.cachedInputPricePer1m) : "",
    cacheWritePricePer1m: priceRule?.cacheWritePricePer1m != null ? String(priceRule.cacheWritePricePer1m) : "",
    outputPricePer1m: priceRule?.outputPricePer1m != null ? String(priceRule.outputPricePer1m) : "",
    longContextThresholdTokens: priceRule?.longContextThresholdTokens != null ? String(priceRule.longContextThresholdTokens) : "",
    longContextThresholdInclusive: priceRule?.longContextThresholdInclusive ?? false,
    longContextInputPricePer1m: priceRule?.longContextInputPricePer1m != null ? String(priceRule.longContextInputPricePer1m) : "",
    longContextCachedInputPricePer1m: priceRule?.longContextCachedInputPricePer1m != null ? String(priceRule.longContextCachedInputPricePer1m) : "",
    longContextCacheWritePricePer1m: priceRule?.longContextCacheWritePricePer1m != null ? String(priceRule.longContextCacheWritePricePer1m) : "",
    longContextOutputPricePer1m: priceRule?.longContextOutputPricePer1m != null ? String(priceRule.longContextOutputPricePer1m) : "",
  };
}

function buildDefaultModel(
  nextSortIndex: number,
  updatedAt: number,
): ManagedModelInfo {
  return {
    slug: "",
    displayName: "",
    description: null,
    defaultReasoningLevel: null,
    supportedReasoningLevels: [],
    shellType: null,
    visibility: null,
    supportedInApi: false,
    priority: 0,
    additionalSpeedTiers: [],
    serviceTiers: [],
    defaultServiceTier: null,
    availabilityNux: null,
    upgrade: null,
    upgradeInfo: null,
    baseInstructions: null,
    modelMessages: null,
    supportsReasoningSummaries: null,
    defaultReasoningSummary: null,
    supportVerbosity: null,
    defaultVerbosity: null,
    applyPatchToolType: null,
    webSearchToolType: null,
    truncationPolicy: null,
    supportsParallelToolCalls: null,
    supportsImageDetailOriginal: null,
    contextWindow: null,
    autoCompactTokenLimit: null,
    effectiveContextWindowPercent: null,
    experimentalSupportedTools: [],
    inputModalities: ["text", "image"],
    minimalClientVersion: null,
    supportsSearchTool: null,
    availableInPlans: [],
    sourceKind: "custom",
    userEdited: true,
    sortIndex: nextSortIndex,
    updatedAt,
  };
}

export function ModelCatalogModal({
  open,
  onOpenChange,
  model,
  nextSortIndex,
  isSaving = false,
  onSave,
  onSavePriceRule,
  priceBillingMode = "standard",
  onPriceBillingModeChange,
  priceRule,
}: ModelCatalogModalProps) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<ModelCatalogDraft>(() =>
    buildDraft(model, nextSortIndex, priceRule),
  );
  const [priceError, setPriceError] = useState<string | null>(null);
  const [savingPrice, setSavingPrice] = useState(false);

  useEffect(() => {
    if (!open) return;
    const frameId = window.requestAnimationFrame(() => {
      setDraft(buildDraft(model, nextSortIndex, priceRule));
      setPriceError(null);
      setSavingPrice(false);
    });
    return () => {
      window.cancelAnimationFrame(frameId);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [model, nextSortIndex, open]);

  useEffect(() => {
    if (!open) return;
    setDraft((prev) => ({
      ...prev,
      inputPricePer1m:
        priceRule?.inputPricePer1m != null
          ? String(priceRule.inputPricePer1m)
          : "",
      cachedInputPricePer1m:
        priceRule?.cachedInputPricePer1m != null
          ? String(priceRule.cachedInputPricePer1m)
          : "",
      cacheWritePricePer1m:
        priceRule?.cacheWritePricePer1m != null
          ? String(priceRule.cacheWritePricePer1m)
          : "",
      outputPricePer1m:
        priceRule?.outputPricePer1m != null
          ? String(priceRule.outputPricePer1m)
          : "",
      longContextThresholdTokens:
        priceRule?.longContextThresholdTokens != null
          ? String(priceRule.longContextThresholdTokens)
          : "",
      longContextInputPricePer1m:
        priceRule?.longContextInputPricePer1m != null
          ? String(priceRule.longContextInputPricePer1m)
          : "",
      longContextCachedInputPricePer1m:
        priceRule?.longContextCachedInputPricePer1m != null
          ? String(priceRule.longContextCachedInputPricePer1m)
          : "",
      longContextCacheWritePricePer1m:
        priceRule?.longContextCacheWritePricePer1m != null
          ? String(priceRule.longContextCacheWritePricePer1m)
          : "",
      longContextOutputPricePer1m:
        priceRule?.longContextOutputPricePer1m != null
          ? String(priceRule.longContextOutputPricePer1m)
          : "",
    }));
  }, [priceRule, open]);

  const title = useMemo(
    () => (model ? t("编辑模型") : t("新增模型")),
    [model, t],
  );

  const updateDraft = <K extends keyof ModelCatalogDraft>(
    key: K,
    value: ModelCatalogDraft[K],
  ) => {
    setDraft((current) => ({ ...current, [key]: value }));
  };

  const handleSave = async () => {
    const slug = draft.slug.trim();
    if (!slug) {
      setPriceError(t("模型 slug 不能为空"));
      return;
    }

    const advancedFields = parseJsonObject(draft.advancedJson, t("高级 JSON"), t);
    const nextModel: ManagedModelInfo = {
      ...buildDefaultModel(nextSortIndex, model?.updatedAt ?? 0),
      ...advancedFields,
      slug,
      displayName: draft.displayName.trim() || slug,
      description: draft.description.trim() || null,
      sourceKind: draft.sourceKind,
      userEdited: draft.userEdited,
      supportedInApi: draft.supportedInApi,
      sortIndex: parseOptionalNumber(draft.sortIndex, t("排序权重"), t),
      priority: parseOptionalNumber(draft.priority, t("Priority"), t),
      visibility: draft.visibility.trim() || null,
      defaultReasoningLevel: draft.defaultReasoningLevel.trim() || null,
      updatedAt: model?.updatedAt ?? 0,
    };

    const ip = draft.inputPricePer1m.trim();
    const cp = draft.cachedInputPricePer1m.trim();
    const wp = draft.cacheWritePricePer1m.trim();
    const op = draft.outputPricePer1m.trim();
    const longThreshold = draft.longContextThresholdTokens.trim();
    const longIp = draft.longContextInputPricePer1m.trim();
    const longCp = draft.longContextCachedInputPricePer1m.trim();
    const longWp = draft.longContextCacheWritePricePer1m.trim();
    const longOp = draft.longContextOutputPricePer1m.trim();
    const hasAnyPriceText = [ip, cp, wp, op, longThreshold, longIp, longCp, longWp, longOp].some(
      (value) => value !== "",
    );
    const isClearingExistingOverride = !hasAnyPriceText && !!priceRule?.id;
    const hasUserInput = hasAnyPriceText || isClearingExistingOverride;

    if (hasUserInput) {
      const inputNum = ip !== "" ? Number(ip) : null;
      const cachedNum = cp !== "" ? Number(cp) : null;
      const writeNum = wp !== "" ? Number(wp) : null;
      const outputNum = op !== "" ? Number(op) : null;
      const longThresholdNum = longThreshold !== "" ? Number(longThreshold) : null;
      const longNumbers = [longIp, longCp, longWp, longOp]
        .filter((value) => value !== "")
        .map(Number);
      if (
        (inputNum !== null && (!Number.isFinite(inputNum) || inputNum < 0)) ||
        (cachedNum !== null && (!Number.isFinite(cachedNum) || cachedNum < 0)) ||
        (writeNum !== null && (!Number.isFinite(writeNum) || writeNum < 0)) ||
        (outputNum !== null && (!Number.isFinite(outputNum) || outputNum < 0)) ||
        longNumbers.some((value) => !Number.isFinite(value) || value < 0) ||
        (longThresholdNum !== null &&
          (!Number.isInteger(longThresholdNum) || longThresholdNum < 0))
      ) {
        setPriceError(t("价格必须为非负有效数字"));
        return;
      }
      if (!isClearingExistingOverride && (inputNum == null || outputNum == null)) {
        setPriceError(t("输入价格和输出价格必须同时填写"));
        return;
      }
    }

    const saved = await onSave({
      previousSlug: model?.slug || null,
      sourceKind: nextModel.sourceKind,
      userEdited: nextModel.userEdited,
      sortIndex: nextModel.sortIndex,
      model: nextModel,
    });
    if (saved) {
      if (onSavePriceRule && slug && hasUserInput) {
        try {
          setSavingPrice(true);
          await onSavePriceRule({
            id: priceRule?.id,
            provider: priceRule?.provider ?? undefined,
            modelPattern: slug,
            billingMode: priceBillingMode,
            inputPricePer1m: isClearingExistingOverride ? 0 : Number(ip),
            cachedInputPricePer1m: isClearingExistingOverride
              ? null
              : (cp !== "" ? Number(cp) : null),
            cacheWritePricePer1m: isClearingExistingOverride
              ? null
              : (wp !== "" ? Number(wp) : null),
            outputPricePer1m: isClearingExistingOverride ? 0 : Number(op),
            longContextThresholdTokens: isClearingExistingOverride
              ? null
              : (longThreshold !== "" ? Number(longThreshold) : null),
            longContextThresholdInclusive: isClearingExistingOverride
              ? false
              : draft.longContextThresholdInclusive,
            longContextInputPricePer1m: isClearingExistingOverride
              ? null
              : (longIp !== "" ? Number(longIp) : null),
            longContextCachedInputPricePer1m: isClearingExistingOverride
              ? null
              : (longCp !== "" ? Number(longCp) : null),
            longContextCacheWritePricePer1m: isClearingExistingOverride
              ? null
              : (longWp !== "" ? Number(longWp) : null),
            longContextOutputPricePer1m: isClearingExistingOverride
              ? null
              : (longOp !== "" ? Number(longOp) : null),
            enabled: isClearingExistingOverride ? false : true,
          });
        } catch (error) {
          setPriceError(
            `${t("模型已保存，但价格保存失败")}: ${
              error instanceof Error ? error.message : String(error)
            }`,
          );
          setSavingPrice(false);
          return;
        }
        setSavingPrice(false);
      }
      onOpenChange(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="glass-card p-0 md:max-w-[750px] xl:max-w-[1360px]">
        <div className="max-h-[84vh] overflow-y-auto p-6">
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>
              {t(
                "核心字段单独编辑，其余官方 `/models` 参数请直接在高级 JSON 中维护。",
              )}
            </DialogDescription>
          </DialogHeader>

          <div className="mt-6 grid gap-6">
            <div className="grid gap-4 md:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="model-slug">Slug</Label>
                <Input
                  id="model-slug"
                  value={draft.slug}
                  onChange={(event) => updateDraft("slug", event.target.value)}
                  placeholder="gpt-5.4"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="model-display-name">{t("显示名称")}</Label>
                <Input
                  id="model-display-name"
                  value={draft.displayName}
                  onChange={(event) =>
                    updateDraft("displayName", event.target.value)
                  }
                  placeholder="GPT-5.4"
                />
              </div>
              <div className="space-y-2 md:col-span-2">
                <Label htmlFor="model-description">{t("描述")}</Label>
                <Textarea
                  id="model-description"
                  rows={3}
                  value={draft.description}
                  onChange={(event) =>
                    updateDraft("description", event.target.value)
                  }
                />
              </div>
            </div>

            <div className="grid gap-4 md:grid-cols-2">
              <div className="space-y-2">
                <Label>{t("来源类型")}</Label>
                <Select
                  value={draft.sourceKind}
                  onValueChange={(value) =>
                    updateDraft("sourceKind", value || "custom")
                  }
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                    <SelectItem value="custom">{t("自定义")}</SelectItem>
                    <SelectItem value="remote">{t("远端同步")}</SelectItem>
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-2">
                <Label htmlFor="model-sort-index">{t("排序权重")}</Label>
                <Input
                  id="model-sort-index"
                  type="number"
                  value={draft.sortIndex}
                  onChange={(event) =>
                    updateDraft("sortIndex", event.target.value)
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="model-priority">{t("Priority")}</Label>
                <Input
                  id="model-priority"
                  type="number"
                  value={draft.priority}
                  onChange={(event) =>
                    updateDraft("priority", event.target.value)
                  }
                />
              </div>
              <div className="space-y-2">
                <Label>{t("可见性")}</Label>
                <Select
                  value={draft.visibility.trim() || UNSET_SELECT_VALUE}
                  onValueChange={(value) =>
                    updateDraft("visibility", normalizeOptionalSelectValue(value))
                  }
                >
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder={t("未设置")} />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                    <SelectItem value={UNSET_SELECT_VALUE}>{t("未设置")}</SelectItem>
                    {VISIBILITY_OPTIONS.map((option) => (
                      <SelectItem key={option.value} value={option.value}>
                        {option.label}
                      </SelectItem>
                    ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-2 md:col-span-2">
                <Label>{t("默认推理等级")}</Label>
                <Select
                  value={draft.defaultReasoningLevel.trim() || UNSET_SELECT_VALUE}
                  onValueChange={(value) =>
                    updateDraft(
                      "defaultReasoningLevel",
                      normalizeOptionalSelectValue(value),
                    )
                  }
                >
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder={t("未设置")} />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                    <SelectItem value={UNSET_SELECT_VALUE}>{t("未设置")}</SelectItem>
                    {REASONING_EFFORT_OPTIONS.map((option) => (
                      <SelectItem key={option.value} value={option.value}>
                        {option.label}
                      </SelectItem>
                    ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="grid gap-4 md:grid-cols-2">
              <Card size="sm">
                <CardContent className="flex items-center justify-between gap-4">
                  <div className="space-y-1">
                    <p className="text-sm font-medium">{t("可用于 API")}</p>
                    <p className="text-xs text-muted-foreground">
                      {t("开启后必须至少配置一个启用映射，否则只能保存为草稿。")}
                    </p>
                  </div>
                  <Switch
                    checked={draft.supportedInApi}
                    onCheckedChange={(checked) =>
                      updateDraft("supportedInApi", checked)
                    }
                  />
                </CardContent>
              </Card>
              <Card size="sm">
                <CardContent className="flex items-center justify-between gap-4">
                  <div className="space-y-1">
                    <p className="text-sm font-medium">{t("保留本地覆写")}</p>
                    <p className="text-xs text-muted-foreground">
                      {t("开启后，远端刷新不会直接覆盖当前本地版本。")}
                    </p>
                  </div>
                  <Switch
                    checked={draft.userEdited}
                    onCheckedChange={(checked) =>
                      updateDraft("userEdited", checked)
                    }
                  />
                </CardContent>
              </Card>
            </div>

            <div className="space-y-2">
              <Label className="text-sm font-medium">{t("Token 价格 (USD / 1M tokens)")}</Label>
              <p className="text-xs text-muted-foreground">
                {t("零表示不计费，价格将用于请求成本估算。")}
              </p>
              {priceError ? (
                <p className="text-xs text-destructive">{priceError}</p>
              ) : null}
            </div>
            <div className="space-y-2">
              <Label>{t("价格档位")}</Label>
              <Select
                value={priceBillingMode}
                onValueChange={(value) =>
                  onPriceBillingModeChange?.(
                    value === "priority" ? "priority" : "standard",
                  )
                }
              >
                <SelectTrigger className="w-full md:w-64">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectItem value="standard">Standard</SelectItem>
                    <SelectItem value="priority">Fast / Priority</SelectItem>
                  </SelectGroup>
                </SelectContent>
              </Select>
            </div>
            <div className="grid gap-4 md:grid-cols-4">
              <div className="space-y-2">
                <Label htmlFor="price-input">{t("输入价格")}</Label>
                <Input
                  id="price-input"
                  type="number"
                  step="0.0001"
                  min="0"
                  value={draft.inputPricePer1m}
                  onChange={(event) =>
                    updateDraft("inputPricePer1m", event.target.value)
                  }
                  placeholder="0"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="price-cached">{t("缓存输入价格")}</Label>
                <Input
                  id="price-cached"
                  type="number"
                  step="0.0001"
                  min="0"
                  value={draft.cachedInputPricePer1m}
                  onChange={(event) =>
                    updateDraft("cachedInputPricePer1m", event.target.value)
                  }
                  placeholder="0"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="price-output">{t("输出价格")}</Label>
                <Input
                  id="price-output"
                  type="number"
                  step="0.0001"
                  min="0"
                  value={draft.outputPricePer1m}
                  onChange={(event) =>
                    updateDraft("outputPricePer1m", event.target.value)
                  }
                  placeholder="0"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="price-cache-write">{t("缓存写入价格")}</Label>
                <Input
                  id="price-cache-write"
                  type="number"
                  step="0.0001"
                  min="0"
                  value={draft.cacheWritePricePer1m}
                  onChange={(event) =>
                    updateDraft("cacheWritePricePer1m", event.target.value)
                  }
                  placeholder={t("留空时按输入价格")}
                />
              </div>
            </div>

            <div className="space-y-2 rounded-md border border-border/60 p-4">
              <Label>{t("长上下文价格（可选）")}</Label>
              <p className="text-xs text-muted-foreground">
                {draft.longContextThresholdInclusive
                  ? t("输入 token 大于或等于阈值时整次请求使用这组价格。")
                  : t("输入 token 严格大于阈值时整次请求使用这组价格；Priority 未公布长上下文价格时请留空。")}
              </p>
              <div className="mt-3 grid gap-4 md:grid-cols-6">
                <div className="space-y-2">
                  <Label htmlFor="price-long-threshold">{t("阈值")}</Label>
                  <Input
                    id="price-long-threshold"
                    type="number"
                    step="1"
                    min="0"
                    value={draft.longContextThresholdTokens}
                    onChange={(event) =>
                      updateDraft("longContextThresholdTokens", event.target.value)
                    }
                    placeholder="272000"
                  />
                </div>
                <div className="flex items-center gap-2 pt-7">
                  <Switch
                    id="price-long-threshold-inclusive"
                    checked={draft.longContextThresholdInclusive}
                    onCheckedChange={(checked) =>
                      updateDraft("longContextThresholdInclusive", checked)
                    }
                  />
                  <Label htmlFor="price-long-threshold-inclusive" className="text-xs">
                    {t("阈值包含等于")}
                  </Label>
                </div>
                <div className="space-y-2">
                  <Label htmlFor="price-long-input">{t("长输入")}</Label>
                  <Input id="price-long-input" type="number" step="0.0001" min="0" value={draft.longContextInputPricePer1m} onChange={(event) => updateDraft("longContextInputPricePer1m", event.target.value)} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="price-long-cached">{t("长缓存读取")}</Label>
                  <Input id="price-long-cached" type="number" step="0.0001" min="0" value={draft.longContextCachedInputPricePer1m} onChange={(event) => updateDraft("longContextCachedInputPricePer1m", event.target.value)} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="price-long-write">{t("长缓存写入")}</Label>
                  <Input id="price-long-write" type="number" step="0.0001" min="0" value={draft.longContextCacheWritePricePer1m} onChange={(event) => updateDraft("longContextCacheWritePricePer1m", event.target.value)} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="price-long-output">{t("长输出")}</Label>
                  <Input id="price-long-output" type="number" step="0.0001" min="0" value={draft.longContextOutputPricePer1m} onChange={(event) => updateDraft("longContextOutputPricePer1m", event.target.value)} />
                </div>
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="model-advanced-json">{t("高级 JSON")}</Label>
              <Textarea
                id="model-advanced-json"
                rows={20}
                value={draft.advancedJson}
                onChange={(event) =>
                  updateDraft("advancedJson", event.target.value)
                }
                placeholder='{"inputModalities":["text","image"],"supportedReasoningLevels":[{"effort":"medium","description":"balanced"}]}'
              />
              <p className="text-xs text-muted-foreground">
                {t(
                  "在这里维护所有复杂字段，包括 supportedReasoningLevels、truncationPolicy、inputModalities、availableInPlans 以及任意扩展字段。",
                )}
              </p>
            </div>
          </div>
        </div>

        <div className="border-t border-border/50 px-5 py-3">
          <DialogFooter>
            <DialogClose
              className={buttonVariants({ variant: "ghost" })}
              type="button"
            >
              {t("取消")}
            </DialogClose>
            <Button
              onClick={() => {
                void handleSave();
              }}
              disabled={isSaving || savingPrice}
            >
              {isSaving ? t("保存中...") : t("保存模型")}
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  );
}
