# 实施计划：Aggregate SSE 502 候选切换修复

## 实施约束

- 当前仍是 planning；本文件完成前不执行 `task.py start`。
- 质量等级：critical。
- 不改 `.omp` 配置、OMP retry/fallback、非 strict-guard 实时直吐路径或账号池策略。
- 不新增冷却参数；沿用 5 次失败 / 5 分钟冷却。
- 每个 slice 完成后只运行其 focused verification；跳过格式化器、全仓 lint 和全仓测试，统一在最终验证阶段执行。
- 子代理只能修改该 slice 的 Allowed files；不得修改 task 状态文件或其他 slice 文件。

### Slice 1：AC-001/AC-002/AC-003 - 修复 strict-guard 零交付判定

- Behavior：Aggregate `/v1/responses` 在 HTTP 200、SSE 终态失败且客户端尚未收到内容时，把 request 归还候选循环；`output_text` 已出现时继续只交付原 buffered stream，不重放。
- Code boundary：
  - `crates/service/src/gateway/observability/http_bridge/mod.rs`
  - `crates/service/src/gateway/observability/http_bridge/delivery.rs`
  - `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`
  - `crates/service/src/gateway/observability/http_bridge/stream_readers/common.rs`
  - `crates/service/src/gateway/observability/http_bridge/stream_readers/openai_responses.rs`
  - `crates/service/src/gateway/observability/http_bridge/aggregate/openai_responses_event.rs`（仅在需要补齐现有语义内容信号时）
- Test seam：`aggregate_api_tests.rs` 的 blocking SSE scenario harness；使用 R1b 快速前置 event + `response.failed`，避免固定等待 10 秒。
- RED：新增/改造 `gateway_concurrency_limit`、`server_error`（含 type variant）、`rate_limit_exceeded` 的 zero-delivery fixtures；当前预期出现单候选 502 或多一次错误 candidate attempt。
- Implementation：
  1. 为 bridge chain 增加独立 `allow_zero_delivery_failover` 参数；Aggregate 传 true，`response_finalize.rs` 传 false；不复用 `allow_failover_for_deactivation`。
  2. strict-guard 保留既有 read-error 分支，新增完整 body 中 `collector.terminal_error` 的 Aggregate-only 分支；要求 runtime kill switch、status 200、无 output text、`output_tokens == 0`。
  3. 仍在调用 `respond_streaming_chunked` 之前返回 pending request；保持 stream terminal error 事实，使 Aggregate 的既有 SSE decision block 可分类。
  4. 如测试证明 tool-call-only body 会绕过 R-004，给 collector 增加单一语义内容标志，并由 Responses event model 设置；不得在 delivery 层复制 SSE 解析规则。
  5. 修复 SSE decision 的 `RequestTerminal` arm：take pending request、release spend reservation，再走原有终态响应。
- GREEN：`cargo test -p codexmanager-service --lib aggregate_sse`；至少覆盖 A→B、A output text→terminal no-replay、invalid request terminal。
- Validation：检查 bridge diagnostics、client response body、upstream hit count；确认 account pool opt-in=false 的 targeted test 未改变。
- Dependencies：无；R-007 已决定，不新增配置。
- Rollback：设置 `CODEXMANAGER_AGGREGATE_ZERO_DELIVERY_FAILOVER=false` 可立即关闭新增 gate；代码可独立回滚。

### Slice 2：AC-002/AC-004/AC-005 - 补齐失败分类与回归契约

- Behavior：`server_error`、`service_unavailable_error`、`gateway_concurrency_limit` 与已有 `rate_limit_exceeded` 进入 `CandidateFailover`；capacity、capability、reasoning guard、invalid request 优先级不变。
- Code boundary：
  - `crates/service/src/gateway/upstream/support/upstream_failure.rs`
  - `crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs`
  - `crates/service/src/gateway/upstream/support/upstream_failure.rs` tests
