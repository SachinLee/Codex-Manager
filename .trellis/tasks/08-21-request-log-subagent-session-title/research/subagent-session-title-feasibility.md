# OMP/Pi 子线程主线程标题可行性与方案研究

日期：2026-08-21
范围：只读可行性、技术方案和界面方案分析；未修改产品代码。

## 1. 问题本质

请求日志已经能保存 OMP/Pi 子线程的会话 ID，但标题 read model 只把每个会话当作独立条目处理，没有利用本地会话文件的目录层级建立父子关系：

```text
子线程请求 -> request_logs.session_id(child)
                    -> 现有 sessionTitles 按 child ID 查找
                    -> 子文件无标题 / 未被扫描
                    -> 日志页显示未匹配会话
```

目标不是把日志归属改成主线程，而是为 child ID 增加只读的 `parentSessionId` / `parentTitle` 关系投影，再由 UI 以主线程标题为主展示。

## 2. 已验证的代码事实

### Service

- `crates/service/src/requestlog/requestlog_session_titles.rs:26-41` 已定义 `RequestLogSessionSource::{Codex,Omp,Pi}` 和 `{sessionId,title,cwd,source}`。
- `:86-122` 读取 Codex、Pi、OMP 三类标题后合并；`:192-270` 使用路径 + mtime + size 缓存，5 秒刷新。
- `:353-438` 的发现逻辑只从已打开目录收集直接 JSONL，并对根目录的一层项目目录做受限扩展；没有把子会话目录解析为父会话关系。
- `:446-479` 的 OMP 解析依赖 title slot + session header；`:481-530` 的 Pi 解析读取 session header，并从后续 `session_info` 或首个 user message 生成标题。两者都没有 parent projection。
- `:998-1010` 已拒绝 symlink/reparse point；现有安全边界应继续保留。

### Gateway / persistence

- `crates/service/src/gateway/request/incoming_headers.rs:15-24,83-149` 已解析 `session_id`、`x-openai-subagent` 和 `x-codex-parent-thread-id`。
- `crates/service/src/gateway/local_validation/request.rs:2447-2462` 的日志 ID 优先级是当前 session header > parent-thread header > body metadata；这能保留 child ID，但没有持久化父子关系。
- `crates/core/src/storage/mod.rs:817-820` 的 `RequestLog` 仅含 `session_id` 与 `conversation_anchor`，没有 parent/session-kind 字段。因此直接改 SQLite schema 不是最小方案。

### Frontend

- `apps/src/app/logs/page.tsx:160-167` 每 5 秒调用 `serviceClient.listRequestLogSessionTitles`；`:276-280` 按 `sessionId` 构建 Map。
- `apps/src/app/logs/page-cells.tsx:38-118` 已在 Tooltip 显示标题、会话 ID、来源和工作目录；当前来源只区分 Codex/OMP/Pi，没有子线程标记。
- `apps/src/types/request-log.ts:112-119` 与 `apps/src/lib/api/normalize.ts:1820-1839` 需要在返回契约扩展时同步新增字段，并兼容旧服务缺字段。

## 3. 本机文件布局证据

### OMP

本机 `C:/Users/shuan/.omp/agent/sessions/` 样本显示：

```text
<project>/
  <main-session>.jsonl
  <main-session-stem>/
    DeviceDetailPlanner.jsonl
    DeviceDetailImplementer.jsonl
    DeviceDetailFinalReview.jsonl
```

主文件第一行是有效 title slot；子代理样本第一行 title 为空，第二行是独立 child session ID。子文件通过所在的 `<main-session-stem>/` 可以回到同项目的主文件。

### Pi

本机 `C:/Users/shuan/.pi/agent/sessions/` 样本显示：

```text
<project>/
  <main-session>.jsonl
  subagent-artifacts/<agent>_transcript.jsonl  # 项目级聚合转录，不作为本轮索引输入
  <main-session-stem>/
    <agent-id>/run-0/session.jsonl
```

主线程标题来自主文件中的有效 `session_info.name`，没有有效 name 时按已有规则回退到首个 user prompt。子代理运行文件有独立 session header，且其路径祖先保留主线程文件 stem，可作为父关系来源。`subagent-artifacts/` 是项目级兄弟目录，文件名不包含主会话 ID；在没有额外、已验证关联字段的前提下不能安全归属到主线程，因此本轮不扫描或展示其转录。

