# 分析 OMP 请求会话标题方案

## Goal

让经过本地 CodexManager 网关的 OMP 请求，在请求日志中以已有的稳定 `session_id` 关联并展示 OMP 当前会话标题；不改变 OMP 协议、网关转发行为或请求日志表结构。

## Confirmed Facts

- CodexManager 已持久化 `request_logs.session_id`，并接受 `session-id`、`session_id`、`x-session-id` 及显式请求体 session/thread ID。
- OMP 17.1.8 会把稳定会话 ID 写入 Codex 请求的 session 相关请求头和 `prompt_cache_key`；其本地 JSONL 会话文件保存该 ID 与自动/用户标题。
- 当前日志页仅查询本地 Codex `threads` / `session_index.jsonl`，按 ID 映射标题；它不会读取 OMP 的 `~/.omp/agent/sessions`。因此 OMP 日志即使已有 session ID，也没有标题来源。
- OMP 标题可从固定大小的 JSONL 标题槽和后续 session header 读取，无需读取会话正文。
- 对 Aggregate API、HTTP、WebSocket 而言，最终日志写入链路仍必须保留已解析的 session ID；此前任务已记录 Aggregate API 漏传的回归风险。

## Requirements

- 提供仅供请求日志读取的合并会话标题索引：现有 Codex 会话与本机 OMP JSONL 会话均可按 `sessionId` 查询。
- OMP 标题索引必须只读取会话 JSONL 的标题槽与 session header；不得读取或返回对话正文、prompt、工具调用、密钥或其他 transcript 内容。
- 默认定位 OMP 会话根 `~/.omp/agent/sessions`，并遵循 `PI_CONFIG_DIR` / OMP profile 的本机路径语义；目录不存在、不可读或包含损坏文件时静默降级为空索引。
- 日志页的标题展示与标题搜索都使用同一合并索引；Codex 会话管理页及其删除、移动、归档操作不得接触 OMP 文件。
- OMP 标题改名应在现有日志页刷新窗口内可见；索引应以文件元数据缓存，避免每次查询解析完整 transcript。
- 已有 HTTP、WebSocket、Aggregate API 请求日志必须继续以 `session_id` 为唯一关联键；缺失 ID 的历史日志不得猜测或回填。

## Acceptance Criteria

- [x] OMP session ID 已写入请求日志时，日志页在标题索引刷新后显示对应的 OMP 标题和来源。
- [x] Codex 会话标题、搜索与会话管理行为保持不变；OMP 会话不会出现在可删除/移动的 Codex 会话接口中。
- [x] 标题读取仅限 JSONL 元数据前缀；损坏、无标题、无权限或缺失目录的文件不会使 RPC 或日志页失败。
- [x] 标题修改在缓存刷新后生效；重复扫描未变化文件不会重复解析其正文或无界增长缓存。
- [x] 服务 RPC、Tauri 注册、Web 命令映射、前端 typed wrapper 和日志 UI 全链路同步。
- [x] 服务端单元测试覆盖 OMP 文件解析、缓存/降级和 Codex+OMP 合并；前端构建通过。

## Out of Scope

- 不修改 OMP，也不增加或转发私有标题请求头。
- 不迁移 `request_logs` 或持久化标题快照；标题以本机 OMP 当前 metadata 为准。
- 不支持服务端与 OMP 位于不同主机或不可访问的用户目录；此场景继续显示 session ID / 未匹配状态。
- 不读取、导入、上传或展示 OMP transcript 正文。

## Decision

- 新增 request-log 专用的只读会话标题索引 RPC，而不是扩展 `codex_session_list`，避免 OMP transcript 被 Codex 会话的可变管理 API 误处理。
- 合并索引返回最小字段 `{ sessionId, title, cwd, source }`，其中 `source` 为 `codex` 或 `omp`。
- 以 OMP 文件的路径、修改时间和大小作为缓存失效依据；扫描与解析均设上限并跳过符号链接/异常文件。
