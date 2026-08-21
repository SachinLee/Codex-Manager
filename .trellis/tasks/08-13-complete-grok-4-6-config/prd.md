# 补全 Grok 4.6 模型配置

## Goal

补齐本机 CodexManager 与 OMP 对 `grok-4.6` 的模型元数据，使模型选择器能够读取 200k 上下文、图像输入和可控推理强度；保持现有网关路由、价格和 fallback 语义不变。

## Confirmed Facts

- CodexManager 数据库：`C:/Users/shuan/AppData/Roaming/com.codexmanager.desktop/codexmanager.db`。
- 当前 V2 `models.slug='grok-4.6'` 已是 `origin='custom'`、启用、可见、API 可用，`default_reasoning_effort='high'`。
- 当前能力字段已有 `inputModalities=['text','image']`，但上下文窗口为 `NULL`；此前能力字段曾包含 `low/high`，本次按用户决定扩展为 `low/medium/high/xhigh`。
- 当前 OMP provider 使用 `openai-models-list`；网关公开 `/v1/models` 的 `data[]` 没有上下文窗口、推理档位或完整模态元数据。
- OMP 内置/curated 目录有 `grok-4.5`，没有 `grok-4.6`，因此缓存中的 `grok-4.6` 退化为 `reasoning=false`、文本输入、128000 context、32768 maxTokens。
- OMP 的 Grok 推理努力白名单包含 `grok-4.5`，不包含 `grok-4.6`。
- 用户已确认：推理档位为 `low/medium/high/xhigh`；上下文窗口按推荐方案采用 `200000`。
- 已有本地备份与前序数据修复记录；所有进一步写入必须先生成带时间戳的新备份，不覆盖旧备份。

## Requirements

1. 将 CodexManager V2 `grok-4.6` 的 `context_window` 与 `max_context_window` 补为 `200000`，默认推理强度保持 `high`，能力推理档位为 `low/medium/high/xhigh`，输入模态保留 `text/image`。
2. 补齐 OMP 对 `grok-4.6` 的静态模型覆盖或等价缓存数据，使其具备 `reasoning=true`、`thinking.mode=effort`、推理档位 `low/medium/high/xhigh`、图像输入、`contextWindow=200000` 和不超过该窗口的 `maxTokens`。
3. 让 OMP 的 Grok 推理努力识别逻辑覆盖 `grok-4.6`，并保证请求兼容配置不会错误省略 `reasoning.effort`。
4. 保留现有 `grok-4.5`、其他 OMP provider 模型及模型缓存；修改范围只针对 `grok-4.6` 及其必要的共享 Grok 配置。
5. 修改后清理/重建受影响的 OMP provider cache，并通过 SQLite 查询、配置解析和模型发现结果验证最终数据。
6. 所有修改均可通过修改前备份回滚；不提交本机用户目录数据到仓库。

## Out of Scope

- 不修改仓库 Rust/TypeScript 源码、迁移文件、内置种子或 OMP 安装包源码。
- 不新增或启用 aggregate API 路由，不改变价格、价格层级、模型排序或 fallback。
- 不把 OMP 的 `grok-4.5` 500k curated 元数据写回 CodexManager 数据库。
- 不修改 `grok-4.6` 的 vision 角色绑定；只补齐其模型能力元数据。

## Acceptance Criteria

- [x] `grok-4.6` 的 CodexManager V2 数据为 200000 context / max context，默认 effort 为 `high`，能力 efforts 精确包含 `low/medium/high/xhigh`，输入模态包含 `text/image`。
- [x] OMP 模型数据不再退化为 `reasoning=false`、`input=['text']`、128000/32768 默认值。
- [x] OMP 模型选择器能显示 `grok-4.6` 的推理强度，档位为 `low/medium/high/xhigh`，图像输入元数据保留，脚注显示约 200k context；`omp models find grok --json` 已返回目标字段。
- [x] 既有 `grok-4.5`、路由、价格、fallback、计费快照数量和其他 provider cache 未被意外改变；目标行仍保留 1 个 price、2 个 tiers、4 个 routes、880 个 charge snapshots，fallback 仍为 `gpt-5.6-terra`。
- [x] 修改前备份存在，SQLite 外键检查通过，受影响 OMP provider cache 可解析；7 个带时间戳备份文件已生成。

## Key Decision

- 上下文窗口采用 `200000`，因为它与当前 CodexManager 数据和长上下文价格 tier 语义一致；不从 `grok-4.5` 的 OMP 500000 curated 值推断 `grok-4.6`。
- 推理档位采用用户确认的 `low/medium/high/xhigh`，不加入 `minimal` 或 `max`。

## Open Questions

- 无阻塞性产品决策。OMP 具体采用本地静态 `models.yml` 覆盖还是直接重建 SQLite cache，将在实现前按现有 schema 与刷新机制选择最小、可回滚方案。
