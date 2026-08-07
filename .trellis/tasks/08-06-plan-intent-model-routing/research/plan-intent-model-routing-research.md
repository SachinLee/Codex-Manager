# 研究：规划意图模型路由扩展（OMP）

只读调查结论。目标文件 `.omp/extensions/trellis/index.ts`（586 行）。所有符号锚点均来自实际读取的
仓库文件或本机安装的 `@oh-my-pi/pi-coding-agent` 源码（`C:/Users/shuan/.bun/install/global/node_modules/@oh-my-pi/pi-coding-agent/`，版本与 `omp v17.2.9` 同源）。

## 1. 已验证的 OMP 扩展 API（全部来自包内 TS 源码，非文档转述）

### ExtensionAPI（`src/extensibility/extensions/types.ts`）
- `pi.setModel(model: Model): Promise<boolean>` — L1279-1280，注释 "Returns false if no API key available"。
  实现 `runExtensionSetModel`（`src/extensibility/extensions/compact-handler.ts:35-40`）：`getApiKey(model)` 无 key → `false`；否则 `session.setModel(model)` → `true`。底层 `session.setModel`（`src/session/agent-session.ts:6458` → `src/session/model-controls.ts:205-239`）会 `appendModelChange("provider/id", role="default")` —— **扩展路径固定写入 `role: "default"` 的 `model_change` 会话条目**（无法通过公开 API 传 role）。失败时可能 throw（`hasConfiguredAuth` 检查在 `setModel` 内，但扩展路径已先过 `getApiKey`）。
- `pi.appendEntry<T>(customType: string, data?: T): void` — L1261-1262；实现为 `sessionManager.appendCustomEntry(customType, data)`（`src/modes/controllers/extension-ui-controller.ts:162-164`；`src/session/session-manager.ts:2108`），写入 `type: "custom"` 会话条目（JSONL 持久化，append-only，压缩不影响其存在）。
- `pi.on(event, handler)` — 事件全集 L1138-1197。本需求相关：`session_start`、`session_switch`、`session_before_branch`/`session_branch`、`session_before_compact`、`session.compacting`、`session_tree`、`context`、`before_agent_start`、`input`。
- `pi.sendMessage(message, { triggerTurn?, deliverAs? })` — L1250-1253（本需求不需要）。
- `pi.hasUI` **不存在**于 ExtensionAPI；UI 可用性只能用 `ctx.hasUI`（types.ts:425，`ExtensionContext.hasUI: boolean`）。

### Handler 上下文（`ExtensionContext`，types.ts:415-472）
- `ctx.models: ExtensionModelQuery` — L437。接口 L394-413：`list(): Model[]`（已认证可用模型，`modelRegistry.getAvailable()`）、`current(): Model | undefined`、`resolve(spec: string): Model | undefined`（`provider/id`、裸 id 或角色别名如 `@plan`；实现 `src/extensibility/extensions/model-api.ts:25-38` → `resolveModelRoleValue`，只对**可用（已认证）**模型解析；无匹配返回 `undefined`）、`family(model): string`。
- `ctx.sessionManager: ReadonlySessionManager` — L431。接口为 `Pick<SessionManager, ...>`（`src/session/session-manager.ts:318-349`）：`getSessionId()`、`getSessionFile()`、`getLeafId()`、`getBranch(fromId?)`（L2258：根→叶路径 `SessionEntry[]`）、`getEntries()`、`getTree()`、`getHeader()` 等。**没有** `getLastModelChangeRole`（那是完整 SessionManager 的方法，未 Pick）。
- `ctx.ui.notify(message: string, type?: "info" | "warning" | "error")` — L187 附近（`ExtensionUIContext.notify`）。
- `ctx.ui.setStatus(key, text)` — 状态栏（L197 附近）。
- `ctx.hasUI`、`ctx.cwd`、`ctx.model`、`ctx.isIdle()` 等 — L424-440。
- `ctx.models.current()` 是 live getter（runner.ts:163-165 注释），`setModel` 后立即读可能短暂滞后；重读/以返回值判断为准。

