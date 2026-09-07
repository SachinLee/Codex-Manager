# 修复 OMP 请求日志会话关联与标题索引超时

## Goal

恢复本机 OMP 请求日志的会话可追溯性：日志写入有效的 OMP 会话 ID；已知主线程和子线程会话在日志页显示正确标题；标题索引不可阻塞请求日志页或服务 RPC。

## Current behavior and problem

- 实机数据库证据：新桌面包启动后的 `request_logs` 行 `73993`–`73997` 的 `session_id` 与 `conversation_anchor` 均为 `NULL`，请求均为 `/v1/responses`。页面因此没有可用于标题 Map 的 join key。
- 请求 ID 解析只按 `session_id`、`x-codex-parent-thread-id`、请求体字段回退；`IncomingHeaderSnapshot` 已解析 `x-client-request-id`，但 `resolve_request_log_session_id` 未使用它。历史 OMP 实现证据表明该头可承载 OMP session ID；必须以真实当前请求证据及回归测试界定安全 fallback。
- 运行日志在 06:56–06:58 连续记录 `requestlog/sessionTitles` 的 10 秒 RPC read timeout，并继发 `requestlog/list_with_summary` connect timeout。当前标题 RPC 同步枚举 Codex、OMP、Pi 会话目录；首次/过期扫描会在 RPC 请求路径中执行。
- 标题 RPC 在缓存预热后实测 28ms；6 秒后再次测量为 14ms，返回 1,066 条。该结果证明缓存快路径可用，但不否定冷扫描曾阻塞 RPC；冷路径必须可验证且不得拖慢日志页。
- 当前主 OMP 会话 `01a03264-f861-7588-9c02-ef3244d5e5c6` 已被标题 RPC 以 `source=omp` 返回且有标题；所以有有效 `session_id` 时标题 read model 可以匹配。
- 数据库中的 `01a03276-2dbb-7251-aa41-318f20ef4acd` 被当前标题 RPC 标为 `source=codex` 且无标题，不能据此推断为未扫描到的 OMP 子线程。
- 现有 08-21 子线程标题任务已扩展 `parentSessionId` / `parentTitle`、child Badge 与 Tooltip，但明确未变更网关 session ID 解析和请求日志 finalizer；该前提现已失效。

## In scope

- 找到并修复 OMP 实际请求中 session ID 未进入 `RequestLogTraceContext.session_id` 的根因；仅将经明确验证、安全限制的身份候选用于请求日志，不改变路由、上游请求、会话亲和性或请求体。
- 为 `requestlog/sessionTitles` 设计并实现有界、单飞、可快速返回的快照刷新方式；冷扫描、慢磁盘、坏文件或刷新错误不得阻塞日志页查询。
- 保持 OMP/Pi/Codex 主线程与已确认子线程的现有标题、`parentSessionId` / `parentTitle`、搜索和降级语义；child 仍使用自身 session ID 作为日志 join key。
- 增加覆盖实际 header fallback、冷/热标题快照、刷新并发、超时降级和日志页展示的回归测试与桌面运行验证。

## Out of scope

- 不修改 `request_logs` schema、迁移、历史回填或标题持久化。
- 不修改 OMP/Pi 协议、会话文件、请求转发、Aggregate API 路由、账号路由或 session-affinity 语义。
- 不将任意 `x-client-request-id` 视为会话 ID；可接受值的来源与格式必须有显式安全约束及测试。
- 不扩展未知 OMP/Pi 子线程目录布局、递归扫描深度或读取 transcript/prompt 正文。

## Actors and affected systems

- 本机 OMP 主线程与子线程客户端。
- service gateway request metadata、request-log finalizer 与 SQLite read model。
- request-log session-title resolver 与 admin-only `requestlog/sessionTitles` RPC。
- Tauri/Web transport、TypeScript normalizer 与请求日志页面。

## Assumptions and constraints

- 质量画像：`standard`。本任务修复服务日志行为和跨层读取投影，但不涉及认证、授权、资金、schema/migration 或远程文件访问。
- 标题索引允许返回最近一次有效快照；刷新期间优先保持日志页可用，而非等待最新本机文件扫描完成。
- OMP/Pi 文件扫描继续维持只读、有限深度、文件/目录/字节预算、no-follow/reparse-point 防护与局部降级。
- 当前实机服务地址为 `localhost:48760`；验证不得输出 RPC token、请求头授权值、prompt 或 transcript 内容。

## Acceptance criteria

### AC-001: OMP 日志保存经验证的会话 ID

- 场景：OMP 主线程或已确认子线程通过本机网关发出 `/v1/responses` 请求，当前标准 session header/请求体路径缺失，但存在符合已验证 OMP 合约的身份候选。
- 操作：完成请求并读取新写入的请求日志。
- 预期：`request_logs.session_id` 为实际 OMP 会话 ID；不改变上游请求、路由、session affinity 或现有更高优先级的 session/parent header。
- 不得：把任意格式的 client request ID 或 route anchor 误存为 session ID。
- 验证方法：网关单元/集成回归测试 + 本机新 OMP 请求的 SQLite 查询。

### AC-002: 标题 RPC 在冷刷新和并发请求下保持可用

- 场景：标题快照过期、OMP/Pi 目录很大、文件损坏/不可读，且日志页持续轮询标题与日志列表。
- 操作：并发调用 `requestlog/sessionTitles` 与 `requestlog/list_with_summary`。
- 预期：标题 RPC 快速返回最近有效快照或空快照；至多一个刷新在后台运行；日志列表不因标题扫描超时、排队或连接失败。
- 不得：同步全量扫描阻塞 RPC handler、为每次 5 秒轮询启动重复扫描或清空可用旧快照。
- 验证方法：服务定向并发/缓存测试 + 桌面运行日志不再出现该 RPC timeout。

### AC-003: 已匹配 OMP/Pi 主线程和子线程正确展示

- 场景：请求日志有对应主会话 ID，或有可确认父关系的 child ID。
- 操作：打开或刷新日志页。
- 预期：主线程显示自身标题；child 显示主标题和“子线程”标记，Tooltip 保留 child ID、parent ID、来源和 cwd；搜索主标题同时命中 main/child 日志。
- 不得：以 parent ID 覆盖 child 日志 join key，或在没有有效关系时猜测 parent 标题。
- 验证方法：Rust resolver、frontend runtime/Playwright 测试和本机浏览器/桌面验证。

### AC-004: 降级不影响请求转发和日志页

- 场景：身份候选缺失/无效、标题目录不可读、扫描尚未完成或 title 为空。
- 操作：发起请求和打开日志页。
- 预期：请求仍按原有路径完成；该日志维持无 session/未匹配或无标题的既有降级，其他日志与页面 RPC 正常。
- 不得：阻塞 gateway、伪造会话关联、泄露扫描的会话正文。
- 验证方法：错误矩阵回归测试与桌面 smoke。

## Open or blocking decisions

- 无：默认维持既有“标题可能滞后一个刷新窗口，但日志页和请求转发不得等待扫描”的用户体验。
