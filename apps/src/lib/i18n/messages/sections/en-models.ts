"use client";

import type { MessageCatalog } from "../types";

export const EN_MODELS_MESSAGES: MessageCatalog = {
  可用模型: "Available models",
  模型管理: "Model management",
  "本地模型目录是唯一运行时真相源；价格、路由和 instructions policy 原子保存。":
    "The local model catalog is the only runtime source of truth; prices, routes, and the instructions policy are saved atomically.",
  重新读取: "Reload",
  "从本地 JSON 导入": "Import local JSON",
  "导出到本地 Codex 缓存": "Export to local Codex cache",
  "导出中...": "Exporting...",
  新增自定义模型: "Add custom model",
  总数: "Total",
  已启用: "Enabled",
  已禁用: "Disabled",
  模型目录明细: "Model catalog details",
  "显示 origin、启用状态、价格状态、instructions mode 和 route 状态。":
    "Shows origin, enabled state, price status, instructions mode, and route status.",
  搜索模型: "Search models",
  全部模型: "All models",
  批量删除模型: "Delete models",
  "服务未连接，模型目录暂不可用。":
    "The service is disconnected, so the model catalog is unavailable.",
  "没有符合条件的模型。": "No models match the current filters.",
  选择全部模型: "Select all models",
  模型: "Model",
  状态: "Status",
  操作: "Actions",
  "选择模型 {slug}": "Select model {slug}",
  "编辑模型 {slug}": "Edit model {slug}",
  "禁用模型 {slug}": "Disable model {slug}",
  "删除模型 {slug}": "Delete model {slug}",
  编辑模型: "Edit model",
  删除模型: "Delete model",
  "Builtin 模型 {slug} 将被禁用，数据不会删除。":
    "Builtin model {slug} will be disabled; its data will not be deleted.",
  "确定要永久删除自定义模型 {slug} 吗？":
    "Permanently delete custom model {slug}?",
  "将处理 {count} 个模型：{builtin} 个 builtin 会被禁用，其余 custom 会被删除。":
    "Process {count} models: {builtin} builtin models will be disabled and the custom models will be deleted.",
  "已删除 {count} 个模型": "Deleted {count} models",
  "批量删除完成：成功{success}个，失败{failed}个":
    "Batch deletion completed: {success} succeeded, {failed} failed",
  批量删除失败: "Batch deletion failed",
  "支持模型目录导出格式和 Codex catalog 格式；所有导入项都会作为自定义模型处理。":
    "Supports model catalog exports and Codex catalog JSON; every imported item becomes a custom model.",
  "本地 JSON 文件": "Local JSON file",
  冲突策略: "Conflict strategy",
  预览导入: "Preview import",
  "处理中...": "Processing...",
  新增: "Added",
  更新: "Updated",
  冲突: "Conflicts",
  跳过: "Skipped",
  错误: "Errors",
  忽略字段: "Ignored fields",
  提交导入: "Commit import",
  "导入中...": "Importing...",
  "请选择或粘贴模型 JSON": "Select or paste model JSON",
  导入预览失败: "Import preview failed",
  导入提交失败: "Import commit failed",
  导入模型: "Import models",
  "已导入 {count} 个模型": "Imported {count} models",
  模型目录为空: "The model catalog is empty",
  "当前服务未返回可用的 Codex CLI 标识":
    "The current service did not return a usable Codex CLI identifier",
  当前环境不支持浏览器导出: "Browser export is unavailable in this environment",
  "当前环境不支持导出 Codex 缓存":
    "Codex cache export is unavailable in this environment",
  "已导出到本地 Codex 缓存": "Exported to the local Codex cache",
  "Codex 缓存已下载，请保存到 `~/.codex/models_cache.json`":
    "Codex cache downloaded. Save it to `~/.codex/models_cache.json`.",
  导出失败: "Export failed",
};
