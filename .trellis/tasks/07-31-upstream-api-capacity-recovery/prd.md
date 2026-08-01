# 上游 API 容量错误自动恢复

## Goal

当 Aggregate API 上游返回 `Selected model is at capacity. Please try a different model.` 时，CodexManager 应在客户端尚未收到任何响应内容的前提下自动恢复请求，让 Codex 客户端不需要手动发送“继续”。恢复过程必须避免重复可见输出、重复工具调用及不透明的模型降级。

## Confirmed Facts

- 容量错误分类器仅匹配截图中的固定英文文案，同时容忍项目已知的键值前缀和诊断后缀：`crates/service/src/gateway/mod.rs:65`。
- Aggregate API 对 HTTP 非成功响应已有同一上游重放逻辑：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs:1917`。
- Aggregate API 对 HTTP 200、但 SSE 终态携带该容量错误的情况，也能在尚未交付客户端时返回原请求以便重放：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs:2083`。
- 当前容量重试预算为一次，且没有专属退避：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs:55`。项目已有可复用的指数抖动退避工具：`crates/service/src/gateway/upstream/support/backoff.rs`。
- 当前候选循环会在非终态失败后转向下一 Aggregate API：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs:2232`。
- 当前流式容量错误在容量预算耗尽后的终态交付和候选切换行为缺少针对性测试；设计和实现必须显式保证不会静默断开客户端请求。

## Requirements

### R1. Safe Retry Boundary

- 仅当网关尚未向客户端写入有效响应字节时，才允许重放请求。
- HTTP 非成功响应和首包/早期 SSE 容量错误均应使用同一恢复策略。
- 一旦已交付有效输出、工具调用或其他可见事件，不得重放；应保留现有客户端终态语义。
- 每次同一上游重放必须使用原始、不可变的候选请求体，不能累积重写或修改会话输入。

### R2. Capacity-Specific Policy

- 容量错误必须维持窄匹配，不得以通用 `capacity` 文本触发重试。
- 同一上游的容量重试应使用独立预算和带抖动的退避，不得意外消耗通用传输重试预算。
- 容量错误不应标记上游为网络故障、账号额度耗尽或普通健康失败。
- 每个上游总共最多发送 3 次请求：首次请求加 2 次容量重放；两次重放使用全抖动指数退避，最大额外等待不超过约 2 秒。

### R3. Candidate Failover and Client Outcome

- 容量重试只在当前 Aggregate API 候选内执行；容量预算耗尽后不得尝试下一个 Aggregate API 候选。
- 不得为了掩盖容量错误而静默切换到不同能力或不同语义的模型。
- 无可用恢复路径时，必须向客户端返回标准化的容量错误终态；不得丢弃或静默关闭请求。
- 请求日志必须保留所有尝试的 Aggregate API 候选及最终结果。

### R4. Observability

- 区分并记录：容量命中、同上游重试、退避等待与最终失败。
- Prometheus 指标和结构化日志应支持按请求 trace、上游 ID、上游模型和流式/非流式方式排障。
- 不记录 API secret、完整提示词或未脱敏的认证头。

### R5. Compatibility

- 不影响现有账号池容量重试、reasoning guard、能力降级、普通传输重试和 Aggregate API 冷却语义。
- Desktop 与 service-mode Web UI 使用同一网关行为，无需修改 Codex 客户端。

## Acceptance Criteria

- [ ] Aggregate API 以 HTTP 错误返回精确容量文案时，客户端不收到首个失败响应；网关按容量专属预算和退避重放原请求。
- [ ] Aggregate API 以 HTTP 200 SSE 错误事件返回精确容量文案、且尚未交付有效内容时，网关使用同一策略恢复，不向客户端泄漏该错误。
- [ ] 容量预算耗尽后不会再意外消耗通用传输重试预算。
- [ ] 两次容量重放的总额外等待不超过约 2 秒，并且整体请求截止时间优先于任何等待。
- [ ] 已向客户端交付文本、工具调用或其他业务事件后的流式失败不重放，避免重复副作用。
- [ ] 无可用候选时，HTTP 与 SSE 请求均收到明确终态而不是静默断连。
- [ ] 容量预算耗尽后，即使存在其他 Aggregate API 候选，也只向客户端返回标准化容量错误；请求日志中保留当前候选的完整重试链。
- [ ] 错误匹配不会将 `model capacity is temporarily exhausted` 等非精确文案误判为可重试容量错误。
- [ ] 覆盖 HTTP、SSE、恢复成功、重试耗尽、已交付内容、退避与指标的回归测试通过。

## Out of Scope

- 改动 Codex 客户端、模拟其“继续”按钮，或主动注入一条 `Continue` 用户消息。
- 自动降级到其他模型、修改用户模型选择或变更模型定价。
- 对任意 5xx/429 错误启用相同策略。
- 持久化上游 API secret、请求正文或 SSE 原始内容作为恢复状态。

## Resolved Decisions

- 不自动切换 Aggregate API：容量恢复只重放当前上游；若重试后仍失败，直接将容量错误返回客户端。
- 每个上游总共尝试 3 次；两次容量重放使用带抖动的退避，最大额外等待约 2 秒。
