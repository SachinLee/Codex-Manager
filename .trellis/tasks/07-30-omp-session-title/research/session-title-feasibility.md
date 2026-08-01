# OMP 请求日志会话标题可行性研究

日期：2026-07-30
范围：方案分析；不修改产品代码。

## 已验证的事实

1. CodexManager 已将请求日志的 `session_id` 写入 SQLite：`RequestLogTraceContext.session_id` 在 `crates/service/src/gateway/observability/request_log.rs:530-535` 映射到 `request_logs.session_id`。会话 ID 解析优先级为受信请求头、父线程头、请求体元数据，见 `crates/service/src/gateway/local_validation/request.rs:2330-2397`。
2. 支持的请求头别名包括 `session-id`、`session_id`、`x-session-id`，见 `crates/service/src/gateway/request/incoming_headers.rs:83-91`。`client_metadata` / `metadata` 中的显式 session/thread ID 也会解析，见 `crates/service/src/gateway/request/request_helpers.rs:138-205`。
3. 当前日志 UI 只调用 `codex_session_list`（`apps/src/app/logs/page.tsx:142-149`），把其结果按 ID 建 Map（:238-247），再通过 `SessionInfoCell` 显示标题（`page-cells.tsx:38-110`）。该后端会话源只读本地 Codex `threads` SQLite 与 `session_index.jsonl`（`crates/service/src/codex_session/storage.rs:123-212`），没有 OMP 读取器；因此 OMP ID 即使已写入日志，也没有标题映射源。
4. 已确认本机 OMP 为 `17.1.8`。发布版源码会把 session ID 同时写入 `SESSION_ID`、会话 ID 和 `x-client-request-id` 头，并将它作为 `prompt_cache_key`，见 OMP `v17.1.8` 的 `packages/ai/src/providers/openai-codex-responses.ts:4032-4034` 与 `:1443-1444`。这与 CodexManager 已接受的 session 头契约兼容。
5. OMP 的会话管理器有稳定 `getSessionId()` 和 `getSessionName()`，且通过 `setSessionName()` 将自动/用户标题写入会话 JSONL，见 OMP `v17.1.8` 的 `packages/coding-agent/src/session/session-manager.ts:1694-1696,1789-1855`。会话根目录是 `~/.omp/agent/sessions`（支持 `PI_CONFIG_DIR` 覆盖），见 `packages/utils/src/dirs.ts:492-495,758-760`；每条会话文件首行是固定 256-byte 标题槽，次行保存 `session.id`，见 `packages/coding-agent/src/session/session-title-slot.ts` 与本机会话文件。

## 根因

不是“OMP 没有标题”，而是标题索引的来源不匹配：

`OMP 请求(session ID) -> CodexManager request_logs.session_id -> UI 仅查 Codex DB -> 找不到 OMP JSONL 标题`

另一个独立前提：所有实际出站路径都必须将已解析的 session ID 传入 `RequestLogTraceContext`。现有日志规范要求 HTTP、WebSocket、Aggregate API 三类 finalizer 都做到这一点；先前任务 `07-25-fix-request-log-session-title` 记录过 Aggregate API 漏传的回归风险。

## 方案比较

### A. CodexManager 读取本机 OMP 会话文件（推荐）

- 新增只读 OMP session-title resolver，按 OMP session ID 扫描/缓存 `~/.omp/agent/sessions/**/*.jsonl` 的首 4 KiB：读取固定标题槽和 session header，不读取会话正文。
- 日志页使用“日志会话标题索引”而不是仅 `codex_session_list`；索引合并 Codex 和 OMP，并为每项标记 `source: codex | omp`。
- 不改 OMP、不改 OpenAI 请求协议、不改 `request_logs` schema；标题始终以当前 OMP 会话文件为准，现有 5 秒查询刷新可自然反映改名。
- 适用条件：CodexManager 服务与 OMP 运行在同一主机、同一用户数据目录可读。服务模式部署到另一台机器时，只能显示 session ID。

### B. OMP 每请求额外传递标题

- OMP 增加专用私有头，例如 `x-codexmanager-session-title`，CodexManager 解析后持久化标题快照或 session-title 表。
- 适用于远程 CodexManager 无法读取 OMP 文件的部署。
- 缺点：要维护 OMP 与 CodexManager 两端契约；首个请求可能早于自动标题生成；标题是用户内容，写入请求日志增加隐私与保留期责任；需要确保该头不转发给上游。
- 仅把标题放在内存缓存不可接受：服务重启和历史日志都会丢失映射。

### C. CodexManager 由首条 prompt 推导标题

- 不推荐。网关无法可靠区分 OMP 会话与其他 OpenAI 兼容客户端；推导结果会偏离 OMP 的用户手动标题；还会扩大日志处理的用户内容范围。

## 推荐实施形态

在 CodexManager 内部增加一个 **read-only request-log session title resolver**，而不是扩展可增删改的 `codex_session_*` API：

1. 获取当前日志页中出现的非空 `session_id`。
2. 先用现有 Codex session 读取器命中；未命中的 ID 查询 OMP 索引。
3. OMP 索引按 `path + mtime + size` 缓存，首次扫根目录、后续只重读变更文件；标题槽/会话头最多读取 4 KiB，绝不解析或上传整个 transcript。
4. 返回 `{sessionId,title,cwd?,source}`；前端保持现有 ID→标题 Map，只将日志页的数据源替换为合并后的只读索引。搜索也改用同一个索引。
5. 无标题、损坏 JSONL、标题槽与 header ID 不一致、不可读目录均降级为“未匹配会话”，不影响请求转发与日志写入。

## 验证门槛

- OMP 17.1.8 经本地网关发起一条 SSE 和一条 WebSocket（若启用）请求：日志 `session_id` 等于 OMP JSONL header 的 `id`。
- OMP 自动标题生成或手工改名后，日志页在一次索引刷新内显示新标题；不产生数据库迁移。
- 仅有 session ID 的旧日志可在标题文件存在时回显标题；没有 ID 的历史日志不做猜测或回填。
- Aggregate API、HTTP、WebSocket 三条日志 finalizer 均保留 `session_id`；覆盖来自任务 `07-25` 的回归风险。
- OMP 会话目录不存在、无权限、JSONL 损坏、超过索引上限时，日志页仍可加载并显示 session ID/未匹配状态。

## 决策

推荐 A。B 仅为远程/多机部署的后续能力；C 不实施。
