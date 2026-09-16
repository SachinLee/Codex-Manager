# 交付结果

## 交付
- 状态：complete
- 概述：请求日志页已从“类型选择 + 单输入框”改为标题、模型、平台秘钥三个独立筛选控件；列表、分页总数与筛选汇总走同一服务端 AND 条件。右侧统计新增当前筛选结果缓存率卡片。
- 兼容性：保留旧版 `/logs?query=` 的服务端结构化查询书签（如 `model:gpt-5-codex`、`key:`、`session_in:`、`status:`）；它们不再被误解释为标题并产生零结果。纯文本 `query` 按新产品语义作为标题筛选。

## 验收标准
| 标准 | 结果 | 证据 |
| --- | --- | --- |
| AC-001 | PASS | Playwright `request logs expose independent title, model and api-key filters` 证明标题、模型、平台秘钥独立参数及组合 AND 传递；`legacy structured query bookmarks remain server-side filters` 证明旧结构化书签仍走 `query`。`cargo test -p codexmanager-core request_log` 33 passed；`cargo test -p codexmanager-service requestlog` 52 passed。 |
| AC-002 | PASS | Playwright `request log summary shows cache rate for the filtered result` 断言 `30 / 120 = 25%`；`request log cache rate falls back to dash without input tokens` 断言零输入显示 `-`。 |

## 实现
- 前端：`apps/src/app/logs/page.tsx`、`page-sections.tsx`、`page-helpers.tsx`、`page-cells.tsx`。标题经 session title lookup 转 `sessionIds`；模型目录来自 `apikey/managedModelListV2` 并回落 startup snapshot；平台秘钥来自 `apikey/list`；React Query key 纳入全部显式筛选值与 legacy query。
- Wire：`apps/src/lib/api/service-client.ts`、`normalize.ts`、`types/request-log.ts`、`apps/src-tauri/src/commands/requestlog.rs` 贯通 `sessionIds`/`model`/`keyId` 与输入/缓存 Token 汇总字段。
- 后端：`RequestLogQueryFilters` 与 `*_with_filters` storage variants 让 query、状态、时间、session IDs、model、key ID 在同一 WHERE 条件 AND 组合；非管理员仍先按拥有的 key 集合限制。
- 汇总：SQL 聚合 `input_tokens` 和 `cached_input_tokens`（只计 `usage_included=1`），前端复用 `formatCacheRate`，输入为零显示 `-`。
- 与 design.md 的偏差：独立 Playwright 文件 `apps/tests/request-logs-filters.spec.ts` 承担新交互回归，既有 duration spec 保持回归基线；保留原位置参数 storage 入口并新增 `*_with_filters`，避免无关调用方迁移。

## TDD 证据
- RED：NOT CAPTURED。实施前未保留独立失败输出。
- GREEN：`pnpm exec playwright test tests/request-logs-filters.spec.ts tests/request-logs-duration.spec.ts --reporter=line` → 5 passed；其中含新增旧书签兼容回归。

## 验证
- PASS — `cargo test -p codexmanager-core request_log`：33 passed。
- PASS — `cargo test -p codexmanager-service requestlog`：52 passed。
- PASS — `pnpm -C apps run build:desktop`：Next static export 成功，包含 `/logs`。
- PASS — `pnpm exec playwright test tests/request-logs-filters.spec.ts tests/request-logs-duration.spec.ts --reporter=line`：5 passed。
- PASS — `node --test tests/request-logs-layout.test.mjs`：6 passed。
- PARTIAL — `cargo test --workspace`：除 `codexmanager-service --test app_settings` 一项外均通过；失败为 `app_settings_roundtrip_gateway_user_agent_and_validates_header_value`，断言 `gatewayUserAgent` 期望 `""`、实际 `null`。该项属于工作区既有、独立的 `crates/service/src/app_settings/` gateway-user-agent 修改，不涉及本任务文件。
- NOT RUN — 完整 `pnpm -C apps run test:runtime`；已知其会被无关的 `tests/aggregate-api-zero-balance.spec.ts:141` TypeScript 错误阻断。`npx tsc --noEmit` 对请求日志页面无报错，仅报该无关错误。

## 独立复核
- NOT CAPTURED：已启动只读 fresh-context reviewer；用户要求接管后，reviewer 未在两分钟内交付并被取消，未将其未完成内容作为结论或证据。
- 已自行复核并修复：旧结构化 `/logs?query=` 会在新标题筛选中被误作文本并返回零结果；现在使用已有服务端 query parser 直通，并由 Playwright 回归覆盖。

