"use client";

import { normalizeRoutePath } from "@/lib/utils/static-routes";

export const TOP_LEVEL_ROUTE_CONFIG = [
  { path: "/", label: "仪表盘" },
  { path: "/accounts", label: "账号管理" },
  { path: "/aggregate-api", label: "聚合API" },
  { path: "/apikeys", label: "平台密钥" },
  { path: "/models", label: "模型管理" },
  { path: "/plugins", label: "插件中心" },
  { path: "/codex-launcher", label: "Codex 启动器" },
  { path: "/logs", label: "请求日志" },
  { path: "/settings", label: "设置" },
  { path: "/author", label: "赞助与推荐" },
] as const;

export type TopLevelRoutePath = (typeof TOP_LEVEL_ROUTE_CONFIG)[number]["path"];

const TOP_LEVEL_ROUTE_SET = new Set<TopLevelRoutePath>(
  TOP_LEVEL_ROUTE_CONFIG.map((route) => route.path),
);

export function isTopLevelRoutePath(path: string): path is TopLevelRoutePath {
  return TOP_LEVEL_ROUTE_SET.has(normalizeRoutePath(path) as TopLevelRoutePath);
}

export function toTopLevelRoutePath(path: string): TopLevelRoutePath {
  const normalizedPath = normalizeRoutePath(path);
  if (isTopLevelRoutePath(normalizedPath)) {
    return normalizedPath;
  }
  return "/";
}

export function getTopLevelRouteLabel(path: string): string {
  const normalizedPath = normalizeRoutePath(path);
  return (
    TOP_LEVEL_ROUTE_CONFIG.find((route) => route.path === normalizedPath)?.label ??
    "CodexManager"
  );
}
