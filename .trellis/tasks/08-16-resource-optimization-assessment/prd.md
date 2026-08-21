# 评估 Codex Manager 内存与 CPU 占用优化

## Goal

识别 Codex Manager 在用户电脑上可实际降低的内存和 CPU 占用，并提供按预期收益、实现成本、回归风险与验证方法排序的优化方案；本任务只做证据驱动的评估，不修改产品代码。

## Confirmed Facts

- 用户主要关注内存与 CPU 占用。
- 仓库同时包含 Next.js/Tauri 桌面端、Rust 本地服务和服务模式 Web UI；评估必须区分这些运行形态，不能把构建期开销当作用户运行期开销。

## Technical Evidence

- 桌面端在 `AppBootstrap` 中启动并连接本地服务；服务启动会先启动用量轮询、网关保活、令牌刷新、预热 cron、聚合 API 健康检查和插件调度器，再开始 HTTP 服务（`apps/src/components/layout/app-bootstrap.tsx:302-318`；`crates/service/src/lifecycle/startup.rs:69-82`）。
- 默认后台配置已启用：用量轮询每 600 秒、网关保活每 180 秒、令牌刷新每 60 秒；预热 cron 默认关闭（`apps/src/lib/api/normalize.ts:82-96`）。
- 前端会为每个已打开的顶级页面保留挂载的隐藏 DOM 与 React 状态；已访问的页面不会卸载（`apps/src/components/layout/page-keep-alive-viewport.tsx:173-195`）。
- Windows 具备“关闭到托盘后销毁界面、保留后台服务”的低资源模式，但需开启关闭到托盘；“低透明度”也可禁用大量 WebView2 背景模糊（`apps/src-tauri/src/commands/settings/tray_state.rs:68-106`；`apps/src/app/globals.css:479-527`）。
- 本轮尚未启动隔离的桌面进程，因此不存在可归因于具体路径的实测 MB、CPU% 或耗电结论；后续方案必须先建立基线。

## Requirements

- 追踪启动、空闲、网关请求、后台轮询/同步、缓存与窗口生命周期中可能持续占用 CPU 或持有内存的路径。
- 检查前端包体与运行时依赖、Tauri/WebView 生命周期、Rust 服务任务与缓存，以及默认配置。
- 对每个候选项说明证据位置、预期资源收益、适用运行形态、实现复杂度、回归风险及可观测验证方法。
- 区分无需代码修改的用户/配置建议与需要产品改动的建议。

## Acceptance Criteria

- [x] 输出覆盖桌面端与服务模式边界的资源路径盘点，且每项均有代码或配置证据。
- [x] 输出按优先级排序的优化清单，包含原因、预期收益区间或不确定性、成本、风险和验证步骤。
- [x] 输出可复现的 Windows 基线测量方案，分别覆盖启动、空闲和至少一个活跃请求场景。
- [x] 明确不建议优化或证据不足的方向，避免以功能/可靠性换取未经验证的资源节省。

## Out of Scope

- 不修改产品代码、构建配置或默认设置。
- 不基于未经测量的猜测宣称具体节省数值。

## Scope Decision

以 Windows 桌面端为主：Tauri 主进程、WebView2、桌面端启动的 Rust 本地服务和它们的后台任务都在评估范围内。服务模式 Web UI 只记录与桌面端不同的资源边界，不单独形成优化目标或基线。