### 事件载荷（`src/extensibility/extensions/types.ts` + `src/extensibility/shared-events.ts`）
- `InputEvent`（types.ts:795-800）：`{ type: "input"; text: string; images?: ImageContent[]; source: "interactive" | "rpc" | "extension" }`。**`event.text` 与 `event.source` 均确认**。仅交互模式发射：`emitInput` 唯一调用点在 `src/modes/controllers/input-controller.ts:664-665`（`source: "interactive"`）。`InputEventResult`（types.ts:1010-1015）：`{ handled?: boolean; text?: string; images?: ImageContent[] }` —— 不改写原文则返回 undefined 即可。
- `BeforeAgentStartEvent`（types.ts:655-660）：`{ prompt: string; images?; systemPrompt: string[] }`。结果 `BeforeAgentStartEventResult`（types.ts:1033-1037）：`{ message?: CustomMessagePayload; systemPrompt?: string[] }`。**每个扩展的每个 handler 一次最多返回一条 message**（`emitBeforeAgentStart`，runner.ts:1344-1418：`messages.push(result.message)`），但同一扩展可注册**多个** `before_agent_start` handler，各自返回一条。
- `ContextEvent`（shared-events.ts:179-183）：`{ messages: AgentMessage[] }`（深拷贝可改）。结果 `ContextEventResult`（types.ts:1001-1003）：`{ messages?: AgentMessage[] }`。
- `SessionStartEvent`（shared-events.ts:28-30）：`{ type: "session_start" }`（无载荷；信息都在 ctx）。
- `SessionSwitchEvent`（shared-events.ts:42-48）：`{ reason: "new" | "resume" | "fork" | "handoff"; previousSessionFile }` —— resume 也会走它。
- `SessionBranchEvent`（shared-events.ts:58-61）、`SessionTreeEvent`（shared-events.ts:133-140，带 `newLeafId`）。
- `SessionBeforeCompactEvent`（shared-events.ts:64-71）：含 `branchEntries: SessionEntry[]`（根→叶）——压缩时也可读状态，但没必要：custom 条目 append-only 持久，`getBranch()` 压缩后仍完整。
- `SessionCompactingResult`（shared-events.ts:368-374）：`{ context?: string[]; prompt?: string; preserveData?: Record<string, unknown> }` —— 非必需。

### 会话条目（`src/session/session-entries.ts` 语义，文档 `omp://session.md`）
- `custom` 条目：`{ type: "custom", id, parentId, timestamp, customType, data }`。**核心保留值**（扩展禁用）：`tool_execution_start`、`session_exit`、`user_todo_edit`、`vibe-session-lifecycle`、`autoresearch-control`。扩展须用域名/包限定名。
- `model_change` 条目：`{ type: "model_change", model: "provider/id", role? }`；`buildSessionContext` 按 `role ?? "default"` 重建模型映射（文档 session.md）。核心恢复流程（`session-operations` 文档 switchSession 步骤 8）会恢复持久化的模型。
- `custom_message` 条目：参与 LLM 上下文（`before_agent_start` 返回的 message 会以 custom_message 形式进入该轮上下文）。

### 配置与角色（`omp://models.md`、`omp://settings.md`、本机 `C:/Users/shuan/.omp/agent/config.yml`）
- 角色别名：`@plan`/`@default` 等经 `settings.modelRoles` 展开；优先级（低→高）：内置默认 ← 全局 `~/.omp/agent/config.yml` ← 项目 `<cwd>/.omp/config.yml` ← CLI `--config` overlay ← 运行时覆盖（`--model/--plan/--smol/--slow` 及 `PI_PLAN_MODEL` 等 env）。本机全局配置确含 `modelRoles.plan: aiswitch-china/glm-5.2:xhigh` 与 `modelRoles.default: codex-manager/gpt-5.6-terra:xhigh`。
- 角色值可带 thinking 后缀（`:xhigh`）；`resolve` 返回基础模型（后缀只影响 thinking 级别）。

