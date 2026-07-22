"use client";

import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Boxes,
  CircleDollarSign,
  Database,
  Download,
  EyeOff,
  FileJson,
  GitBranch,
  PencilLine,
  Plus,
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react";

import { PageHeader, MetricCard, PageWorkspace } from "@/components/layout/page-workspace";
import { ConfirmDialog } from "@/components/modals/confirm-dialog";
import { ModelCatalogModal } from "@/components/modals/model-catalog-modal";
import { ModelImportModal } from "@/components/modals/model-import-modal";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useDesktopPageActive } from "@/hooks/useDesktopPageActive";
import { isAdminRole, resolveSessionRole, useAppSession } from "@/hooks/useAppSession";
import { useManagedModels } from "@/hooks/useManagedModels";
import { usePageTransitionReady } from "@/hooks/usePageTransitionReady";
import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";
import { accountClient } from "@/lib/api/account-client";
import { microusdToUsdPerMillion } from "@/lib/api/managed-models-v2";
import { useI18n } from "@/lib/i18n/provider";
import type { ManagedModelV2 } from "@/types/model-v2";

type ModelFilter =
  | "all"
  | "enabled"
  | "builtin"
  | "custom"
  | "price_missing"
  | "route_missing"
  | "hidden";

function enabledRouteCount(model: ManagedModelV2): number {
  return model.routes.filter((route) => route.enabled).length;
}

function modelMatchesFilter(model: ManagedModelV2, filter: ModelFilter): boolean {
  if (model.visibility === "hide") return filter === "hidden";
  if (filter === "hidden") return false;
  if (filter === "enabled") return model.enabled;
  if (filter === "builtin") return model.origin === "builtin";
  if (filter === "custom") return model.origin === "custom";
  if (filter === "price_missing") return model.price.priceStatus === "missing";
  if (filter === "route_missing") return enabledRouteCount(model) === 0;
  return true;
}

function PriceBadge({ model }: { model: ManagedModelV2 }) {
  if (model.price.priceStatus === "missing") {
    return <Badge variant="destructive">price missing</Badge>;
  }
  const input = microusdToUsdPerMillion(model.price.inputMicrousdPer1m);
  const cached = microusdToUsdPerMillion(model.price.cachedInputMicrousdPer1m);
  const output = microusdToUsdPerMillion(model.price.outputMicrousdPer1m);
  return (
    <div className="space-y-1">
      <Badge variant="secondary">{model.price.priceStatus}</Badge>
      <div className="font-mono text-[10px] text-muted-foreground">
        {input} / {cached} / {output}
      </div>
    </div>
  );
}