## 提交
- PASS — 功能提交 `58b7e862 feat(logs): add independent filters and cache rate` 已推送（`df17b58a..58b7e862`）。
- PASS — 兼容性/布局提交 `39e321aa fix(logs): improve filter and metrics layout` 已推送（`0f1e7fc2..39e321aa`）。

## 剩余风险
- 全工作区验证仍被独立 gateway-user-agent 的 `app_settings` 测试失败阻断；升级触发：其所属改动修复后，重新运行 `cargo test --workspace`。
- `test:runtime` / 全局 `tsc` 仍被独立 aggregate-api fixture 类型错误阻断；升级触发：该 fixture 修复后，运行 `pnpm -C apps run test:runtime`。
- legacy structured query 仍可由 URL 使用，但新 UI 不重新暴露旧复合查询输入；这是已批准的控件替换边界。

## 验收
- 状态：待用户验收
- 验证清单：
  1. 打开 `/logs`，展开筛选，分别设置标题、模型、平台秘钥；确认请求结果与右侧统计同步收窄。
  2. 用任意有缓存输入 Token 的筛选结果确认“缓存率”为 `缓存输入 / 输入`；筛到输入 Token 为零时确认显示 `-`。
  3. 打开 `/logs?query=model:gpt-5-codex`，确认不会落入标题零结果，网络/RPC 参数保留 `query: "model:gpt-5-codex"`。
  4. 可复跑：`pnpm -C apps exec playwright test tests/request-logs-filters.spec.ts tests/request-logs-duration.spec.ts --reporter=line`。
- 未运行的检查：`pnpm -C apps run test:runtime`（无关 aggregate-api 类型错误）；完整 workspace 测试未全绿（无关 gateway-user-agent `app_settings` 失败）。

## 复验轮次 1（2026-09-16）

用户在实际界面中反馈三项可见问题：筛选控件缺少字段说明、平台密钥选中态只显示难以识别的 ID、统计卡片在右侧形成三行。任务恢复到 `in_progress`，本轮把筛选区改为有字段标签与用途说明的紧凑表单，明确未命名密钥，统计区固定为两列最多两行。

### 结果
- PASS — 筛选区域重构为带字段名与用途说明的三项紧凑控件：标题说明匹配会话标题、父标题或会话 ID；模型说明按平台模型筛选；平台密钥说明按平台密钥筛选。
- PASS — 平台密钥选中态以 `名称/供应商或模型上下文 · 短 ID` 回显；无名称时明确显示“未命名密钥 · 短 ID”，不再直接显示难以辨识的完整 ID。
- PASS — 统计区域固定为 2×2 网格：当前结果、累计 Token、累计费用、缓存率，每个卡片一格，最多两行。

### 复验
- PASS — `pnpm -C apps run build:desktop`：静态导出成功，包含 `/logs`。
- PASS — `pnpm -C apps exec playwright test tests/request-logs-filters.spec.ts tests/request-logs-duration.spec.ts --reporter=line`：5 passed；新增断言覆盖三项筛选用途说明、选中密钥名称回显和四卡两列统计网格。
- PARTIAL — `npx tsc --noEmit`：本轮文件无报错；仍仅有无关的 `tests/aggregate-api-zero-balance.spec.ts:141` 类型错误。
- NOT CAPTURED — 普通静态 HTTP 服务下 `/logs` 按设计显示 Web 运行方式不受支持，无法替代真实 service-mode 视觉截图；Playwright mock runtime 已验证实际页面 DOM、交互和布局结构。

## 复验轮次 2（2026-09-16）

用户反馈右侧统计在 1568px 宽度下过挤，要求左侧收窄、右侧加宽以完整显示数据。

### 结果
- PASS — 右侧统计面板桌面断点由 `390px` 扩宽至 `480px`（`apps/src/app/logs/page-sections.tsx:195`），左侧筛选区相应收窄；2×2 统计卡获得更充分宽度，Token、费用和缓存率说明不再在 1568px 宽度下拥挤截断。

### 复验
- PASS — `pnpm -C apps exec playwright test tests/request-logs-filters.spec.ts --reporter=line`：4 passed。
- PASS — 上述两轮改动随提交 `39e321aa` 推送至 `origin/codex/integrate-main-20260717`。
