# OMP/Trellis workflow-state 缓存稳定性修复指南

## 用途

这份文档用于以下场景：

- 其他项目的 `.omp/extensions/trellis/index.ts` 出现相同的 Responses prompt cache 问题。
- Trellis 更新后覆盖了项目级修复，需要重新应用。
- AI 需要根据现有项目代码完成最小修复，而不是修改 OMP provider 或关闭 Trellis。

目标是保留 Trellis 功能，同时保证每次 OMP `context` 投影中最多存在一个 `trellis-workflow-state` 消息。

## 症状

典型表现包括：

- 长会话中 `cached_input_tokens` 反复停在一个固定的较低值，例如 `18944`。
- 同一会话重启或重新打开后，缓存前缀暂时重新增长。
- `prompt_cache_key` 和 tools fingerprint 不变，但 provider `instructions` 的长度或 hash 持续变化。
- 每轮 tool-use continuation 或压缩后的上下文中，重复出现相同的 `trellis-workflow-state`。
- `trellis-workflow-state` 副本数可能从 `1` 增长到 `2`、`3`、`4`。

`18944` 不是 context window 上限。它可能是静态 instructions 前缀在 128-token cache granularity 下的 rounded boundary。例如 `nonMessageTokens=19274` 时：

```text
floor(19274 / 128) * 128 = 18944
```

## 已确认的根因

OMP 的事件链可能同时执行：

1. `before_agent_start` 返回一个新的 `trellis-workflow-state` custom message。
2. `context` 在每次 LLM API 调用前运行，包括 tool-use continuation 和 compaction 后的调用。
3. `context` 只检查已有 workflow-state 是否存在，却没有删除全部副本并重新投影。

结果是同一 workflow 状态被不断追加。即使消息内容相同，最终 provider instructions 的序列化输入仍可能发生变化，导致 prefix cache 不再命中稳定的长前缀。

注意：这不是所有 cache miss 的唯一可能原因。tool-result pruning、历史重写、compaction、provider replay 和路由变化也可能造成一次 cache cold request。但重复 workflow-state 是项目级 Trellis 扩展中必须先消除的确定性问题。

## 修复不变量

修复后的扩展必须满足：

- 每次 `context` 事件中，`trellis-workflow-state` 数量只能是 `0` 或 `1`。
- 当前 turn 需要 workflow state 时，保留一个最新内容。
- `no-trellis` turn 时，删除已有 workflow-state，保留 `0` 个。
- workflow 内容不变、task context 不变时，重复调用 `context` 应返回 `undefined`，不重新构建消息数组。
- `trellis-task-context` 继续最多保留一个，并在内容变化时替换。
- `trellis-session-context`、task 状态解析、Bash `TRELLIS_CONTEXT_ID`、子代理上下文和 compaction 恢复能力继续存在。
- 不修改 provider 的 `buildResponsesInput`、`prompt_cache_key`、`previous_response_id` 或路由逻辑。
- 不通过 `--no-extensions` 作为长期解决方案，也不关闭 Trellis。

## 推荐修改策略

### 1. 先检查目标文件

读取并搜索：

```text
.omp/extensions/trellis/index.ts
```

重点搜索：

```text
before_agent_start
context
session_before_compact
trellis-workflow-state
lastInjectionTs
lastCompactionTs
```

确认当前文件是否仍有下面的危险结构：

```ts
pi.on("before_agent_start", async (...) => {
  return {
    message: {
      customType: "trellis-workflow-state",
      ...
    },
  };
});
```

以及下面这种只扫描、不归一化的逻辑：

```ts
for (...) {
  if (message.customType === "trellis-workflow-state") {
    return ...;
  }
}
```

### 2. 将 workflow-state 的唯一注入点移到 `context`

`before_agent_start` 可以保留用于：

- 初始化 `projectRoot`。
- 预热 `turnCache`。

但它不应再返回 workflow-state message。workflow-state 只应由 `context` handler 投影。

示例结构：

```ts
pi.on("before_agent_start", async (_event, ctx) => {
  if (!projectRoot) {
    projectRoot = findProjectRoot(ctx.cwd);
  }
  if (!projectRoot) return;

  const contextKey = rememberContextKey(ctx);
  turnCache.get(projectRoot, contextKey);
});
```

如果 `before_agent_start` 不再承担任何初始化职责，可以删除它；不要保留一个返回 workflow-state 的旧 handler。

### 3. 在 `context` 中统一归一化

核心流程：

