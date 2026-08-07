# 修复 OMP 请求日志会话标题

## Goal

让经本地 CodexManager 网关发出的 OMP 请求，在请求日志中按已持久化的 `session_id` 显示 OMP 当前会话标题；不得改变 OMP 协议、请求路由、上游转发或请求日志 schema。

## Confirmed Facts

1. 历史任务 `07-25-fix-request-log-session-title` 解决了请求日志 `session_id` 在请求头、请求体、HTTP、WebSocket 与 Aggregate API 路径中的解析/落库问题；截图中 UUID 与 UI 的“未匹配会话”证明本轮请求已经有 `session_id`，不是该回归。
2. 历史任务 `07-30-omp-session-title` 已实现 OMP+Codex 合并标题索引、`requestlog/sessionTitles` RPC、Tauri/Web transport、typed wrapper 和日志页 Map；当前 UI 每 5 秒刷新该索引。
3. 实现错误地仅枚举 `~/.omp/agent/sessions` 根目录直接 `.jsonl` 文件。真实 OMP 文件位于 `sessions/<project-directory>/*.jsonl`；本机根目录没有直接 `.jsonl`，包括截图 ID `019fca51-ab55-7000-beca-006a4140fdfa` 在内的会话均位于 `abs-Codex-Manager-…/` 子目录。
4. 因 OMP 索引为空，日志页 `sessionTitleMap` 无法命中非空 `sessionId`，`SessionInfoCell` 按既有降级行为显示“未匹配会话”。
5. 现有元数据解析与真实文件格式匹配：首行是固定标题槽、次行是含 UUID 与 `cwd` 的 session header；读取器不需要也不得读取后续 transcript。

## Requirements

- 标题索引必须兼容 OMP 的两种已知布局：根目录直接 `.jsonl`（兼容旧布局）和一层项目目录下的 `.jsonl`（当前布局）。
- 遍历只能进入根目录的直接普通项目目录一层；不得无界递归，不得跟随符号链接或 Windows reparse point。
- Windows 上必须继续用每个已验证目录的持有句柄相对打开文件；不得以拼接的子路径绕过现有 TOCTOU/reparse-point 防护。Unix 保持等价的 no-follow 语义。
- 既有文件数、目录项数、单行读取、标题/ID 长度限制和 5 秒缓存语义必须覆盖子目录扫描；目录不可读、损坏文件或单个异常项目目录必须局部降级。
- Codex 标题优先级、admin-only RPC、Web/Tauri transport、日志页搜索及“未匹配/无标题”降级行为保持不变。
- 本次只修标题 read model；不得修改 `request_logs.session_id` 解析、请求转发、ORM/schema，亦不回填没有 ID 的历史日志。

## Acceptance Criteria

- [x] `sessions/<project-directory>/*.jsonl` 中有效的 OMP title slot + session header 被索引并以原 UUID 返回；根目录直接 `.jsonl` 仍可索引。
- [x] 对截图 ID `019fca51-ab55-7000-beca-006a4140fdfa`，标题 RPC 在本机 OMP 根目录返回该 ID 与标题；日志页在一次刷新窗口内可获得标题而非“未匹配会话”。
- [x] 标题扫描不读取 session header 之后的 transcript，且拒绝符号链接、reparse point、超过上限、损坏或不可读的目录/文件。
- [x] 缓存能识别子目录内新增、删除和改名的标题元数据；同一文件未变更时不重新解析。
- [x] Codex 标题、ID 碰撞时 Codex 优先级、HTTP/WebSocket/Aggregate API 的既有 `session_id` 行为和 admin-only 标题访问保持不变。
- [x] 服务端定向回归测试与本机端到端标题匹配验证通过。

## Out of Scope

- 不修改 OMP，也不增加或转发私有标题请求头。
- 不持久化标题快照、不修改数据库 schema、不回填缺少 `session_id` 的历史日志。
- 不支持服务端与 OMP 位于不同主机、容器未挂载 OMP home 或非管理员 Web actor 的本机 OMP 标题；这些场景保留现有未匹配/拒绝行为。
- 不读取、导入、上传或展示 OMP transcript 正文、prompt、工具调用或密钥。

## Risks

- 原方案的“direct children only”安全约束与 OMP 实际布局冲突。修复必须使用受限的一层遍历，而非以无界递归换取匹配率。
- Windows 当前文件打开以目录句柄为安全边界；扩展遍历时必须让每个子目录拥有独立的已验证句柄。
