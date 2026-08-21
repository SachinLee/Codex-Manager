# 上游容量重试与耗尽终态实施计划

## Preconditions

- 已确认 `Retry-After` 最大等待约 2 秒；超过上限回退全抖动。
- 此计划覆盖同一条服务端数据流中的容量恢复和路由耗尽终态；不拆 child task，因为二者共享同一终态原因与集成验证。
- 在用户审阅并显式批准本版计划前，不运行 `task.py start`，不修改产品代码。

## Ordered Changes

1. 先写失败回归：Aggregate API 与账号池的 JSON/纯文本/早期 SSE 精确容量错误；断言首次请求加两次同候选重放、成功恢复、deadline 和已交付流不重放。
2. 在 `gateway/upstream/support/` 复用/扩展退避与 deadline 工具，提供可测试的 `Retry-After` 延迟决策；仅接受不超过约 2 秒的值。
3. 让 Aggregate API 的 HTTP 与 early-SSE 容量恢复都使用该决策；预算耗尽设置专有容量终态 `502`，并阻止该原因进入 Aggregate 候选或模型回退。
4. 将账号池容量预算从一次立即重放改为两次共享延迟重放；`response_finalize` 在预算耗尽时以 `502` 结束，而非直透上游 `429`/`503`。
5. 在 `proxy.rs` 仅为确认无可继续候选/模型回退的路由不可用终态归一化为 `502`：空账号池、账号耗尽、混合路径两侧耗尽、Aggregate 无匹配/冷却/零余额。显式 `model_not_found`、鉴权、请求错误和日限额保持原状态。
6. 为每条新终态添加客户端响应断言：状态 `502`、`error.type=server_error`、既有脱敏诊断及单次响应；为普通 `4xx`/日限额增加反向断言。
7. 检查容量事件、等待、重放、deadline 和耗尽的指标/trace 仍完整；运行窄测试、格式检查和相关服务测试，不触及工作区无关修改。

## Target Areas

- `crates/service/src/gateway/upstream/support/`
- `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`
- `crates/service/src/gateway/upstream/proxy_pipeline/candidate_executor.rs`
- `crates/service/src/gateway/upstream/proxy_pipeline/response_finalize.rs`
- `crates/service/src/gateway/upstream/proxy.rs`
- 相邻单元/集成测试，以及仅在覆盖需要时的 trace/metrics 测试

## Verification

```powershell
cargo fmt --check
cargo test -p codexmanager-service aggregate_api
cargo test -p codexmanager-service gateway
cargo test --workspace
```

每个新增路径测试必须断言 mock upstream 的真实请求次数和客户端最终 HTTP 状态，不能只验证预算函数或日志字符串。

## Risk Controls

- 已向客户端输出业务内容后绝不重放。
- 容量错误绝不借候选、账号或模型切换掩盖；普通非容量失败保留既有 failover。
- 容量、reasoning guard、传输重试和日限额预算保持独立。
- 仅在明确路由耗尽的终点归一化 `502`，不得覆盖鉴权、用户输入或显式配额错误。
- 通过一次终态响应消除网关侧重复；外部客户端重试行为以实际客户端验证为准。
