# Technical Design: OMP Request-Log Session Titles — Nested Project Directories

## Problem and boundary

`request_logs.session_id`、标题 RPC、Tauri/Web transport 和日志页均已工作；失败点仅在 OMP title read model 的发现阶段。它假设 `~/.omp/agent/sessions/*.jsonl`，而实际 OMP 将会话写入 `~/.omp/agent/sessions/<project-directory>/*.jsonl`。

本次改动仍是请求日志的只读投影：不参与请求准入、路由、上游 header/body、日志持久化、Codex 会话管理或 OMP 生命周期。`request_logs.session_id` 是唯一 join key。

## Correct data flow

```text
OMP session root
  ├─ legacy direct *.jsonl
  └─ direct project directory/*.jsonl
       -> bounded metadata-only scanner and cache
       -> requestlog/sessionTitles (admin-only)
       -> existing Tauri/Web command and typed wrapper
       -> existing logs-page sessionId -> title Map
```

现有 RPC、transport、normalizer、UI 与 `SessionInfoCell` 均不改。标题搜索和表格继续使用同一个返回列表。

## Discovery and safety design

### Supported tree

只支持深度 0 与深度 1：

1. 在已安全打开的 sessions 根目录中收集直接 regular `.jsonl`。
2. 对每个根目录直接普通子目录，拒绝 symlink/reparse point 后安全打开该目录；仅在该目录中收集直接 regular `.jsonl`。
3. 不再进入孙目录或跟随任意链接。若未来 OMP 改为更深树，必须先有新的布局证据与独立设计。

统一计算根目录和所有已访问项目目录的目录项数，文件数仍受 `MAX_OMP_SESSION_FILES` 限制。无法打开或枚举一个项目目录时跳过该目录，不能让 RPC 失败。

### Handle ownership

当前 Windows 防护要求文件通过持有的 `OmpSessionDirectory.handle` 以 basename 相对打开，并拒绝 reparse point。将 `OmpSessionFile` 扩展为携带其 owner directory（或同等的稳定 owner key/handle 生命周期），不能把 `project/file.jsonl` 填入现有只接受单 path component 的 `name`。

- 根目录和每个子目录均以 no-follow/reparse-safe 方式打开并验证。
- Windows 以该目录句柄调用现有 `NtCreateFile` 相对打开 basename；不使用不受约束的拼接路径。
- Unix 以相应已验证目录上下文枚举并保持 `O_NOFOLLOW` 文件打开语义。
- 缓存仍按稳定完整文件路径的 `{modified_at, size, parsed metadata}` 键控；新增/删除子目录项在下一次刷新时自然写入/剪枝。

### Metadata parser and privacy

沿用现有 parser：每个文件只读取固定上限内的第一、第二 JSONL 行，分别要求 `type: title` 和 `type: session`；从 session header 的 UUID-shaped `id` 与 title slot/header 的合法标题生成 `{sessionId, title, cwd, source: omp}`。不读取第三行及以后内容，不用首个 message 兜底。

Codex 会话继续与 OMP 候选合并；ID 冲突时 Codex 优先。缺失 root、权限错误、坏 JSON、无标题、超限、链接或 reparse point 一律局部降级为未匹配。

## Compatibility and operations

- 本机桌面管理员与本机 service/Web 管理员的既有 RPC 合约不变。
- 容器或远程服务若无法访问其自身 OMP home，仍没有 OMP 标题；不以远程文件读取或标题上报扩大本次范围。
- 代码只在 `crates/service/src/requestlog/requestlog_session_titles.rs` 及对应测试中变更；无需 schema、RPC、Tauri 或前端改动。
- 交付桌面 exe 时仍需重新构建包含 service 的 Tauri bundle；无需因本次不改前端而单独修改 static export。

## Regression tests

1. 根目录直接 JSONL（现有兼容行为）。
2. `root/project-a/session.jsonl` 发现、解析并返回 title/UUID（本轮失效场景）。
3. 多项目目录、根目录 + 子目录的总数/排序/limit 和 cache pruning。
4. 子目录 symlink/reparse、无法打开目录、坏文件、超过目录项/文件上限均被跳过。
5. 子目录 session title 改名/删除后，缓存刷新反映更新且未变化文件不重新解析。
6. 既有 parser 隐私边界、UUID 拒绝和 Codex collision precedence 不回归。

## Rollback

若一层发现逻辑出现兼容问题，只回退该发现逻辑；现有 direct-root 扫描和所有 RPC/UI 合约可独立保留，数据库无需回滚。
