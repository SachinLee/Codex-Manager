"use client";

import { useEffect, useRef } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { useDeferredDesktopActivation } from "@/hooks/useDeferredDesktopActivation";
import { useDesktopPageActive } from "@/hooks/useDesktopPageActive";
import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";
import {
  buildCodexModelsCachePayloadV2,
  managedModelsV2Client,
  serializeManagedModelsV2ForCodexCache,
} from "@/lib/api/managed-models-v2";
import { serviceClient } from "@/lib/api/service-client";
import { getAppErrorMessage } from "@/lib/api/transport";
import { useI18n } from "@/lib/i18n/provider";
import { useAppStore } from "@/lib/store/useAppStore";
import type {
  ManagedModelImportPreviewV2Result,
  ManagedModelImportV2Params,
  ManagedModelListV2Result,
  ManagedModelV2Upsert,
  ModelCatalogV2Stats,
} from "@/types/model-v2";

export const MANAGED_MODELS_V2_QUERY_KEY = ["managed-models-v2"] as const;

const EMPTY_STATS: ModelCatalogV2Stats = {
  total: 0,
  enabled: 0,
  builtin: 0,
  custom: 0,
  priceMissing: 0,
  missingRoute: 0,
};

type BatchDeleteManagedModelsResult = {
  deleted: string[];
  failed: Array<{ slug: string; reason: string }>;
};

