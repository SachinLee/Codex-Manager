# 补齐本地模型目录数据

## Goal

直接更新本机 CodexManager SQLite 模型目录与路由元数据，使指定模型在 Codex 菜单中可见、可选，并始终声明兼容的 `high` 推理强度。

## Confirmed Facts

- 目标数据库为 `C:/Users/shuan/AppData/Roaming/com.codexmanager.desktop/codexmanager.db`。
- 运行时菜单和网关以 V2 表 `models`、`model_prices`、`model_price_tiers`、`model_routes` 为准；旧目录表 `model_catalog_models` 仍保存远程模型菜单元数据，需同步。
- `glm-5.2` 与 `kimi-k2.7-code` 已存在于旧目录，且各自已有三条已发现 aggregate API 来源；其余五个目标模型在本地没有已发现的上游来源。
- Kimi K2.7 Code 官方价格可查询，但用户决定当前不写入任何目标模型价格，后续自行补充。

## Requirements

1. 处理七个精确 slug：`deepseek-v4-flash`、`deepseek-v4-flash-0731`、`deepseek-v4-pro`、`glm-5.2`、`kimi-k2.7-code`、`mimo-v2.5-pro`、`qwen3.8-max`。
2. 每个模型必须在 V2 `models` 中启用、对 API 可用、可见，并有 Codex 可解析的最小文本模型能力元数据。
3. 每个模型的 `default_reasoning_effort` 必须为 `high`，能力 JSON 只声明 `reasoning_efforts: ["high"]`；不得凭空声明其他档位。
4. 每个模型必须在 `model_catalog_models` 中可见、对 API 可用，且默认推理强度为 `high`。
5. `glm-5.2` 与 `kimi-k2.7-code` 的 V2 aggregate 路由必须逐条同步已发现的 legacy 来源；其余模型仅建立默认 `account_pool/default` 路由，禁止伪造指向未知供应商的 aggregate 路由。
6. 不写入目标模型价格或价格层级；V2 价格记录状态保持 `missing`，供后续手动补充。
7. 同步更新 `C:/Users/shuan/.codex/models_cache.json`：保留既有缓存模型，并为七个 slug 提供可由 Codex 直接选择的最小文本模型记录。

## Out of Scope

- 不修改代码、迁移文件、内置种子或供应商配置。
- 不发现、探测、创建或启用未知 aggregate API 上游模型。
- 不编造上下文长度、输入/输出价格或额外推理档位。

## Acceptance Criteria

- [x] 七个 slug 在 V2 `models` 中各有一个可见、启用、API 可用的记录,默认推理强度为 `high`。
- [x] 每个 V2 记录仅声明 `reasoning_efforts: ["high"]`,并作为文本生成模型进入 Codex 管理目录。
- [x] 七个 slug 都出现在 `model_catalog_models` 中,默认推理强度为 `high`、`visibility=list`、`supported_in_api=1`。
- [x] `glm-5.2` 和 `kimi-k2.7-code` 的 V2 aggregate 路由与旧来源逐条对应;其余模型没有 aggregate API 路由。
- [x] 七个模型都没有价格层级,且关联 `model_prices.price_status` 均为 `missing`。
- [x] 在线 SQLite 备份可恢复,且只读 SQL 查询确认数量、菜单可见性、推理强度、路由范围和缺失价格均符合上述约束。
- [x] `models_cache.json` 保留原有八个缓存记录，并新增七个可见、API 可用、仅支持 `high` 推理强度的文本模型记录。

## Key Decisions

- 用户确认：无法可靠获取的价格暂不填写，后续自行补充。
- 用户确认：模型只写 `high` 推理强度，以保证 Codex 兼容。
- 使用直接数据库更新；先做 SQLite 在线备份，再在单一事务内写入。
