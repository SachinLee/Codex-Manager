# 实施 OMP P0 会话延迟修复

## Goal

以最小、可回滚的全局 OMP 配置变更，移除已证实失败或重复的 MCP 启动工作，并让标题生成失败不再拖慢主会话。

## Confirmed Facts

- 近期启动日志出现 Postman 401、OpenAI Developer Docs 403、IDA Pro 无法连接、Codex MCP transport closed，以及 `context7` 和 `Context7` 的重复工具冲突。
- 最近一次正常但受污染的启动完成 MCP 刷新耗时约 22.43 秒。
- 标题生成使用 DeepSeek Flash；同一会话出现 4 次 502，单次等待约 19.9–72.3 秒。

## Requirements

1. 定位 MCP 和标题生成的权威配置来源；不得凭日志猜测后删除配置。
2. 移除或禁用不应在当前 OMP 会话加载的失效 MCP，保留仍在使用的能力。
3. 消除 Context7 重复注册，保留一个可用实例。
4. 将标题生成改为不会对主会话造成长重试等待的安全配置；若没有受支持配置项，停止并报告源码/产品缺口。
5. 修改前创建可恢复备份；修改后验证配置可被 OMP 读取、冷启动错误减少、标题链路不再长重试。

## Out of Scope

- 不变更默认主模型、模型计费、项目代码或用户会话历史。
- 不删除未知用途或尚未确认来源的 MCP。
- 不通过改写日志、屏蔽错误或降低全局安全检查来伪造提速。

## Acceptance Criteria

- [x] 已标明每个移除/保留 MCP 的配置来源与理由；详见 `research/verification.md`。
- [~] 权威配置已不再包含 Context7 重复项或已移除 MCP；嵌套 OMP smoke 受当前 `PI_TOOL_BRIDGE_*` 宿主桥污染，仍显示桥接注入的旧清单，未冒充为通过。
- [x] 标题生成已切换到 `codex-manager/gpt-5.6-terra:low`；实测请求约 2.2 秒成功，主会话返回 `OMP_P0_OK`。
- [x] 存在修改前配置备份和明确回滚路径：`research/config-backup.md` 及 Claude 完整自动备份。
- [x] 已记录修改前后可复核启动/会话验证结果和验证限制。

## Open Questions

- [x] 本机没有可用的持久化 `no-title` 配置项；本地 LFM2 下载以 subprocess code 3 失败，因此采用受支持的低延迟 Terra 标题角色作为安全降级。