export function useManagedModels() {
  const queryClient = useQueryClient();
  const { t } = useI18n();
  const serviceStatus = useAppStore((state) => state.serviceStatus);
  const {
    canAccessManagementRpc,
    isDesktopRuntime,
    canUseBrowserDownloadExport,
  } = useRuntimeCapabilities();
  const isServiceReady = canAccessManagementRpc && serviceStatus.connected;
  const isPageActive = useDesktopPageActive("/models/");
  const isQueryEnabled = useDeferredDesktopActivation(
    isServiceReady && isPageActive,
  );
  const codexUserAgentRef = useRef("");

  const ensureServiceReady = (actionLabel: string): boolean => {
    if (isServiceReady) return true;
    toast.info(`${t("服务未连接，暂时无法")} ${t(actionLabel)}`);
    return false;
  };

  const query = useQuery({
    queryKey: MANAGED_MODELS_V2_QUERY_KEY,
    queryFn: () => managedModelsV2Client.list(true),
    enabled: isQueryEnabled,
    retry: 1,
  });

  const invalidateConsumers = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: MANAGED_MODELS_V2_QUERY_KEY }),
      queryClient.invalidateQueries({ queryKey: ["model-groups"] }),
      queryClient.invalidateQueries({ queryKey: ["startup-snapshot"] }),
    ]);
  };

  const reloadCatalog = async (): Promise<ManagedModelListV2Result> => {
    const catalog = await managedModelsV2Client.list(true);
    queryClient.setQueryData(MANAGED_MODELS_V2_QUERY_KEY, catalog);
    return catalog;
  };

  const saveMutation = useMutation({
    mutationFn: (input: ManagedModelV2Upsert) =>
      managedModelsV2Client.upsert(input),
    onSuccess: async () => {
      await reloadCatalog();
      await invalidateConsumers();
      toast.success(t("模型已保存"));
    },
    onError: (error: unknown) => {
      toast.error(`${t("保存模型失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (slug: string) => managedModelsV2Client.delete(slug),
    onSuccess: async () => {
      await reloadCatalog();
      await invalidateConsumers();
      toast.success(t("模型已删除"));
    },
    onError: (error: unknown) => {
      toast.error(`${t("删除模型失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  const batchDeleteMutation = useMutation({
    mutationFn: async (slugs: string[]): Promise<BatchDeleteManagedModelsResult> => {
      const normalizedSlugs = Array.from(
        new Set(slugs.map((slug) => slug.trim()).filter(Boolean)),
      );
      const deleted: string[] = [];
      const failed: Array<{ slug: string; reason: string }> = [];
      for (const slug of normalizedSlugs) {
        try {
          await managedModelsV2Client.delete(slug);
          deleted.push(slug);
        } catch (error) {
          failed.push({ slug, reason: getAppErrorMessage(error) });
        }
      }
      return { deleted, failed };
    },
    onSuccess: async (result) => {
      await reloadCatalog();
      await invalidateConsumers();
      if (result.deleted.length > 0 && result.failed.length === 0) {
        toast.success(t("已删除 {count} 个模型", { count: result.deleted.length }));
      } else if (result.deleted.length > 0) {
        toast.warning(
          t("批量删除完成：成功{success}个，失败{failed}个", {
            success: result.deleted.length,
            failed: result.failed.length,
          }),
        );
      } else if (result.failed.length > 0) {
        toast.error(
          `${t("批量删除失败")}: ${result.failed[0].slug} - ${result.failed[0].reason}`,
        );
      }
    },
    onError: (error: unknown) => {
      toast.error(`${t("批量删除失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  const previewImportMutation = useMutation({
    mutationFn: (input: ManagedModelImportV2Params) =>
      managedModelsV2Client.previewImport(input),
    onError: (error: unknown) => {
      toast.error(`${t("导入预览失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  const commitImportMutation = useMutation({
    mutationFn: (input: ManagedModelImportV2Params) =>
      managedModelsV2Client.commitImport(input),
    onSuccess: async (result) => {
      await reloadCatalog();
      await invalidateConsumers();
      toast.success(t("已导入 {count} 个模型", { count: result.committed }));
    },
    onError: (error: unknown) => {
      toast.error(`${t("导入提交失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  const resolveCodexUserAgent = async (): Promise<string> => {
    const cachedUserAgent = codexUserAgentRef.current.trim();
    if (cachedUserAgent.includes("codex_cli_rs/")) return cachedUserAgent;
    const initializeResult = await serviceClient.initialize(serviceStatus.addr);
    const userAgent = String(initializeResult.userAgent || "").trim();
    if (!userAgent.includes("codex_cli_rs/")) {
      throw new Error(t("当前服务未返回可用的 Codex CLI 标识"));
    }
    codexUserAgentRef.current = userAgent;
    return userAgent;
  };

  const triggerBrowserDownload = (fileName: string, content: string): void => {
    if (typeof document === "undefined") {
      throw new Error(t("当前环境不支持浏览器导出"));
    }
    const blob = new Blob([content], {
      type: "application/json;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = fileName;
    anchor.style.display = "none";
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 0);
  };

  const exportMutation = useMutation({
    mutationFn: async () => {
      const catalog = query.data ?? (await reloadCatalog());
      const models = serializeManagedModelsV2ForCodexCache(catalog.items);
      if (models.length === 0) throw new Error(t("模型目录为空"));
      const userAgent = await resolveCodexUserAgent();

      if (isDesktopRuntime) {
        await serviceClient.exportCodexModelsCache({ userAgent, models });
        return "desktop" as const;
      }
      if (!canUseBrowserDownloadExport) {
        throw new Error(t("当前环境不支持导出 Codex 缓存"));
      }
      const payload = buildCodexModelsCachePayloadV2(catalog.items, userAgent);
      triggerBrowserDownload(
        "models_cache.json",
        `${JSON.stringify(payload, null, 2)}\n`,
      );
      return "browser" as const;
    },
    onSuccess: (mode) => {
      toast.success(
        mode === "browser"
          ? t("Codex 缓存已下载，请保存到 `~/.codex/models_cache.json`")
          : t("已导出到本地 Codex 缓存"),
      );
    },
    onError: (error: unknown) => {
      toast.error(`${t("导出失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  useEffect(() => {
    codexUserAgentRef.current = "";
  }, [serviceStatus.addr]);

  return {
    models: query.data?.items || [],
    catalog: query.data || { items: [], stats: EMPTY_STATS },
    stats: query.data?.stats || EMPTY_STATS,
    isLoading: isServiceReady && (!isQueryEnabled || query.isLoading),
    isServiceReady,
    refreshLocal: async () => {
      if (!ensureServiceReady("读取模型")) return null;
      const result = await query.refetch();
      if (result.error) throw result.error;
      return result.data ?? null;
    },
    saveModel: async (input: ManagedModelV2Upsert) => {
      if (!ensureServiceReady("保存模型")) return null;
      return saveMutation.mutateAsync(input);
    },
    deleteModel: async (slug: string) => {
      if (!ensureServiceReady("删除模型")) return false;
      await deleteMutation.mutateAsync(slug);
      return true;
    },
    deleteModels: async (slugs: string[]) => {
      if (!ensureServiceReady("批量删除模型")) {
        return { deleted: [], failed: [] };
      }
      return batchDeleteMutation.mutateAsync(slugs);
    },
    previewImport: async (
      input: ManagedModelImportV2Params,
    ): Promise<ManagedModelImportPreviewV2Result | null> => {
      if (!ensureServiceReady("导入模型")) return null;
      return previewImportMutation.mutateAsync(input);
    },
    commitImport: async (
      input: ManagedModelImportV2Params,
    ): Promise<ManagedModelImportPreviewV2Result | null> => {
      if (!ensureServiceReady("导入模型")) return null;
      return commitImportMutation.mutateAsync(input);
    },
    exportCodexCache: async () => {
      if (!ensureServiceReady("导出模型目录")) return false;
      await exportMutation.mutateAsync();
      return true;
    },
    isRefreshing: query.isRefetching,
    isSaving: saveMutation.isPending,
    isDeleting: deleteMutation.isPending || batchDeleteMutation.isPending,
    isImporting:
      previewImportMutation.isPending || commitImportMutation.isPending,
    isExporting: exportMutation.isPending,
    canExportCodexCache:
      isServiceReady &&
      Boolean(query.data?.items?.length) &&
      (isDesktopRuntime || canUseBrowserDownloadExport),
  };
}
