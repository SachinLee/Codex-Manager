# 读取清单

- `.workflow/tasks/09-16-request-log-filter-cache-rate/prd.md` — 已确认需求、口径和 AC。
- `.workflow/tasks/09-16-request-log-filter-cache-rate/design.md` — 已选技术方案与接口边界。
- `apps/src/app/logs/page.tsx` — 请求日志状态、React Query 参数与 startup snapshot。
- `apps/src/app/logs/page-sections.tsx` — 筛选控件、统计卡片和组件 props。
- `apps/src/app/logs/page-helpers.tsx` — 标题到 session ID 的既有解析 helper。
- `apps/src/lib/api/service-client.ts` — 请求日志 typed wrapper。
- `apps/src/lib/utils/billing.ts` — `formatCacheRate` 既有缓存率格式化口径。
- `apps/src/types/request-log.ts` — 前端日志汇总类型。
- `apps/src/types/startup.ts`、`apps/src/types/model.ts`、`apps/src/types/api-key.ts` — 模型目录、API key 与启动快照数据源。
- `apps/tests/request-logs-duration.spec.ts` — 现有请求日志 Playwright fixture 和缓存率表格断言。
- `crates/core/src/rpc/types.rs` — RPC 请求参数与汇总返回类型。
- `crates/core/src/storage/request_log_filters.rs` — 统一 WHERE 条件构造。
- `crates/core/src/storage/request_log_query.rs` — 现有 query 语法和兼容行为。
- `crates/core/src/storage/request_logs.rs` — 列表、计数和 summary SQL。
- `crates/core/src/storage/tests/request_logs_tests.rs` — storage 行为测试。
- `crates/service/src/requestlog/requestlog_list.rs`、`requestlog_summary.rs` — service 参数归一化与 summary 映射。
- `crates/service/src/requestlog/requestlog_list_tests.rs`、`requestlog_summary_tests.rs` — service 测试模式。

# 已定位结论

- `crates/service/src/rpc_dispatch/requestlog.rs:37-105` 直接用 `serde_json::from_value::<RequestLogListParams>` 反序列化并复用同一 params；无需手工映射，但三条 RPC 路径都必须同步验证。
- `apps/src/app/logs/page.tsx:142-149` 已加载 API keys；`page.tsx:103-113` 已有 startup API keys；`StartupSnapshot.apiModels` 提供平台模型目录。
- `apps/src/app/logs/page-sections.tsx:232-280` 是旧复合搜索控件；`page-sections.tsx:389-439` 是右侧统计卡片。
- `apps/src/lib/utils/billing.ts:41-55` 已定义 `formatCacheRate(input,cached)`：输入为零显示 `-`，缓存值限制到输入范围。
- `crates/core/src/storage/request_log_filters.rs:12-46` 负责统一组合条件；显式标题 session IDs、model、key ID 应在此处追加 AND 条件。
- `crates/core/src/storage/request_logs.rs:1217-1249` 的 summary SQL 当前只选 count/success/error/total_tokens/cost，需要追加 `SUM(t.input_tokens)` 与 `SUM(t.cached_input_tokens)`。
- `crates/core/src/rpc/types.rs:1783-1901` 是 `RequestLogListParams` 与 summary wire contract；默认 serde 保证旧字段兼容。

# 不变量

- 旧 `query` 调用行为保持；新显式字段为空时不加条件。
- 标题空输入不筛选；标题有输入但无 session match 必须返回零结果。
- 三个新字段、状态、时间、旧 query 共同 AND；列表/分页 total/summary 必须同口径。
- 缓存率严格为 `cached_input_tokens / input_tokens`，不含 cache write；输入为零显示 `-`。
- 平台 key 只显示名称/紧凑 ID，不显示 secret。

# 实施切片

### 切片 1：AC-001 - 独立筛选控件和服务端组合过滤

- 前端移除复合字段选择器，加入标题 Input、模型 Select、平台秘钥 Select；查询 key 和 typed wrapper 改传显式字段。
- Rust 扩展 `RequestLogListParams`、service normalization、storage filter builder，使三字段与旧 query AND 组合。
- 需要同步 list、list_with_summary、summary 的调用链；不改数据库。
- 验证：`pnpm -C apps exec playwright test tests/request-logs-duration.spec.ts`；`cargo test -p codexmanager-core request_log`；`cargo test -p codexmanager-service requestlog`。

### 切片 2：AC-002 - 汇总缓存率和统计卡片

- Rust summary SQL/映射返回输入与缓存输入 Token；前端类型 normalize 与卡片展示缓存率。
- 清空日志 placeholder summary 同步归零新字段；旧统计保持。
- 验证：切片 1 命令加上 summary/storage 测试；同一 Playwright fixture 断言卡片值和 detail。

# 实施约束

- 这是 standard 前端 + Rust service 行为变更；实现前须用户批准本设计。
- 若进入实现，按切片顺序推进；每切片先写能失败的行为测试/fixture，再实现最小改动。
- 不新增数据库迁移、依赖、全局筛选状态或 secret 展示。
