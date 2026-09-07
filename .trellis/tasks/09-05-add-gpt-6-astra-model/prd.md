# Add gpt-6-astra model catalog data

## Goal

将 GPT-6 Astra 加入 Codex Manager 的模型目录，使新安装和已有 SQLite 数据库都能看到该模型，并按 OpenAI 官方文档计费。

## Background and confirmed facts

- 模型目录 V2 的内置模型来源是 `crates/core/seeds/model_catalog_v2_2026_07_10.json`；`Storage::init()` 会通过 `seed_missing_builtin_models_v2()` 将缺失内置模型写入 SQLite。
- SQLite 模型数据分为 `models`、`model_prices`、`model_price_tiers`、`model_routes`，长上下文费用由价格 tier 选择。
- OpenAI 官方模型页 `https://developers.openai.com/api/docs/models/gpt-6-astra` 当前显示：上下文窗口 1,050,000 tokens，最大输出 128,000 tokens；标准输入 $10.00 / 1M、缓存输入 $1.00 / 1M、缓存写入 $12.50 / 1M、输出 $50.00 / 1M；超过 272K 输入 tokens 时整次请求按输入/缓存 2x、输出 1.5x 计费。
目前工作区 shell 未设置 `CODEXMANAGER_DB_PATH`，但用户已提供并确认运行时数据库为 `C:\Users\shuan\AppData\Roaming\com.codexmanager.desktop\codexmanager.db`；该数据库当前 `builtin_revision=8`、模型总数 21，尚无 `gpt-6-astra`。

## Requirements

- 新增 slug `gpt-6-astra`，显示名 `GPT-6 Astra`，可见、启用、支持 API，并作为模型目录首选排序项。
- 设置 `context_window=1050000`、`max_context_window=1050000`；在能力元数据中保留官方最大输出限制 `128000`，不把输出限制误写成输入上下文长度。
- 设置官方标准价格：输入 `10000000`、缓存输入 `1000000`、缓存写入 `12500000`、输出 `50000000`（micro-USD per 1M tokens）。
- 设置长上下文 tier 的阈值与倍率结果：`min_input_tokens=272001`，输入 `20000000`，缓存输入 `2000000`，缓存写入 `25000000`，输出 `75000000`；基础 tier 保持官方标准价格。
- 使用官方模型页作为价格来源，保证模型路由指向上游 slug `gpt-6-astra`，并保持内置模型写入幂等、不会覆盖用户编辑的模型。
- 更新模型目录 fixture 的 revision/校验元数据及对应存储测试断言；不改变现有模型价格或无关工作区改动。

## Acceptance Criteria

- [ ] 全新 SQLite 存储初始化后，`gpt-6-astra` 出现在可见模型列表中，字段、上下文长度、能力元数据、默认路由和价格状态正确。
- [ ] 已存在模型目录的 SQLite 存储再次初始化后，缺失的 `gpt-6-astra` 被幂等补齐，且现有用户编辑模型保持不变。
- [ ] 在 272K 长上下文边界前后选择到正确价格 tier，缓存写入价格和输出倍率与官方文档一致。
- [ ] 相关核心存储测试通过；运行数据库完成备份、事务写入，并通过完整性检查与只读查询核验。

## Out of scope

- 不新增上游供应商凭据、聚合 API 配置或 UI 专属逻辑。
- 不猜测或伪造官方未公开的模型快照、速率限制、音频/视频能力；只配置模型目录当前需要的字段。

## Open questions

- 无阻塞的产品决策；官方参数已通过模型页确认。

