"use client";

import { useEffect, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { getAppErrorMessage } from "@/lib/api/transport-errors";
import { managedModelsV2Client } from "@/lib/api/managed-models-v2";
import { useI18n } from "@/lib/i18n/provider";
import type { AggregateApi, AggregateApiModelDiscoveryItem } from "@/types";
import type { ManagedModelAggregateRouteAddV2Result } from "@/types/model-v2";

interface AggregateApiModelQuickAddDialogProps {
  api: AggregateApi | null;
  item: AggregateApiModelDiscoveryItem | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSuccess: (result: ManagedModelAggregateRouteAddV2Result) => void;
}

function apiName(api: AggregateApi): string {
  return api.supplierName?.trim() || api.providerType;
}

export function AggregateApiModelQuickAddDialog({
  api,
  item,
  open,
  onOpenChange,
  onSuccess,
}: AggregateApiModelQuickAddDialogProps) {
  const { t } = useI18n();
  const [slug, setSlug] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !item) return;
    setSlug(item.id);
    setDisplayName(item.displayName || item.id);
    setError(null);
  }, [item, open]);

  const handleOpenChange = (nextOpen: boolean) => {
    if (isSubmitting && !nextOpen) return;
    onOpenChange(nextOpen);
  };

  const handleSubmit = async () => {
    if (!api || !item || isSubmitting) return;
    const normalizedSlug = slug.trim();
    const normalizedDisplayName = displayName.trim();
    if (!normalizedSlug) {
      setError(t("模型标识不能为空"));
      return;
    }

    setIsSubmitting(true);
    setError(null);
    try {
      const result = await managedModelsV2Client.addAggregateRoute({
        slug: normalizedSlug,
        displayName: normalizedDisplayName,
        aggregateApiId: api.id,
        upstreamModel: item.id,
      });
      setIsSubmitting(false);
      onSuccess(result);
      onOpenChange(false);
      return;
    } catch (submitError) {
      setError(getAppErrorMessage(submitError));
    } finally {
      setIsSubmitting(false);
    }
  };

  if (!api || !item) return null;

  return (
    <Dialog
      open={open}
      onOpenChange={handleOpenChange}
      disablePointerDismissal={isSubmitting}
    >
      <DialogContent
        showCloseButton={!isSubmitting}
        className="glass-card max-h-[calc(100dvh-2rem)] overflow-hidden p-0 sm:max-w-[560px]"
      >
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void handleSubmit();
          }}
        >
          <div className="max-h-[calc(100dvh-2rem)] space-y-5 overflow-y-auto p-5">
          <DialogHeader>
            <DialogTitle>{t("添加到模型与路由")}</DialogTitle>
            <DialogDescription>
              {t("确认后会创建或复用本地模型，并为当前聚合 API 添加或更新一条显式路由。")}
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-4">
            <div className="grid gap-2">
              <Label htmlFor="aggregate-model-quick-add-slug">{t("模型标识（Slug）")}</Label>
              <Input
                id="aggregate-model-quick-add-slug"
                value={slug}
                disabled={isSubmitting}
                required
                aria-required="true"
                aria-invalid={Boolean(error)}
                aria-describedby={error ? "aggregate-model-quick-add-error" : undefined}
                onChange={(event) => setSlug(event.target.value)}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="aggregate-model-quick-add-display-name">{t("显示名称")}</Label>
              <Input
                id="aggregate-model-quick-add-display-name"
                value={displayName}
                disabled={isSubmitting}
                onChange={(event) => setDisplayName(event.target.value)}
              />
            </div>

            <div className="grid gap-2 rounded-lg border border-border/60 bg-muted/20 p-3 text-sm">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-muted-foreground">{t("来源聚合 API")}</span>
                <Badge variant="secondary">{apiName(api)}</Badge>
              </div>
              <code className="break-all text-xs">{api.id}</code>
              <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
                <span className="text-muted-foreground">{t("上游模型")}</span>
                <Badge variant="outline">{item.id}</Badge>
              </div>
            </div>

            <p className="text-xs leading-5 text-muted-foreground">
              {t("同名本地模型会被复用；发现结果不会自动推断价格、能力或计费权限。")}
            </p>
            {error ? (
              <p
                id="aggregate-model-quick-add-error"
                className="text-sm text-destructive"
                role="alert"
              >
                {error}
              </p>
            ) : null}
          </div>
        </div>

        <div className="border-t border-border/50 px-5 py-3">
          <DialogFooter className="mx-0 mb-0 rounded-none border-0 bg-transparent p-0">
            <DialogClose
              className={buttonVariants({ variant: "ghost" })}
              type="button"
              disabled={isSubmitting}
            >
              {t("取消")}
            </DialogClose>
            <Button
              type="submit"
              disabled={isSubmitting || !slug.trim()}
            >
              {isSubmitting ? t("添加中...") : t("确认添加")}
            </Button>
          </DialogFooter>
        </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