## 2. 现有扩展数据流（`.omp/extensions/trellis/index.ts`）

- `session_start`（L455-499）：`findProjectRoot(ctx.cwd)`（L11-24，向上找 `.trellis` 目录）；`deriveContextKey(ctx)`（L43-54，`sessionManager.getSessionId()/getSessionFile()` 或 `TRELLIS_CONTEXT_ID`）；主会话注入 `trellis-session-context` + `trellis-task-context` 消息并 `ctx.ui.notify(...)`；子代理（`detectAgentType` L426-433，`PI_BLOCKED_AGENT`）只注入任务上下文。
- `session_before_compact`（L501-503）：记录 `lastCompactionTs`。
- `before_agent_start`（L505-524）：每次主代理启动返回 `{ message: { customType: "trellis-workflow-state", content, display: false } }`（turnCache L355-407，1.5s TTL 去重）。
- `context`（L528-562）：若压缩发生在最近注入之后，反向扫描 `event.messages` 找 `role === "custom" && customType === "trellis-workflow-state"`；缺失则追加注入（压缩安全网）。
- `tool_call`（L565-574）：给 bash 注入 `TRELLIS_CONTEXT_ID` env。
- `input`（L576-583）：仅预热 turnCache，不返回任何内容。

**可复用的既有模式**：① 压缩安全网（`lastCompactionTs`/`lastInjectionTs` + context 反向扫描）——规划约束注入直接复用；② 会话身份派生（`deriveContextKey`）；③ `.trellis/.runtime/sessions/<contextKey>.json` 的运行时状态恢复（`resolveActiveTaskStatus` L182-243）——但**规划状态应存 OMP 会话 custom 条目**（JSONL 天然随会话/分支/压缩保留），不要另开文件。Python 孪生 hooks（`.codex/hooks/inject-workflow-state.py` 等）是其他平台版本，本需求仅 OMP。

## 3. 实现建议（符号级）

### 新增模块（纯函数，便于单测）
- `.omp/extensions/trellis/plan-intent.ts`：`classifyPlanIntent(text: string): "enter" | "exit" | "none"`。
  - 先剥离 fenced code block（``` ... ```）与行内代码（`` `...` ``）、引用段（`>` 开头块）再匹配。
  - ENTER 短语表（PRD 确认 + 稳定同义）：规划方案、修改方案规划、技术方案、实施计划、帮我计划、帮我规划、做个方案、出个方案、设计方案、计划一下、plan this、plan this out 等。
  - EXIT 短语表：批准实施、开始实现、开始实施、批准执行、按方案实施、退出规划（模式）、approve (the) plan、start implementation、exit plan mode 等。
  - 否定抑制：触发词所在从句被 `不要/别/无需/不用/无须/别去/不必` 等否定词修饰 → 不触发 ENTER（同样对 EXIT 生效）。
  - 无触发短语 → `"none"`（含纯历史回顾/旁问）。这是 PRD Open Question 的可测试定义：**“仅含触发短语、且位于非代码/非引用/未被否定绑定的请求性文本”才算请求**。
- `.omp/extensions/trellis/plan-state.ts`：`type PlanMode = "inactive" | "planning"`；`PLAN_STATE_CUSTOM_TYPE = "com.trellis.plan-intent.state"`（避开核心保留值）；`PLAN_CONSTRAINT_CUSTOM_TYPE = "trellis-plan-intent-constraint"`；`entrySchema = { version: 1, state: PlanMode, enteredAt?: string, planModel?: string }`；`stateFromEntry(entry)` / `entryFromState(state)`。

