# 实施前提

本任务当前仅完成需求澄清与技术设计，等待用户批准后进入 `in_progress`。以下切片是批准后的执行顺序。

### 切片 1：AC-001 - 独立筛选控件和服务端组合过滤

- 行为：页面直接展示标题输入框、模型下拉框、平台秘钥下拉框；三者可单独或组合筛选，列表和统计一致。
- 代码边界：`apps/src/app/logs/page.tsx`、`apps/src/app/logs/page-sections.tsx`、`apps/src/app/logs/page-helpers.tsx`、`apps/src/lib/api/service-client.ts`、`apps/src/types/request-log.ts`、`crates/core/src/rpc/types.rs`、`crates/core/src/storage/request_log_filters.rs`、`crates/core/src/storage/request_logs.rs`、`crates/core/src/storage/request_log_query.rs`、`crates/service/src/requestlog/requestlog_list.rs`、相关测试。
- 测试接缝：RPC 请求体中的 `sessionIds`/`model`/`keyId` 与渲染出的结果；storage/service 的 AND 过滤测试。
- RED：先把 Playwright fixture 改为识别新字段并断言控件存在、组合值传出；运行 `pnpm -C apps exec playwright test tests/request-logs-duration.spec.ts`，预期新控件/参数断言失败。先增加 Rust 组合参数测试，运行 `cargo test -p codexmanager-core request_log`，预期新 API/行为未实现而失败。
- 实现：扩展显式请求字段并贯通 storage filter；前端读取 startup/API key/model 目录，标题复用 session title lookup，React Query key 包含三字段。
- GREEN：`pnpm -C apps exec playwright test tests/request-logs-duration.spec.ts`；`cargo test -p codexmanager-core request_log`；`cargo test -p codexmanager-service requestlog`。
- 验证：`pnpm -C apps run build`；若包含静态导出路径则 `pnpm -C apps run build:desktop`；检查 Web RPC mapping 透传新 camelCase 字段。
- 依赖：无；用户批准设计后开始。
- 回滚：回退显式字段和前端三个控件，保留旧 `query` contract；无数据库回滚。

### 切片 2：AC-002 - 汇总缓存率和统计卡片

- 行为：右侧统计模块显示当前筛选结果的输入缓存率；输入 Token 为零显示 `-`，现有请求数、Token、费用不变。
- 代码边界：`crates/core/src/storage/request_logs.rs`、`crates/core/src/rpc/types.rs`、`crates/service/src/requestlog/requestlog_summary.rs`、`apps/src/types/request-log.ts`、`apps/src/lib/api/normalize.ts`、`apps/src/app/logs/page.tsx`、`apps/src/app/logs/page-sections.tsx`、测试 fixture。
- 测试接缝：summary RPC 返回 `inputTokens=120,cachedInputTokens=30` 时页面显示 `25%`；零输入时显示 `-`；Rust summary 聚合值与筛选条件一致。
- RED：扩展 fixture 返回输入/缓存 Token并断言统计卡片；运行 `pnpm -C apps exec playwright test tests/request-logs-duration.spec.ts`，预期“缓存率”卡片断言失败。新增 summary 聚合测试，运行 `cargo test -p codexmanager-core request_log`，预期字段尚不存在而失败。
- 实现：summary SQL 增加聚合列，扩展 wire/type/normalize/map；卡片使用现有 `formatCacheRate`，清空日志缓存同步归零。
- GREEN：`pnpm -C apps exec playwright test tests/request-logs-duration.spec.ts`；`cargo test -p codexmanager-core request_log`；`cargo test -p codexmanager-service requestlog`。
- 验证：`pnpm -C apps run build`，并检查多语言 key 已存在或按仓库 i18n 约定补齐。
- 依赖：切片 1 的显式筛选 contract；summary 必须与同一过滤器复用。
- 回滚：回退 summary 新字段与卡片；旧列表和既有统计 contract 可继续工作。

# 上下文包

完整上下文、定位结论、约束和验证命令见 `context.md`；实施派发必须复制对应切片上下文，不得让 worker 重新扫描任务目录。

# 实施记录（2026-09-16）

## 批准与进入实现

用户在会话中回复“开始实施”，视为对本设计的批准；`STATUS` 由 `planning` 迁移到 `in_progress`。

## 与计划的偏差

- 切片 1/2 的 RED 命令原计划复用 `tests/request-logs-duration.spec.ts`；实际改为新建独立 spec `tests/request-logs-filters.spec.ts`，避免把新交互断言塞进既有的“会话标题/用时”用例，duration spec 保持原样作为回归基线。
- 计划外新增：`apps/src-tauri/src/commands/requestlog.rs` 三个命令补 `session_ids`/`model`/`key_id` 透传；`crates/core/src/storage/mod.rs` 抽出 `RequestLogQueryFilters` 结构体并把 `RequestLogQuerySummary` 改为 `Default`，避免 8 个位置参数在多处调用点扩散。
- storage 采用“保留原位置参数入口 + 新增 `*_with_filters` 变体”的方式：dashboard 等既有调用者零改动，日志页走新变体。
- 标题“有输入但无匹配”沿用前端哨兵 `__no_match__` 作为唯一 session id，服务端 `session_id IN (?)` 自然返回零结果，不需要服务端特判。
- 模型下拉数据源：优先实时 `apikey/managedModelListV2`，`placeholderData` 回落到 `StartupSnapshot.apiModels`；平台密钥下拉取 `apikey/list`。

## 验证执行

- `cargo test -p codexmanager-core request_log`：33 passed。
- `cargo test -p codexmanager-service requestlog`：52 passed（含新增 `explicit_filters_keep_page_and_summary_on_same_result_set`）。
- `pnpm -C apps run build:desktop`：静态导出成功。
- `pnpm -C apps exec playwright test tests/request-logs-filters.spec.ts tests/request-logs-duration.spec.ts`：4 passed。
- `npx tsc --noEmit`：仅 `tests/aggregate-api-zero-balance.spec.ts` 一处预存在错误（非本次改动文件）。
- `cargo test --workspace`：见 outcome.md 记录。