## 4. 方案比较

### A. 扩展只读标题索引，建立父子投影（推荐）

- 在现有 session title resolver 内增加有限深度的 known-layout discovery，不使用无界递归。
- 先索引主会话，再对每个主会话的已知派生目录索引 child session；通过父文件路径得到 `parentSessionId`，通过主索引得到 `parentTitle`。
- 返回条目保留 child `sessionId`，新增可选字段，例如 `isSubagent`、`parentSessionId`、`parentTitle`、`parentSource`。
- 前端以 parentTitle 作为子线程主显示标题，追加 Badge；Tooltip 保留 child/parent ID 和来源。
- 优点：不改请求日志 schema、不改客户端协议，历史 child 日志只要文件仍在即可显示；与现有 5 秒缓存和 admin-only RPC 一致。
- 风险：OMP/Pi 的文件布局是外部实现细节，必须限制扫描深度、路径形态、数量和读取字节数，并为每种布局写 fixture。

### B. 请求日志持久化父子关系（不推荐）

- 扩展 `request_logs` 保存 parent ID、subagent 标记或标题快照。
- 优点：不依赖服务重启后仍能发现文件。
- 缺点：要迁移 schema、同步 HTTP/WebSocket/Aggregate 所有 finalizer、处理标题改名和隐私保留；现有日志只读投影目标会扩大成数据模型变更。

### C. 仅凭请求头在 UI 端推断（不推荐）

- 将 `x-openai-subagent` / `x-codex-parent-thread-id` 作为日志字段或临时返回值，再由 UI 查找父标题。
- 当前日志 API 没有这些字段；且父头可能只是 session ID 回退来源，不能保证对应本地文件路径。无法覆盖 Pi 的文件层级关系，也难以关联历史日志。

### D. 子线程标题自行展示，不显示主线程标题（不满足需求）

- 只解析 child 自身 `session_info` 或首条 prompt。
- 能显示部分 child 名称，但没有解决“显示对应主线程标题”，并且 OMP child 常为空标题。

## 5. 推荐数据流

```text
OMP/Pi known session layouts
  -> bounded metadata scanner + cache
  -> main title index + child->parent relation
  -> requestlog/sessionTitles (admin-only)
  -> existing Tauri/Web transport + typed normalizer
  -> sessionId -> title map
  -> SessionInfoCell: parent title + 子线程 badge + IDs in tooltip
```

建议使用可选父字段，确保旧主线程条目和旧服务响应无需迁移：

```text
RequestLogSessionTitle {
  sessionId,
  title,
  cwd,
  source,
  isSubagent?: boolean,
  parentSessionId?: string | null,
  parentTitle?: string | null,
  parentSource?: RequestLogSessionSource | null,
}
```

字段命名最终在 `design.md` 中收敛；不要让 UI 从路径或原始 JSONL 自行推断。

## 6. 界面方案

### 推荐布局：主标题优先 + 子线程徽标

主线程：

```text
修复登录超时
session: 019f...
```

子线程：

```text
修复登录超时   子线程
session: 01a0...
```

Tooltip：

```text
会话标题：修复登录超时
执行层级：子线程
来源：OMP / Pi
子线程 ID：01a0...
主线程 ID：019f...
工作目录：D:/...
```

- Badge 采用现有 `Badge` 组件，使用低强调度颜色，不改变表格行高度。
- 搜索“会话标题”继续按主标题和 session ID 命中；是否把 child-only title 纳入搜索属于后续增强，不作为本轮必要行为。
- 主线程不显示 Badge；无法确认父关系时按原有普通会话显示，避免误标记。
- 不在表格主标题位置展示子任务 prompt，避免列表噪音和敏感信息扩散。

## 7. 可行性结论

技术上可行，且推荐 A。当前接口、缓存、类型和 UI 已有可扩展接缝；主要工作集中在 service 的 bounded parent discovery、返回契约扩展和 `SessionInfoCell` 的展示。不可行边界是远程/容器 service 无法访问客户端会话目录的场景，该场景只能保留现有未匹配降级。

本轮尚需用户确认的唯一产品决策：是否采用“主线程标题 + 子线程 Badge，Tooltip 展示 child/parent ID”的推荐界面语义。