### 修改 `.omp/extensions/trellis/index.ts`
- L1 附近：`import { classifyPlanIntent } from "./plan-intent"; import { ... } from "./plan-state";`
- default export（L438）内新增闭包状态：`let planMode: PlanMode = "inactive"`（会话级，与 `projectRoot` 同级）。
- 新增 helper（L438-454 区域）：`restorePlanState(ctx)`（扫描 `ctx.sessionManager.getBranch()` 取最新 `PLAN_STATE_CUSTOM_TYPE` 条目 → 设置 `planMode`；若 `planning` 且 `ctx.models.current()` 与 `ctx.models.resolve("@plan")` 不一致则 `pi.setModel(...)` 幂等重断言）、`enterPlanning(pi, ctx)`、`exitPlanning(pi, ctx)`、`notifyPlan(ctx, msg, type)`。
- 扩展 `session_start`（L455）：现有逻辑后追加 `restorePlanState(ctx)`（恢复时模型切换失败仅 notify 警告，状态仍按条目恢复——条目是持久真相）。
- 新增 `pi.on("session_switch", ...)`、`pi.on("session_branch", ...)`、`pi.on("session_tree", ...)`（紧随 L499 后）：都调 `restorePlanState(ctx)`（文档 `omp://extensions.md` 明确推荐这三个 + session_start 做状态重建）。
- 扩展 `input`（L576-583）：`if (isSubAgent) return;`（现有变量 L442）→ `const action = classifyPlanIntent(event.text)`：
  - `enter` 且 `planMode !== "planning"`：`ctx.models.resolve("@plan")` → `undefined` 则 notify(warning, "无法进入规划模式：modelRoles.plan 未配置或模型不可用") 并 return（不写条目、不切模型）；否则 `try { ok = await pi.setModel(planModel) } catch { ok = false }`；`ok === false` → notify(error, 可操作原因) 并 return；成功后 `planMode = "planning"`；`pi.appendEntry(PLAN_STATE_CUSTOM_TYPE, { version: 1, state: "planning", enteredAt: ..., planModel: "provider/id" })`；`ctx.hasUI && ctx.ui.notify("已进入规划模式", "info")`。
  - `exit` 且 `planMode === "planning"`：`resolve("@default")` → 失败/`setModel` false → notify(error)，**保持 planning 不变**（PRD 要求 6/AC5：失败时状态与模型保持原样）；成功 → `planMode = "inactive"`；appendEntry `{ state: "inactive" }`；notify("已退出规划模式")。
  - 其余情况（含已处于 planning 时再次 enter / 已 inactive 时 exit）：无操作（幂等，不重复写条目/不重复切模型）。
  - **事件顺序**：input（await 序列内完成 setModel）→ before_agent_start → context → agent_start → LLM 调用。input handler 被 `await runner.emitInput(...)` 等待（input-controller.ts:665），因此切模型对**当前这一轮**即生效（满足 AC1“下一次主代理调用”）。
- 新增第二个 `pi.on("before_agent_start", ...)` handler（与 L505 并列）：`if (isSubAgent || planMode !== "planning") return;` 返回 `{ message: { customType: PLAN_CONSTRAINT_CUSTOM_TYPE, content: <隐藏约束文本：只分析/出方案，禁止编辑/写入/实现类工具，等待用户明确批准>, display: false } }`。必须用**独立 handler**（每 handler 单 message 限制）。
- 扩展 `context`（L528）：反向扫描时把 `PLAN_CONSTRAINT_CUSTOM_TYPE` 与 `trellis-workflow-state` 一并检查/重注入（压缩安全网复用现有 `lastCompactionTs/lastInjectionTs` 机制；为两条消息各维护一套或合并）。
- 约束文本在压缩后仍有效的保证：custom 条目不参与压缩删除（append-only JSONL），`getBranch()` 压缩后仍含条目；压缩后 `session.compacting` 无需额外处理（可选：用 `preserveData` 冗余一份）。