export default function ModelsPage() {
  const { t } = useI18n();
  const { isDesktopRuntime } = useRuntimeCapabilities();
  const { data: session, isLoading: isSessionLoading } = useAppSession();
  const role = resolveSessionRole(session, isSessionLoading, isDesktopRuntime);
  const isAdminMode = isAdminRole(role);
  const isPageActive = useDesktopPageActive("/models/");
  const {
    models,
    stats,
    isLoading,
    isServiceReady,
    refreshLocal,
    saveModel,
    deleteModel,
    deleteModels,
    previewImport,
    commitImport,
    exportCodexCache,
    canExportCodexCache,
    isRefreshing,
    isSaving,
    isDeleting,
    isImporting,
    isExporting,
  } = useManagedModels();
  usePageTransitionReady("/models/", !isServiceReady || !isLoading);

  const { data: aggregateApis = [] } = useQuery({
    queryKey: ["aggregate-apis"],
    queryFn: () => accountClient.listAggregateApis(),
    enabled:
      isServiceReady && isPageActive && isAdminMode && !isSessionLoading,
    staleTime: 60_000,
    retry: 1,
  });

  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<ModelFilter>("all");
  const [editorOpen, setEditorOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [editingSlug, setEditingSlug] = useState<string | null>(null);
  const [selectedSlugs, setSelectedSlugs] = useState<string[]>([]);
  const [deleteSlugs, setDeleteSlugs] = useState<string[]>([]);

  useEffect(() => {
    if (isPageActive) return;
    const frameId = window.requestAnimationFrame(() => {
      setEditorOpen(false);
      setImportOpen(false);
      setEditingSlug(null);
      setSelectedSlugs([]);
      setDeleteSlugs([]);
    });
    return () => window.cancelAnimationFrame(frameId);
  }, [isPageActive]);

  useEffect(() => {
    const availableSlugs = new Set(models.map((model) => model.slug));
    const frameId = window.requestAnimationFrame(() => {
      setSelectedSlugs((current) =>
        current.filter((slug) => availableSlugs.has(slug)),
      );
    });
    return () => window.cancelAnimationFrame(frameId);
  }, [models]);

  const filteredModels = useMemo(() => {
    const needle = search.trim().toLocaleLowerCase();
    return models.filter((model) => {
      if (!modelMatchesFilter(model, filter)) return false;
      if (!needle) return true;
      return [
        model.slug,
        model.displayName,
        model.description || "",
        model.provider || "",
        model.family || "",
        ...model.tags,
      ].some((value) => value.toLocaleLowerCase().includes(needle));
    });
  }, [filter, models, search]);

  const editingModel = useMemo(
    () => models.find((model) => model.slug === editingSlug) || null,
    [editingSlug, models],
  );
  const nextSortOrder = useMemo(
    () => models.reduce((largest, model) => Math.max(largest, model.sortOrder), 0) + 10,
    [models],
  );
  const selectedVisibleCount = filteredModels.filter((model) =>
    selectedSlugs.includes(model.slug),
  ).length;
  const allVisibleSelected =
    filteredModels.length > 0 && selectedVisibleCount === filteredModels.length;

  const openNewModel = () => {
    setEditingSlug(null);
    setEditorOpen(true);
  };

  const openEditor = (slug: string) => {
    setEditingSlug(slug);
    setEditorOpen(true);
  };

  const confirmDeleteDescription = useMemo(() => {
    if (deleteSlugs.length === 0) return "";
    const builtinCount = deleteSlugs.filter(
      (slug) => models.find((model) => model.slug === slug)?.origin === "builtin",
    ).length;
    if (deleteSlugs.length === 1) {
      const model = models.find((item) => item.slug === deleteSlugs[0]);
      return model?.origin === "builtin"
        ? t("Builtin 模型 {slug} 将被禁用，数据不会删除。", { slug: model.slug })
        : t("确定要永久删除自定义模型 {slug} 吗？", { slug: deleteSlugs[0] });
    }
    return t(
      "将处理 {count} 个模型：{builtin} 个 builtin 会被禁用，其余 custom 会被删除。",
      { count: deleteSlugs.length, builtin: builtinCount },
    );
  }, [deleteSlugs, models, t]);

  return (
    <>
      <PageWorkspace>
        <PageHeader
          title={isAdminMode ? t("模型管理") : t("可用模型")}
          description={t("本地模型目录是唯一运行时真相源；价格、路由和 instructions policy 原子保存。")}
          actions={
            <>
              <Button
                size="sm"
                variant="outline"
                disabled={!isServiceReady || isRefreshing}
                onClick={() => void refreshLocal()}
              >
                <RefreshCw className={`mr-1.5 h-4 w-4 ${isRefreshing ? "animate-spin" : ""}`} />
                {t("重新读取")}
              </Button>
              {isAdminMode ? (
                <Button size="sm" variant="outline" onClick={() => setImportOpen(true)}>
                  <FileJson className="mr-1.5 h-4 w-4" />
                  {t("从本地 JSON 导入")}
                </Button>
              ) : null}
              <Button
                size="sm"
                variant="outline"
                disabled={!canExportCodexCache || isExporting}
                onClick={() => void exportCodexCache()}
              >
                <Download className="mr-1.5 h-4 w-4" />
                {isExporting ? t("导出中...") : t("导出到本地 Codex 缓存")}
              </Button>
              {isAdminMode ? (
                <Button size="sm" disabled={!isServiceReady} onClick={openNewModel}>
                  <Plus className="mr-1.5 h-4 w-4" />
                  {t("新增自定义模型")}
                </Button>
              ) : null}
            </>
          }
        />

        <section className="grid grid-cols-2 gap-2 md:grid-cols-3 xl:grid-cols-6">
          <MetricCard title={t("总数")} value={stats.total} icon={Database} tone="blue" />
          <MetricCard title={t("已启用")} value={stats.enabled} icon={Boxes} tone="emerald" />
          <MetricCard title="builtin" value={stats.builtin} icon={Database} tone="violet" />
          <MetricCard title="custom" value={stats.custom} icon={Plus} tone="slate" />
          <MetricCard title="price missing" value={stats.priceMissing} icon={CircleDollarSign} tone="amber" />
          <MetricCard title="missing route" value={stats.missingRoute} icon={GitBranch} tone="rose" />
        </section>

        <Card className="glass-card overflow-hidden py-0">
          <CardHeader className="border-b border-border/50 px-4 py-3">
            <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
              <div>
                <CardTitle>{t("模型目录明细")}</CardTitle>
                <p className="mt-1 text-xs text-muted-foreground">
                  {t("显示 origin、启用状态、价格状态、instructions mode 和 route 状态。")}
                </p>
              </div>
              <div className="flex flex-1 flex-wrap items-center justify-end gap-2">
                <div className="relative min-w-[220px] flex-1 lg:max-w-[320px]">
                  <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                  <Input
                    value={search}
                    onChange={(event) => setSearch(event.target.value)}
                    placeholder={t("搜索模型")}
                    className="h-9 pl-9"
                  />
                </div>
                <Select value={filter} onValueChange={(value) => setFilter((value || "all") as ModelFilter)}>
                  <SelectTrigger className="h-9 w-[160px]"><SelectValue /></SelectTrigger>
                  <SelectContent><SelectGroup>
                    <SelectItem value="all">{t("全部模型")}</SelectItem>
                    <SelectItem value="enabled">{t("已启用")}</SelectItem>
                    <SelectItem value="builtin">builtin</SelectItem>
                    <SelectItem value="custom">custom</SelectItem>
                    <SelectItem value="price_missing">price missing</SelectItem>
                    <SelectItem value="route_missing">missing route</SelectItem>
                    <SelectItem value="hidden">hidden</SelectItem>
                  </SelectGroup></SelectContent>
                </Select>
                {isAdminMode && selectedSlugs.length > 0 ? (
                  <Button
                    size="sm"
                    variant="destructive"
                    disabled={isDeleting}
                    onClick={() => setDeleteSlugs(selectedSlugs)}
                  >
                    <Trash2 className="mr-1.5 h-4 w-4" />
                    {t("批量删除模型")} ({selectedSlugs.length})
                  </Button>
                ) : null}
              </div>
            </div>
          </CardHeader>

          <CardContent className="p-0">
            {!isServiceReady ? (
              <Empty className="min-h-64">
                <EmptyHeader><EmptyTitle>{t("服务未连接，模型目录暂不可用。")}</EmptyTitle></EmptyHeader>
              </Empty>
            ) : isLoading ? (
              <div className="space-y-2 p-4">
                {Array.from({ length: 6 }).map((_, index) => (
                  <Skeleton key={index} className="h-12 w-full" />
                ))}
              </div>
            ) : filteredModels.length === 0 ? (
              <Empty className="min-h-64">
                <EmptyHeader><EmptyTitle>{t("没有符合条件的模型。")}</EmptyTitle></EmptyHeader>
              </Empty>
            ) : (
              <div className="overflow-x-auto">
                <Table>
                  <TableHeader>
                    <TableRow>
                      {isAdminMode ? (
                        <TableHead className="w-10">
                          <Checkbox
                            aria-label={t("选择全部模型")}
                            checked={allVisibleSelected}
                            onCheckedChange={(checked) => {
                              const visibleSlugs = filteredModels.map((model) => model.slug);
                              setSelectedSlugs((current) =>
                                checked === true
                                  ? Array.from(new Set([...current, ...visibleSlugs]))
                                  : current.filter((slug) => !visibleSlugs.includes(slug)),
                              );
                            }}
                          />
                        </TableHead>
                      ) : null}
                      <TableHead>{t("模型")}</TableHead>
                      <TableHead>origin</TableHead>
                      <TableHead>{t("状态")}</TableHead>
                      <TableHead>{t("价格")}</TableHead>
                      <TableHead>instructions</TableHead>
                      <TableHead>routes</TableHead>
                      {isAdminMode ? <TableHead className="w-24 text-right">{t("操作")}</TableHead> : null}
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {filteredModels.map((model) => {
                      const routeCount = enabledRouteCount(model);
                      return (
                        <TableRow key={model.id || model.slug} className={!model.enabled ? "opacity-60" : undefined}>
                          {isAdminMode ? (
                            <TableCell>
                              <Checkbox
                                aria-label={t("选择模型 {slug}", { slug: model.slug })}
                                checked={selectedSlugs.includes(model.slug)}
                                onCheckedChange={(checked) =>
                                  setSelectedSlugs((current) =>
                                    checked === true
                                      ? Array.from(new Set([...current, model.slug]))
                                      : current.filter((slug) => slug !== model.slug),
                                  )
                                }
                              />
                            </TableCell>
                          ) : null}
                          <TableCell className="min-w-[240px]">
                            <div className="font-medium">{model.displayName}</div>
                            <div className="font-mono text-xs text-muted-foreground">{model.slug}</div>
                            {model.description ? <div className="mt-1 max-w-[360px] truncate text-xs text-muted-foreground">{model.description}</div> : null}
                          </TableCell>
                          <TableCell>
                            <div className="flex flex-wrap gap-1">
                              <Badge variant={model.origin === "builtin" ? "secondary" : "outline"}>{model.origin}</Badge>
                              {model.visibility === "hide" ? <Badge variant="outline"><EyeOff className="mr-1 h-3 w-3" />hide</Badge> : null}
                            </div>
                          </TableCell>
                          <TableCell>
                            <Badge variant={model.enabled ? "default" : "outline"}>{model.enabled ? t("已启用") : t("已禁用")}</Badge>
                          </TableCell>
                          <TableCell><PriceBadge model={model} /></TableCell>
                          <TableCell><Badge variant="outline">{model.instructionsMode}</Badge></TableCell>
                          <TableCell>
                            {routeCount > 0 ? (
                              <div className="space-y-1">
                                <Badge variant="secondary">{routeCount} routes</Badge>
                                <div className="max-w-[220px] truncate font-mono text-[10px] text-muted-foreground">
                                  {model.routes.filter((route) => route.enabled).map((route) => `${route.sourceKind}:${route.sourceId}`).join(", ")}
                                </div>
                              </div>
                            ) : (
                              <Badge variant="destructive">missing route</Badge>
                            )}
                          </TableCell>
                          {isAdminMode ? (
                            <TableCell>
                              <div className="flex justify-end gap-1">
                                <Button type="button" variant="ghost" size="icon" aria-label={t("编辑模型 {slug}", { slug: model.slug })} onClick={() => openEditor(model.slug)}>
                                  <PencilLine className="h-4 w-4" />
                                </Button>
                                <Button type="button" variant="ghost" size="icon" aria-label={model.origin === "builtin" ? t("禁用模型 {slug}", { slug: model.slug }) : t("删除模型 {slug}", { slug: model.slug })} onClick={() => setDeleteSlugs([model.slug])}>
                                  <Trash2 className="h-4 w-4" />
                                </Button>
                              </div>
                            </TableCell>
                          ) : null}
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              </div>
            )}
          </CardContent>
        </Card>
      </PageWorkspace>

      {isAdminMode && editorOpen ? (
        <ModelCatalogModal
          open
          onOpenChange={setEditorOpen}
          model={editingModel}
          nextSortOrder={nextSortOrder}
          aggregateApis={aggregateApis}
          isSaving={isSaving}
          onSave={saveModel}
        />
      ) : null}

      {isAdminMode ? (
        <ModelImportModal
          open={importOpen}
          onOpenChange={setImportOpen}
          isWorking={isImporting}
          onPreview={previewImport}
          onCommit={commitImport}
        />
      ) : null}

      {isAdminMode ? (
        <ConfirmDialog
          open={deleteSlugs.length > 0}
          onOpenChange={(open) => {
            if (!open) setDeleteSlugs([]);
          }}
          title={deleteSlugs.length > 1 ? t("批量删除模型") : t("删除模型")}
          description={confirmDeleteDescription}
          confirmText={isDeleting ? t("处理中...") : t("删除")}
          confirmVariant="destructive"
          onConfirm={() => {
            const targets = [...deleteSlugs];
            if (targets.length === 1) {
              void deleteModel(targets[0]).then(() => {
                setSelectedSlugs((current) => current.filter((slug) => slug !== targets[0]));
                setDeleteSlugs([]);
              });
              return;
            }
            void deleteModels(targets).then((result) => {
              setSelectedSlugs((current) => current.filter((slug) => !result.deleted.includes(slug)));
              setDeleteSlugs([]);
            });
          }}
        />
      ) : null}
    </>
  );
}
