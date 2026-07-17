# Implement: 请求日志模型使用统计

## Prerequisites

- [x] 用户确认方案 A
- [x] Trellis 任务已创建：`07-17-request-log-model-usage-stats`
- [ ] 用户确认本 prd/design/implement 后执行 `task.py start`
- [ ] 实现前读取：`.trellis/spec/codexmanager-service/backend/`、`apps/AGENTS.md`（若存在）

## Ordered Checklist

### 1. Core storage 聚合

- [x] 在 `crates/core/src/storage/mod.rs` 增加 `RequestLogModelUsageSummary`（或等价命名）
- [x] 在 `request_logs.rs` 实现：
  - `summarize_request_logs_by_model_filtered(...)`
  - `summarize_request_logs_by_model_filtered_for_keys(...)`
- [x] 复用 `build_request_log_filters` / account join / token join 表达式，与 filter summary 对齐
- [x] LIMIT 50 + truncated 标志（在 service 层或 storage 层明确返回）
- [x] 单测：`crates/core/src/storage/tests/request_logs_tests.rs`（或新建专用 test 模块）

**Validate:**

```bash
cargo test -p codexmanager-core request_logs -- --nocapture
```

### 2. RPC types + service 映射

- [x] `crates/core/src/rpc/types.rs`：`RequestLogModelUsageStatResult` + 扩展 `RequestLogFilterSummaryResult`
- [x] 序列化 camelCase 单测（`types_tests.rs`）
- [x] `requestlog_summary`：调用新 storage API，填入 `model_stats` / `model_stats_truncated`
- [x] 确认 `list_with_summary` 复用同一 map 路径
- [x] admin / for_keys 分支均覆盖

**Validate:**

```bash
cargo test -p codexmanager-core request_log_filter_summary -- --nocapture
cargo test -p codexmanager-service request_log -- --nocapture
```

### 3. 前端 API 链路

- [x] `apps/src/types/request-log.ts` 扩展类型
- [x] `normalizeRequestLogFilterSummary` 解析 `modelStats` / `modelStatsTruncated`
- [x] 确认 transport / Tauri 无需新命令名（若仅扩展 payload）；若有独立 command 则同步 registry
- [x] 缺字段默认 `[]` / `false`

**Validate:** 类型检查随 build；必要时补 normalize 单测。

### 4. UI（方案 A）

- [x] 新建 `apps/src/app/logs/model-usage-stats.tsx`
- [x] `page-sections.tsx` 在汇总卡与明细表之间挂载
- [x] 折叠、Top8 默认截断展示、排序、占比条、空态、skeleton
- [x] 脚注：不含 Guard 重试用量
- [x] 可选：点击模型 → `onSearchChange(model)` + 回第一页（`page.tsx` 接线）
- [x] i18n：中文 key + en/ko/ru 对照

**Validate:**

```bash
pnpm -C apps run build
```

### 5. 联调与验收

- [ ] 本地打开请求日志：筛选「今天」→ 有多模型数据时表格正确
- [ ] 切 2XX / 搜索模型名 → 模型表与明细一致
- [ ] 非 admin（若可测）仅见自己 key 数据
- [ ] 清空日志后模型统计同步为空（在日志仍为空的前提下）

### 6. 收尾（实现完成后）

- [ ] `trellis-check` / ECC 风格自检
- [ ] 必要时更新 `.trellis/spec` 中 requestlog 相关说明
- [ ] 不自动 commit，除非用户明确要求

## Review Gates

1. **口径门**：模型表 Token/费用之和在未 truncated 且 Guard 未计入时，与 summary 主口径一致（或文档明确差异仅 Guard）。
2. **权限门**：member 路径必须走 key_ids。
3. **兼容门**：旧 summary JSON 无 modelStats 不崩。
4. **性能门**：默认「今天」筛选下响应可接受；避免在 all-time 无索引场景做额外 N+1。

## Rollback Points

| 节点 | 动作 |
|------|------|
| 仅后端合并 | 前端未展示则无用户影响 |
| 前端展示有问题 | 隐藏 `ModelUsageStatsCard` 挂载 |
| 全回滚 | revert 相关 commit |

## File Touch List (expected)

| Path | Action |
|------|--------|
| `crates/core/src/storage/mod.rs` | UPDATE |
| `crates/core/src/storage/request_logs.rs` | UPDATE |
| `crates/core/src/storage/tests/request_logs_tests.rs` | UPDATE |
| `crates/core/src/rpc/types.rs` | UPDATE |
| `crates/core/src/rpc/tests/types_tests.rs` | UPDATE |
| `crates/service/src/requestlog/requestlog_summary.rs` | UPDATE |
| `crates/service/src/requestlog/*` (list_with_summary path) | UPDATE if needed |
| `apps/src/types/request-log.ts` | UPDATE |
| `apps/src/lib/api/normalize.ts` | UPDATE |
| `apps/src/app/logs/model-usage-stats.tsx` | CREATE |
| `apps/src/app/logs/page-sections.tsx` | UPDATE |
| `apps/src/app/logs/page.tsx` | UPDATE (optional click-to-filter) |
| `apps/src/lib/i18n/messages/**` | UPDATE |

## Out of Scope During Implement

- Guard 按模型分摊
- 独立 Tab / 导出 / 趋势图
- 改 Dashboard topModels


## Validation Record

- `cargo test -p codexmanager-core request_logs_filtered_summary_groups_by_model` — ok
- `cargo test -p codexmanager-core request_log_filter_summary_serialization` — ok
- `cargo test -p codexmanager-service filter_summary` — ok (4 tests)
- `pnpm -C apps run build` — ok (Next.js static export, `/logs` included)
- Manual UI against live service not verified in this environment