### 失败处理汇总（PRD 要求 4/6/8、AC5）
- resolve 返回 undefined → 不进入、不改模型、不写条目；UI 给可操作错误（角色未配置/模型不可用）。
- setModel 返回 false 或 throw → 同上；提示“缺少可用认证/切换失败”。
- 退出时 @default 不可解析 → 保持 planning，提示用户配置 `modelRoles.default`（注意：这会让用户“卡在”规划态，属契约内行为，需在错误信息中说明）。
- 恢复/切分支时重断言失败 → 状态按条目恢复（planning），模型维持当前，仅警告（避免静默丢失约束）。

## 4. 测试入口（现有 + 最小新增）

- 仓库现有测试体系：根 `package.json` `"test": "... && vitest run"`（vitest 4.1.5，devDeps）；`apps/tests/*.test.mjs` 走 `node --test`（apps `test:runtime`）。**没有**任何 `.omp/` 扩展测试。
- 最小新增（推荐）：
  - `.omp/extensions/trellis/plan-intent.test.ts` + `plan-state.test.ts`（vitest，esbuild 直接转译 TS；根目录 vitest 默认 include `**/*.{test,spec}.*` 可覆盖 `.omp/`）。用输入样例表断言 enter/exit/none（含代码块、引用、否定、旁问、历史回顾）与状态机转移/幂等。
  - 接线测试（同目录 `index.test.ts`）：构造 mock `ExtensionAPI`（`pi.on` 注册表 + 记录 `setModel`/`appendEntry` 的调用数组），fixture 用 `mkdtemp` 建含 `.trellis/` 空目录的 cwd，逐事件驱动 `session_start → input → before_agent_start → context`，断言切换/条目/注入/失败路径。`ctx.models.resolve`、`ctx.sessionManager.getBranch`、`ctx.hasUI` 均可 mock。
  - 运行命令（聚焦、不触发根 `test` 的重构建）：`pnpm exec vitest run .omp/extensions/trellis`。
- 可选 E2E 冒烟（花 token，不建议进 CI）：`omp -p --cwd <含 .trellis 的 fixture> -e .omp/extensions/trellis/index.ts --no-session "<规划请求>"`（`-p` print 模式已确认存在），观察 `--model` 前模型 vs 规划请求后模型。

## 5. 未确认点（仓库与 OMP 文档/源码均无法闭合）

1. `input` 事件在 **RPC 模式**是否发射：源码注释 “interactive mode only”，唯一发射点 `input-controller.ts:664`（interactive）；`source: "rpc" | "extension"` 枚举存在但**未见调用点**。影响：RPC 下状态机可能收不到 input 事件（约束注入靠 before_agent_start 仍生效，恢复靠 session_start）。需运行时验证或按“RPC 不做进入/退出判定，仅注入与恢复”设计。
2. `pi.setModel` 在切换瞬间是否可能抛非“无认证”异常（`refreshSelectedModelMetadata`/provider reset 失败）：代码显示会 throw，但异常类型/消息未文档化；实现需 try/catch 兜底。
3. `session_start` 恢复时调用 `setModel` 会追加一条 `role: "default"` 的 `model_change` 条目（公开 API 无法指定 role）——核心恢复流程（switchSession 步骤 8）与扩展重断言之间的顺序/幂等未在文档中给出精确契约，需实测确认重复恢复不会产生多余的模型切换副作用。
4. `event.source === "extension"`（扩展自身 sendUserMessage 触发的 input）是否真实存在：未见发射点；若存在，需决定是否参与状态机（建议不参与，仅 interactive/rpc 用户输入）。
5. 显式 `--model` 运行时覆盖与规划态重断言的冲突语义：恢复时按“持久化状态为准”重断言 @plan，可能与用户显式 --model 冲突；PRD 未覆盖，实现需记录该决策。
6. `notify` 在 RPC 模式的行为（fire-and-forget `extension_ui_request`）：不阻塞、不依赖；状态正确性不依赖 UI（已满足要求 8），但 RPC 端是否显示通知未验证。
