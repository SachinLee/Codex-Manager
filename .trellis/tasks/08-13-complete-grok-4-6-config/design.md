# Technical Design

## Boundaries

本次是本机运行时数据修复，不改仓库产品源码：

- CodexManager SQLite：`models` 行及其 `capabilities_json`，只改 `grok-4.6` 的窗口和推理元数据。
- OMP provider 配置/cache：为 `codex-manager/grok-4.6` 提供静态模型元数据，避免 `openai-models-list` 的默认退化。
- OMP Grok identity/compat 覆盖：确保 `grok-4.6` 被识别为支持 effort 的 Grok SKU。

## Data Flow

```text
CodexManager models row
  -> gateway /v1/models
  -> OMP openai-models-list discovery
  -> bundled/static metadata merge
  -> OMP model_cache
  -> model picker + request compat
```

网关公开 `data[]` 目前不携带完整能力，因此 OMP 必须通过本地静态覆盖或等价 cache 注入 `reasoning/thinking/input/contextWindow/maxTokens/compat`。

## Target Contract

```json
{
  "id": "grok-4.6",
  "name": "Grok 4.6",
  "reasoning": true,
  "thinking": { "mode": "effort", "efforts": ["low", "medium", "high", "xhigh"] },
  "input": ["text", "image"],
  "contextWindow": 200000,
  "maxTokens": 200000,
  "compat": {
    "supportsReasoningEffort": true,
    "omitReasoningEffort": false
  }
}
```

CodexManager 侧保持 snake/camel 能力字段兼容：`reasoningEfforts` 与 `reasoning_efforts` 均写入同一组四档；`default_reasoning_effort` 保持 `high`。

## Implementation Choice

使用 OMP 已支持的 provider-level `modelOverrides`，未修改 npm 安装包源码。该覆盖在动态 discovery 结果合并后应用，实测 CLI 输出已覆盖同名 `grok-4.6` 的默认退化数据。

OMP Grok effort 白名单源码没有用户配置级入口；本次通过同名模型 override 显式设置 `supportsReasoningEffort=true` 与 `omitReasoningEffort=false`，实际解析结果已验证。

## Safety and Rollback

1. 修改 SQLite 前复制主库、WAL、SHM 到新时间戳备份。
2. 修改 OMP `models.yml` / `models.db` 前复制对应文件。
3. 对 SQLite 使用单事务和 FK 检查；不动路由、价格、key、快照。
4. 记录修改前后 grok-4.5、grok-4.6 以及 provider cache 的 hash/行数；异常时停止并从备份恢复。

## Verification

- SQLite：查询目标行、能力 JSON、相关子表计数、`PRAGMA foreign_key_check`。
- OMP：解析 YAML；读取 `model_cache`，验证 target contract；确认其他 provider/model 数量及 grok-4.5 spec 未变。
- Runtime：刷新或重启 OMP discovery，确认 `grok-4.6` 显示 200k、image 和四档 effort；执行最小模型解析/请求兼容检查。