- Test seam：classifier unit tests + aggregate SSE integration tests。
- RED：先增加新 code/type fixtures；验证当前 `server_error` 仍 retry same candidate、`gateway_concurrency_limit` 未按期望直接切换。
- Implementation：
  1. 扩展 central classifier 的 candidate-failover code 集合，保持 capacity/capability checks 在前。
  2. 让 terminal string code extraction 在没有 `code=` 时回退到 leading `type=`；JSON 路径继续复用既有 code→type 规则。
  3. 将旧 `aggregate_sse_server_error_retries_same_candidate_once` 改为验证新的直接 candidate failover 契约；同步全候选失败的 hit-count 断言。
  4. 不修改账号池的 `should_failover_terminal_gateway_error`；不改变 preflight 的 metadata/commit 时机。
- GREEN：`cargo test -p codexmanager-service --lib aggregate_sse` 与 upstream failure focused tests。
- Validation：确认 `selected model is at capacity...` 仍走 CapacityRecovery，`invalid_request_error` 仍 terminal，未知 5xx 仍按 transport budget 处理。
- Dependencies：Slice 1 的 integration fixtures 可先落盘，但 classifier 可独立实现。
- Rollback：回滚 classifier/test 改动；不涉及 schema。

### Slice 3：AC-006 - 保持并验证既有冷却策略

- Behavior：新候选失败继续进入现有 cooldown counter；第一次失败不冷却，第五次连续失败进入 5 分钟冷却；单次请求不会把全部 Terra candidates 置为 cooling down。
- Code boundary：
  - `crates/service/src/gateway/routing/tests/aggregate_api_cooldown_tests.rs`
  - 必要时 `crates/service/src/gateway/routing/aggregate_api_cooldown.rs`（仅为测试 seam，不改阈值）
  - `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`（仅核对/修正 cooldown_eligible_failure 生命周期）
- Test seam：cooldown runtime status / existing test reset helper；必要时 Aggregate integration 查询 candidate status。
- RED：增加 1 vs 5 failure boundary assertion，当前无本任务专属可观测断言。
- Implementation：确认 `CandidateFailover`、最终 retry exhaustion 与 existing health-neutral 分支的 `cooldown_eligible_failure` 语义；不新增首击短冷却、权重或环境变量。
- GREEN：相关 cooldown unit tests；Aggregate scenario 验证单次 A→B sweep 后没有 all-cooling 503。
- Validation：断言常量仍为 threshold=5、cooldown=300s；验证按 `(api_id, upstream_model)` 隔离。
- Dependencies：Slice 2 的 CandidateFailover 分类。
- Rollback：只回滚新增测试或 cooldown eligibility 调整；保留既有机制。

### Slice 4：AC-007 - 增加逐候选 trace 观测

- Behavior：每个 Aggregate candidate failure/decision 产生可检索的 `AGGREGATE_CANDIDATE_DECISION` 事件，包含 `trace_id`、candidate id、1-based position、sanitized upstream URL、status、error code、decision、zero_delivery、retry ordinal；不包含敏感载荷。
- Code boundary：
  - `crates/service/src/gateway/observability/trace_log.rs`
  - `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`
  - `crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs`
  - `crates/service/src/gateway/observability/trace_log_tests.rs`（如现有测试模块适合承载纯格式化测试）
- Test seam：纯 line formatter + trace buffer test hook；Aggregate A→B integration fixture。
- RED：对 AC-001 场景断言缺少 aggregate candidate event，或 event 缺少 required fields。
- Implementation：
  1. 在 trace_log 增加结构化 helper，复用 `sanitize_text`、URL 脱敏和现有 trace buffer convention。
  2. 在 Aggregate preflight、非 2xx、post-bridge SSE decision 位置记录实际 decision；retry 记录 ordinal。
  3. 记录 bounded code，不记录 body、raw SSE、Authorization、tool args 或 secret。
  4. 确认恢复成功的 request 也能保留 candidate failure event；若 trace flush 依赖 error 标志，增加最小、明确的保留条件。