1. 读取当前 task context。
2. 读取当前 turn 的 cached workflow message。
3. 收集全部 task-context 和 workflow-state 消息。
4. 如果两类内容都没有变化，直接返回 `undefined`。
5. 重建消息数组：
   - task context 最多保留一个最新副本。
   - 删除所有 workflow-state 副本。
   - 若当前 turn 非 skip，则插入一个最新 workflow-state。
6. 返回新的消息数组。

参考实现骨架：

```ts
pi.on("context", async (event, ctx) => {
  if (!projectRoot) return;

  const contextKey = rememberContextKey(ctx);
  const messages = event.messages as {
    role?: string;
    customType?: string;
    content?: string;
  }[];

  const { taskDir } = resolveActiveTaskStatus(projectRoot, contextKey);
  const currentTaskContext = taskDir
    ? getTaskContext(taskDir, projectRoot)
    : "";

  const taskContextMessages = messages.filter(
    (message) => message.customType === "trellis-task-context",
  );
  const existingTaskContext = taskContextMessages[0]?.content ?? "";
  const taskContextChanged =
    existingTaskContext !== currentTaskContext || taskContextMessages.length > 1;

  const cached = turnCache.get(projectRoot, contextKey);
  const workflowMessages = messages.filter(
    (message) =>
      message.role === "custom" &&
      message.customType === "trellis-workflow-state",
  );
  const workflowChanged = cached.workflowMsg
    ? workflowMessages.length !== 1 ||
      workflowMessages[0]?.content !== cached.workflowMsg
    : workflowMessages.length > 0;

  if (!taskContextChanged && !workflowChanged) return;

  const taskReplacement = currentTaskContext
    ? {
        role: "custom" as const,
        customType: "trellis-task-context",
        content: currentTaskContext,
        display: false,
        timestamp: Date.now(),
      }
    : null;

  let taskReplaced = false;
  const projectedMessages = messages.flatMap((message) => {
    if (message.customType === "trellis-task-context") {
      if (taskReplaced || !taskReplacement) return [];
      taskReplaced = true;
      return [taskReplacement];
    }

    if (
      message.role === "custom" &&
      message.customType === "trellis-workflow-state"
    ) {
      return [];
    }

    return [message];
  });

  if (taskReplacement && !taskReplaced) {
    projectedMessages.push(taskReplacement);
  }

  if (cached.workflowMsg) {
    projectedMessages.push({
      role: "custom" as const,
      customType: "trellis-workflow-state",
      content: cached.workflowMsg,
      display: false,
      timestamp: Date.now(),
    });
  }

  return { messages: projectedMessages };
});
```

项目当前修复保留了已有 workflow message 的大致位置；将它追加到稳定的投影位置也可以，但必须保证后续相同输入不会继续移动或重建它。若选择保留原位置，需要正确计算删除重复 task context 后的新索引。

## Skip keyword 行为

`input` handler 应继续执行现有 skip 判断：

```ts
const skipThisTurn = shouldSkipWorkflowState(event.text ?? "", skipKeyword);
turnCache.beginTurn(skipThisTurn);
```

当 `cached.workflowMsg` 为空且历史消息中存在 workflow-state 时，`context` 必须删除它们并返回新消息数组。不能因为没有新 workflow 内容就直接返回 `undefined`。

预期结果：

```text
普通 turn: 0 -> 1，或 N -> 1
no-trellis turn: N -> 0
稳定重复调用: 1 -> unchanged
```

## 不要采用的修复

以下方案不能作为本问题的项目级长期修复：

- 关闭所有 extensions。
- 关闭 Trellis。
- 修改 provider 的 cache key 生成逻辑。
- 强行启用 `previous_response_id` chaining。
- 把 workflow-state 放进每轮变化的 system prompt。
- 只在 `before_agent_start` 中加一个布尔锁。
- 只扫描最后一个 workflow-state，不删除前面的副本。
- 只删除副本但每次都重新创建相同消息数组。
- 修改 `.trellis/.template-hashes.json` 伪装模板未被用户修改。
- 使用 `trellis update --force` 覆盖本地修复。

## 验证清单

### 静态验证

```bash
bun build .omp/extensions/trellis/index.ts \
  --target bun \
  --external '@oh-my-pi/pi-coding-agent' \
  --outdir <temporary-output>

git diff --check -- .omp/extensions/trellis/index.ts
```

确认：

- 文件可以被 Bun 构建。
- 没有遗留 `lastInjectionTs` / `lastCompactionTs` 的无效引用。
- `before_agent_start` 不再返回 workflow-state message。
- `context` 同时处理 workflow-state 和 task-context。

### Handler-level 验证

使用 mock `ExtensionAPI` 注册 handler，不记录任何 prompt body，只记录：

