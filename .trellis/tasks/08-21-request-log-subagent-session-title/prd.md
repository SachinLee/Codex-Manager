# 请求日志展示 OMP/Pi 子线程主线程标题

## Goal

让请求日志中由 OMP 或 Pi 子线程发出的请求能够显示其对应主线程标题，并在界面上明确标记该记录来自子线程；主线程记录、Codex 标题、请求转发和既有会话 ID 行为保持不变。

## Current behavior and problem

请求日志通过 `request_logs.session_id` 与只读会话标题索引关联。当前标题索引已经支持 Codex、OMP、Pi 三类来源，日志页也会显示来源，但只按 `sessionId` 建立一层标题 Map。

OMP/Pi 子代理的持久化会话文件位于主会话文件的派生目录或运行目录中，而不是现有标题扫描器直接枚举的会话文件层级。子线程自身通常没有可展示的主标题；即使能解析到子线程 ID，也缺少“父会话标题”和“子线程”关系信息。因此请求日志会显示“未匹配会话”或无法表达其执行层级。

## Confirmed facts

- `crates/service/src/requestlog/requestlog_session_titles.rs` 已统一合并 Codex、OMP、Pi 标题，并以 `RequestLogSessionTitle { sessionId, title, cwd, source }` 返回；缓存刷新间隔为 5 秒。
- 当前 OMP/Pi 扫描器支持根目录直接文件和一层项目目录文件，但不会建立“子会话 -> 主会话”的父子关系。
- OMP 本机样本的主会话文件位于 `~/.omp/agent/sessions/<project>/*.jsonl`；子代理文件位于主会话文件同名目录下，例如 `<main-session-stem>/DeviceDetailPlanner.jsonl`，子文件的首行标题可能为空。
- Pi 本机样本的主会话文件位于 `~/.pi/agent/sessions/<project>/*.jsonl`；可安全关联的子代理运行会话位于 `<main-session-stem>/<agent-id>/run-<decimal>/session.jsonl`，可通过路径祖先关联到主会话文件。项目级 `subagent-artifacts/` 转录文件与具体主会话没有确定性路径关联，本轮不扫描或展示它们。
- 网关已解析 `x-openai-subagent` 与 `x-codex-parent-thread-id`；当前请求日志会话 ID 解析优先使用当前 `session_id`，再回退到 `parent_thread_id`，但持久化的 `RequestLog` 只有 `session_id` 与 `conversation_anchor`，没有父会话或子线程标记字段。
- 前端 `SessionInfoCell` 当前会显示标题、短会话 ID、来源和工作目录；同一 `sessionTitleMap` 也被用于会话标题搜索。

## Requirements

1. 标题服务应能将 OMP/Pi 子线程会话 ID 解析为对应主线程标题，并返回足够的父子关系元数据供界面展示。
2. 子线程请求日志必须保留当前子线程会话 ID；不得把数据库中的 `session_id` 改写为主线程 ID。
3. 主线程请求日志的现有标题、来源、搜索和降级行为不变。
4. OMP/Pi 的文件扫描必须保持只读、有限深度、有限文件/目录/单行大小，并拒绝符号链接、Windows reparse point、损坏或不可读文件；不得读取 transcript 中不必要的敏感正文。
5. 父会话不存在、标题为空、层级无法确认或跨机器无法读取会话目录时，局部降级为既有未匹配/无标题状态，不影响日志页加载和请求转发。
6. 界面需要同时表达“这是 OMP/Pi 来源”和“这是子线程”，并保留子线程 ID 可追溯性；主线程不显示多余的子线程标记。

## Actors and affected systems

- OMP/Pi 客户端及其本地 JSONL 会话目录
- `crates/service` 请求日志标题只读投影与 `requestlog/sessionTitles` RPC
- `apps/src/app/logs` 请求日志表格、搜索和会话信息 Tooltip
- Tauri/Web transport 与 TypeScript 类型归一化（仅在返回契约扩展时同步）
- SQLite 请求日志 schema：原则上不改动

## Constraints and assumptions

- 采用本机文件读取方案；远程 service、容器未挂载 OMP/Pi 会话目录时不承诺显示标题。
- 主线程标题以主会话文件最新有效元数据为准，不从子线程首条 prompt 推导主标题。
- 不新增 OMP/Pi 私有请求头，不修改 OMP/Pi，不把 transcript 正文写入数据库或请求日志。
- 任务质量画像暂定为 `standard`：涉及 Rust 只读索引、RPC 返回契约和日志 UI，但不涉及认证、权限、资金或数据库迁移。

## Acceptance criteria

### AC-001: 子线程显示主线程标题

- 场景：请求日志的 `sessionId` 对应 OMP/Pi 子线程，且能从本机会话目录找到主线程有效标题。
- 操作：打开请求日志页或刷新标题索引。
- 预期：会话列显示主线程标题，并明确显示“子线程”标记；Tooltip 同时显示子线程 ID、来源和主线程关联信息。
- 不得：将请求日志中的子线程 ID 替换成主线程 ID。
- 验证方式：service 标题索引回归测试 + 日志页浏览器验证。

### AC-002: 主线程行为不回归

- 场景：请求日志的 `sessionId` 对应 Codex、OMP 或 Pi 主线程。
- 操作：刷新日志页并按会话标题搜索。
- 预期：主线程标题、来源、工作目录、搜索命中和现有无标题/未匹配降级保持原行为，不出现“子线程”标记。
- 验证方式：现有 service 标题测试、前端类型/组件测试和日志页浏览器验证。

### AC-003: 父子关系解析受限且可降级

- 场景：会话目录存在嵌套子目录、损坏文件、超限文件、符号链接/reparse point、缺失父会话或权限错误。
- 操作：调用标题索引 RPC。
- 预期：有效父子关系被返回；无效条目被局部跳过，RPC 和日志页仍成功返回；扫描不无限递归且不读取不必要的 transcript。
- 验证方式：Rust 文件布局/缓存/安全边界回归测试。

### AC-004: 返回契约跨端一致

- 场景：service 返回带父子关系的 OMP/Pi 标题条目。
- 操作：通过 Tauri 和 Web 两种 transport 请求标题列表。
- 预期：TypeScript 归一化、请求日志 Map、搜索和展示均保留新增关系字段；旧服务返回缺少字段时仍按主线程条目兼容处理。
- 验证方式：RPC/transport 归一化测试和前端构建。

## Out of scope

- 不修改 `request_logs` 表结构，不回填缺少会话 ID 的历史日志。
- 不改变网关路由、上游转发、请求体、请求头或 session ID 解析优先级。
- 不展示子线程 transcript、prompt、工具调用、模型上下文或敏感元数据。
- 不实现远程主机文件访问、跨机器标题同步或 OMP/Pi 协议改造。
- 不重做整个请求日志页面，只调整会话单元格及必要的类型/契约字段。

## Key decisions

- 采用“主线程标题 + `子线程` Badge”的会话列展示语义。
- 子线程 Tooltip 展示子线程 ID、主线程 ID、平台来源和工作目录。
- 子线程自身标题不替换主线程标题，避免标题层级混淆、表格噪音和子任务文本额外暴露。
- 子线程日志仍以子线程 `sessionId` 作为唯一追踪 ID；主线程 ID 只作为只读展示关联。

## Open or blocking decisions

- 无。