- GREEN：trace formatter/trace buffer focused tests + Aggregate SSE integration test。
- Validation：读取实际 trace output，确认 event 可见且敏感字段不存在。
- Dependencies：Slice 1/2 提供 decision 与 zero-delivery 事实。
- Rollback：删除 helper/call sites；不影响 routing。

### Slice 5：AC-008 - 持久化 Aggregate attempt records

- Behavior：多候选请求结束后，`request_logs.aggregate_api_attempts` 可查询，数组包含每个候选的 `api_id`、position、outcome、failure_category；已有 `attempted_aggregate_api_ids_json` 保持兼容。
- Code boundary：
  - `crates/core/migrations/140_request_logs_aggregate_api_attempts.sql`
  - `crates/core/src/storage/mod.rs`
  - `crates/core/src/storage/request_logs.rs`
  - `crates/service/src/gateway/observability/request_log.rs`
  - `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`
  - `crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs`
- Test seam：core storage migration/insert/list tests + Aggregate integration query。
- RED：当前 live DB 无 `aggregate_api_attempts` column，且 `Responded { .. }` records 未持久化。
- Implementation：
  1. 注册既有 migration 140，并添加 `ensure_request_log_aggregate_api_attempts_column` compat path。
  2. 同步 `RequestLog`、request-log select/row mapper、普通 INSERT、token-stat INSERT、legacy compaction schema/INSERT。
  3. 扩展 `RequestLogTraceContext` 传递预序列化 attempts JSON。
  4. 为 outcome/record 添加稳定 serde 表示；Aggregate 写 log 前纳入最终候选 record，避免 success path 漏掉最后一项。
  5. 失败终态和成功桥接分别验证完整数组；`RequestReleased` 不提前写最终 request outcome。
- GREEN：core request-log tests + Aggregate persistence integration test。
- Validation：使用 SQL 查询 JSON，验证每个 candidate 的 `failure_category` 与 `outcome`；验证旧数据库初始化/已有 migration 不重复执行。
- Dependencies：Slices 1–4 的 final attempt semantics；不依赖前端改动。
- Rollback：保留 additive column；代码回滚只停止写入/读取，不做 destructive migration。

### Slice 6：规范文档与最终回归 - AC-009

- Behavior：现有零交付开关的语义可发现；账号池路径、既有 transport/capacity/capability 测试保持通过。
- Code boundary：
  - `docs/en/report/environment-and-runtime-config.md`
  - `docs/zh-CN/report/环境变量与运行配置说明.md`
  - `docs/ru/report/Переменные-среды-и-конфигурация-запуска.md`
  - `docs/ko/report/환경변수-및-실행-설정-안내.md`
  - 不修改产品代码。
- Test seam：文档检索 + final targeted/full Rust validation。
- RED：当前 `CODEXMANAGER_AGGREGATE_ZERO_DELIVERY_FAILOVER` 未在环境变量文档中说明。
- Implementation：说明默认 true、仅控制 Aggregate strict-guard zero-delivery failover、不是 retry budget/冷却策略开关。
- GREEN：文档一致性检查；账号池 targeted tests。
- Validation：按仓库规则运行 `cargo test --workspace` 或记录无法运行的准确原因；运行 `cargo test -p codexmanager-service --lib aggregate_sse` 与 core storage tests。
- Dependencies：Slices 1–5 完成。
- Rollback：文档可独立回滚；运行时开关提供行为止损。

## Final quality gate

1. `prd.md`、`design.md`、`implement.md` 的 AC 映射完整且无 open question。
2. `cargo test -p codexmanager-service --lib aggregate_sse` 通过。
3. classifier、cooldown、trace-log、core request-log targeted tests 通过。
4. 账号池相关 targeted tests 与 workspace Rust validation 通过，或记录确切环境阻塞。
5. 检查 migration 140 在新库和旧库上的幂等行为。
6. 检查日志与持久化字段不泄露 body、SSE 原文、Authorization、tool 参数、secret。
7. 实现后运行独立 implementation review，再进入 finish/evidence 阶段。
