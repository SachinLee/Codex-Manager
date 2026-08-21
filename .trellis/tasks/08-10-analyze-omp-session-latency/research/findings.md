# OMP 会话时延调查结果

调查日期：2026-08-10。范围：只读检查 `C:/Users/shuan/.omp/agent/config.yml`、`models.yml`、`agent.db`、OMP 日志及会话 JSONL；未修改 OMP 或产品配置。

## 结论

主要瓶颈不是单一的本机工具执行，而是四项固定开销叠加：

1. **模型首响本身偏慢**。`agent.db:model_perf` 的历史均值显示：默认 `codex-manager/gpt-5.6-terra` 的 TTFT 约 16.84 秒；`codex-manager/gpt-5.6-sol` 约 31.60 秒；标题生成使用的 `aiswitch-china/deepseek-v4-flash-0731` TTFT 约 41.86 秒、平均生成耗时约 48.80 秒。
2. **每次 OMP 进程启动都会探测/加载大量外部能力**。最近的 PID 13412 从首条 TTSR 注册到 Context7 刷新完成约 22.43 秒（10:36:06.406–10:36:28.838）。PID 7468 约 9.64 秒；PID 18652 还出现 Codex 与 Context7 连接 30 秒超时，说明冷启动在故障网络下可更长。
3. **存在失效 MCP 和重复 MCP**。日志持续报告 Postman 401、OpenAI Developer Docs 403、IDA Pro 无法连接、Codex transport closed；同时 `context7` 与 `Context7` 同时暴露同名工具并产生 collision。失败连接与重复注册会把本应一次完成的启动变成等待、重试和刷新。
4. **标题生成在失败时重复等待**。同一 session `019fe986-fe50-7000-82ad-0d6d84841dec` 在 10:39:17、10:40:21、10:41:46、10:45:39 发起四次 DeepSeek 标题请求，分别约 24.2 秒、19.9 秒、21.7 秒、72.3 秒，均以 502 失败；仅这条后台链路就贡献约 2 分 18 秒的网络等待（不含两次间隔）。

## 证据解释

- `config.yml` 将 `default` 设为 `codex-manager/gpt-5.6-terra:high`，将 `slow` 设为 `codex-manager/gpt-5.6-sol:max`；`retry.enabled=true`、`maxRetries=2`、`modelFallback=true`。因此当前默认路径不是最低延迟模型，失败时还可能进入重试/回退。
- 同一份配置将 `task.maxConcurrency=4`、`maxRecursionDepth=1`。子代理启动准备阶段本身很快（最近日志的 `invokeToFirstChatMs` 约 376–1123ms），但子代理的真正模型/工具工作可能持续很久；PID 18652 的一个会话启动了 5 个 review 子代理，且出现 09:18–09:20 才陆续退出，属于任务级放大项，不是每个简单会话的固定成本。
- 日志有多次 `ui.loop-blocked`，严重样本为 5259ms、6084ms，另有 1611ms、1994ms；这表明本地事件循环存在卡顿。它会放大“界面看起来一直没结束”的感受，但现有日志不足以证明它是主要总耗时来源。
- AutoQA push 在最近进程中多次返回 HTTP 400，在旧进程中出现超时。它更像后台故障/重试噪声；若该后台任务与主事件循环耦合，会进一步造成尾部延迟，需单独做开关对照确认。
- OMP 项目会话 JSONL 显示本次调查从 02:38:21Z 开始，到 02:49:10Z 仍在进行。该时间包含 Trellis 任务确认、工具调用和调查动作，不能直接等同于单次模型请求耗时；它证明当前工作流层也会显著延长“完整会话”墙钟时间。

## 优先级建议

### P0：先清理启动阶段

- 暂时禁用当前不使用且明确失败的 Postman、OpenAI Developer Docs、IDA Pro、Codex MCP。
- 只保留一个 Context7 实例，删除 `context7` / `Context7` 的重复配置之一。
- 对每个 MCP 做一次冷启动前后对照：记录首条日志、最后一次工具刷新和第一条模型请求时间。

### P1：隔离非核心后台请求

- 标题生成失败不应阻塞主会话收尾；标题请求应改为单次短超时、失败即跳过，或使用本地标题回退。
- AutoQA push 失败不应同步占用主事件循环；关闭 AutoQA 做一次 A/B 对照，确认 `ui.loop-blocked` 与会话尾延迟是否下降。

### P1：按任务选择模型

- 简单问答/小修改不要使用 `high`/`max` 路径；优先测试低延迟模型。当前性能表里 Terra 明显快于 Sol，标题 DeepSeek 明显慢，不适合承担同步标题任务。
- 对需要多个审查代理的任务，限制并行审查数量；`maxConcurrency=4` 只限制并行度，不限制总工作量。

## 可复核验证

1. 记录一次关闭失效 MCP、关闭标题生成和关闭 AutoQA 后的冷启动时间、首响时间、收尾时间。
2. 用同一短 prompt 连续运行 5 次，分别使用 Terra 与低延迟模型，比较 TTFT、完整 turn 时长和错误重试次数。
3. 对比日志中的 `title-generator:*`、`MCP tool load failed`、`MCP tool name collision`、`ui.loop-blocked` 与用户可见的等待区间。
4. 若仍有长尾，再检查 provider HTTP 请求的连接/读取 timeout 与代理网络；当前证据已经足以优先处理 MCP 和标题后台链路。

## 限制

- 当前可见日志记录了 OMP 进程和部分子代理时序，但没有统一的“用户发送到最终 UI 完成”的单一 duration 字段。
- 因此“模型、启动 MCP、标题重试是主要来源”是有日志和性能表支撑的高置信结论；AutoQA 与 UI loop 对总耗时的精确占比仍需 A/B 实验。
