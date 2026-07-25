# 修复请求日志会话标题关联

## Goal

恢复 Codex App Gateway 请求日志与本地 Codex 会话标题的关联，使新请求在“会话”列显示对应的会话标题。

## Confirmed Facts

- 请求日志页面在 `sessionId` 与 `conversationAnchor` 都为空时显示 `-`；标题映射本身正常。
- 本机 `codexmanager.db` 中，2026-07-25 15:25:57 后的新日志记录的 `session_id` 和 `conversation_anchor` 均为 `NULL`。
- 本地 Codex `state_5.sqlite` 中存在对应会话及标题。
- 7 月 22 日的回归删除了从请求体显式会话字段获取日志 `session_id` 的后备路径，仅保留部分 HTTP 头读取。
- 新版 Codex 客户端会使用 `x-session-id`；原实现只识别 `session-id` / `session_id`，因此即使请求带有现代会话头也会漏记。
- 当前线上流量主要走 Aggregate API。`proxy_aggregate_request` 写 `request_logs` 时使用 `..Default::default()`，即使 local validation 已解析出 `request_log_session_id`，成功/失败日志也不会落库。

## Requirements

- 服务端应继续优先使用可信的会话 HTTP 头，并兼容 `session-id`、`session_id` 与 `x-session-id`。
- 当请求头未提供会话 ID 时，服务端应从请求体顶层、`client_metadata` 或 `metadata` 中的显式会话/线程字段恢复会话 ID。
- `prompt_cache_key` 仅在其值可识别为 Codex 线程 ID 时才能作为后备，且不得使用路由锚点格式 `pck:v1:`。
- 修复不得改变会话路由、上游请求体或数据库 schema；仅影响请求日志的 `session_id` 记录。
- 为无请求头、但含 `client_metadata.thread_id` 的载荷添加回归覆盖。

## Acceptance Criteria

- [x] 无会话 HTTP 头、但包含有效 `client_metadata.thread_id` 的 `/v1/responses` 请求会在日志中保存该 ID。
- [x] HTTP 与 WebSocket 请求携带 `x-session-id` 时均能识别并持久化该会话 ID。
- [x] 有会话 HTTP 头时，日志仍优先保存头中的 ID。
- [x] `pck:v1:` 路由锚点和任意普通 `prompt_cache_key` 不会被错误记录为会话 ID。
- [x] 相关 Rust 测试通过。
- [ ] 重新打包并重启后，新 Codex App Gateway 请求可被前端关联到本地会话标题。

## Notes

- 历史 `session_id` 为空的日志没有可靠的会话归属信息，不在本次回填范围内。