- 每次输出的 message 数量。
- `customType` 数量。
- content 长度和 hash。
- instructions 长度和 hash（若 provider probe 可用）。

至少覆盖：

1. 输入两个或多个不同 workflow-state，输出应为一个最新副本。
2. 对第一次输出再次调用 context，应返回 `undefined`。
3. 输入一个旧 workflow-state 后执行 `no-trellis`，输出应为零个 workflow-state。
4. task context 变化时，task context 应刷新且仍然最多一个。
5. task context 不变时，不应每次重建上下文。

示例断言：

```text
firstWorkflowCount === 1
secondContextResult === undefined
skipWorkflowCount === 0
```

### RPC 验证

在启用项目 Trellis extension 的情况下运行至少 4 轮 tool-use continuation，记录 metadata-only 输出：

- context event 序号。
- workflow-state 数量。
- task-context 数量。
- workflow content hash。
- instructions length/hash。
- prompt cache key hash。
- tools hash。
- request timestamp、model、source/session metadata。

不记录：

- prompt body。
- Authorization header。
- API key。
- 完整 provider input。

预期：

- workflow-state 数量始终为 `0` 或 `1`，不再递增。
- workflow 内容不变时 instructions hash 稳定。
- prompt cache key 和 tools hash 的既有行为不变。
- compaction 后允许第一请求 cold，但后续 cache 应重新增长。

如果 RPC 在 `resolveModelDiscoveryFallbackNonRuntime` 启动阶段超时，应单独记录为 OMP 启动环境问题，不能作为扩展逻辑失败证据。

## Trellis 更新后的恢复流程

项目级文件通常受 `.trellis/.template-hashes.json` 管理。修复后该文件 hash 会与模板记录不同，这是预期的用户修改状态。

当 Trellis 模板被更新或覆盖时：

1. 先保存当前 `.omp/extensions/trellis/index.ts` 的本地 diff。
2. 运行普通 `trellis update`，或使用：

   ```bash
   trellis update --create-new
   ```

3. 比较新的 `.new` 模板与当前文件。
4. 将本指南中的 workflow-state 幂等投影逻辑合并回项目文件。
5. 重新运行构建、handler-level 测试和 metadata-only probe。
6. 不要选择 `Overwrite`，不要使用 `--force` 覆盖本地修复。

长期方案是把同样的修复提交回 Trellis 模板源：

```text
packages/cli/src/templates/omp/extensions/trellis/index.ts.txt
```

在上游修复发布前，各项目仍需保留自己的项目级 patch。

## 可直接交给 AI 的修改提示

```text
请修复当前项目的 OMP/Trellis workflow-state 重复注入问题。

目标文件：.omp/extensions/trellis/index.ts

背景：OMP 的 before_agent_start 和 context 都可能参与 workflow-state 注入。tool-use continuation 和 compaction 后 context 会重复运行，导致 trellis-workflow-state 从 1 个增长到多个，改变 provider instructions 前缀并破坏 prompt cache 稳定性。不要把问题归因到 provider cache key，也不要关闭 Trellis。

要求：
1. 先读取项目规范、目标文件和 git diff，保留用户现有修改。
2. 搜索所有 before_agent_start、context、trellis-workflow-state、task-context 和 skip keyword 逻辑。
3. 将 workflow-state 的唯一注入点放到 context projection。before_agent_start 可以预热 turn cache，但不得再返回 workflow-state message。
4. 每次 context 处理时，删除所有 trellis-workflow-state；当前 turn 非 skip 时只插入一个最新副本，skip 时保留零个。
5. task-context 继续最多保留一个；内容变化时替换，内容不变时不要重复重建消息数组。
6. 保留 session context、task 状态解析、子代理 context、compaction 后恢复、Bash TRELLIS_CONTEXT_ID 和 no-trellis 行为。
7. 不修改 OMP provider、Responses 输入构建、prompt_cache_key、路由、previous_response_id 或全局配置。
8. 不修改 .trellis/.template-hashes.json，不使用 trellis update --force，不关闭 extensions。
9. 添加或执行 metadata-only handler 验证：多个 workflow 副本必须变成 1 个；再次处理稳定结果必须返回 undefined；no-trellis 必须变成 0 个。
10. 执行 Bun build 和 git diff --check。若 RPC 因 resolveModelDiscoveryFallbackNonRuntime 启动超时失败，请将其单独报告为环境阻塞，不要伪称为扩展逻辑失败。

完成标准：每个 context event 中 workflow-state 数量为 0 或 1；workflow 内容不变时 instructions hash 稳定；Trellis 其他能力保持可用；未修改 provider 或全局设置。
```
