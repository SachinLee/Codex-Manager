# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目定位

CodexManager 是一个同时承载"桌面端"和"独立 Service"两种运行形态的 Codex 账号池 + 本地网关。仓库里并存**三种不同的构建世界**,任何改动前必须先判断自己落在哪一层:

- **Next.js 前端** — `apps/src/`(App Router + TS + Tailwind v4 + shadcn + TanStack Query + Zustand)
- **Tauri 桌面壳(Rust)** — `apps/src-tauri/`(独立 Cargo 项目,被工作区 `exclude`)
- **Rust 工作区** — `crates/core | service | web | start`(`Cargo.toml` 里的 workspace,四个 crate)

桌面模式 = 前端 + `apps/src-tauri` + 内嵌 `codexmanager-service`。
Service 模式 = `codexmanager-service` + `codexmanager-web` + `codexmanager-start`(一键启动器)。

## 常用命令

所有前端命令必须带 `-C apps`(pnpm 工作区)。PowerShell 命令在 Windows 上用 `pwsh`;Linux/macOS 用 `bash`。

### 前端(apps/)

```bash
pnpm -C apps install
pnpm -C apps run dev                # 普通开发服务器
pnpm -C apps run dev:desktop        # 127.0.0.1:3005,供 Tauri 连接
pnpm -C apps run build:desktop      # 静态导出(默认前端验证入口)
pnpm -C apps run lint
pnpm -C apps run test:runtime       # node --test 跑 tests/*.test.mjs
pnpm -C apps run test:e2e           # Playwright
pnpm -C apps run test:navigation    # 只跑导航缓存用例
```

注意:`build` 即 `next build`。前端默认验证用 **`build:desktop`**,不是 `lint`。

### Rust 工作区(crates/)

```bash
cargo test --workspace
cargo test -p codexmanager-service                 # 单 crate
cargo test -p codexmanager-web
cargo build -p codexmanager-service --release
cargo build -p codexmanager-web --release
cargo build -p codexmanager-start --release
```

桌面壳 `apps/src-tauri` 不在 workspace 里,它的测试跟随 `scripts/rebuild*` 触发,或单独 `cargo test --manifest-path apps/src-tauri/Cargo.toml`。

### 桌面端打包

```powershell
pwsh -NoLogo -NoProfile -File scripts/rebuild.ps1 -Bundle nsis -CleanDist -Portable
pwsh -NoLogo -NoProfile -File scripts/rebuild.ps1 -DryRun              # 只验证流程
```

Linux / macOS:`scripts/rebuild-linux.sh`、`scripts/rebuild-macos.sh`。

### 协议适配回归

改动涉及 `crates/service/src/gateway/`、`crates/service/src/http/`、`crates/service/src/lib.rs` 时,`cargo test --workspace` 只是最低底线。`docs/zh-CN/TESTING.md` 第 6 节列出必须覆盖的路径:`/v1/responses`、`/v1/chat/completions`、流式 SSE、非流式、`tools`、`tool_calls`。仓库内没有 `scripts/tests/` 目录,相关 PowerShell 探针只在文档中提到 —— 需要运行它们时先与用户确认来源。

## 按边界找入口

### 前端 → 桌面 → 服务的链路

前端**不要直接 `fetch`**。桌面端所有 IPC 走 `apps/src/lib/api/transport.ts` 里的 `invoke` / `invokeFirst`,并用 `withAddr()` 注入后端服务地址;命令名遵循后端下划线 ↔ 前端 camelCase 的映射。新增 Tauri 命令时必须同步在 `apps/src/lib/api/` 补一份封装。

相关层次:
- `apps/src/app/` 路由与页面(accounts / apikeys / aggregate-api / plugins / models / logs / settings / author)
- `apps/src/hooks/` 业务逻辑 hook(`useAccounts`、`useApiKeys`、`useDashboardStats` 等)
- `apps/src/lib/api/` 后端客户端封装
- `apps/src/store/`、`apps/src/components/` — 严格维持"dumb 组件 + hook 承载逻辑"

### Rust service 架构

`crates/service/src/gateway/` 是本仓库最复杂的域,子目录职责不能错:

- `auth/` 上游鉴权补全、token exchange、OpenAI fallback
- `request/` 进入 gateway 前的请求规范化与改写
- `routing/` 选路、cooldown、failover、route quality(`selection.rs`、`route_hint.rs`、`route_quality.rs`)
- `upstream/` 候选管理、超时/重试/退避、代理、transport、各上游协议实现
- `protocol_adapter/` 产出内部统一请求结构(`mod.rs`、`request_router.rs`)
- `observability/` trace / request log / metrics(`http_bridge.rs` 是已知偏厚文件)
- `model_picker/` 模型选择与 `/v1/models` 目录
- `local_validation/` 本地前置校验
- `core/` gateway 运行时配置

典型链路:`request` → `routing` → `auth` / `upstream` 发送 → `protocol_adapter` 产出元数据 → `observability` 落日志。

详细子目录文档与改动建议见 `crates/service/src/gateway/README.md`。

### 数据库

SQLite,迁移目录 `crates/core/migrations/`,严格按编号 `NNN_name.sql` 递增追加。新增表 / 列时要同步检查 `crates/core/src/storage/`、`crates/service/src/storage/` 与 `crates/service/src/app_settings/`。当前工作分支(`codex/custom-features`)近期新增的迁移围绕 `request_token_stats_aggregate_api`、`aggregate_api_model_override / cost_multiplier / daily_spend_limit`、`api_key_quota_limits` — 与 API 成本计算、配额与限额相关。

### 设置 / 环境变量 / 持久化

运行时可调配置走:前端 `appSettings` → RPC → `crates/service/src/app_settings/` → SQLite `app_settings` 表。启动前必须生效的配置走 `CODEXMANAGER_*` 环境变量。新增设置项必须同时考虑:桌面端、service、web 三端行为是否一致,以及默认值是否明确写入。路由策略 `gateway.route_strategy`(`ordered` / `balanced`,别名 `round_robin` 归一化为 `balanced`)、`gateway.free_account_max_model`、`gateway.request_compression_enabled`、`gateway.account_max_inflight` 这些键是已有约定,格式固定。

## 需要警惕的"厚文件"与写入边界

`CONTRIBUTING.md` 第 3.2、3.3 节定义了仓库级大文件阈值与高风险清单。改下列文件前先考虑拆分,不要继续无脑追加:

- `apps/src/main.js`(历史入口,逐步下沉到 `apps/src/runtime/` 与 `apps/src/settings/`)
- `apps/src-tauri/src/lib.rs`
- `crates/service/src/lib.rs`
- `crates/service/src/gateway/protocol_adapter/response_conversion.rs`(历史高兼容分支,回归风险大)
- `.github/workflows/release-all.yml`(唯一发布 workflow)

阈值:TS/JS `>500` 预警、`>800` 必须解释;Rust `>400` 预警、`>700` 必须解释;YAML `>250` 预警。

## 语言与提交约定

- 用户要求**中文回复**(来自全局 `~/.claude/CLAUDE.md`)。
- 提交信息以中文为主,单次提交只解决一类问题。PR 描述最低要写清:改了哪些文件、解决什么问题、影响哪些平台或接口、跑了哪些验证、有无未覆盖风险。
- 版本号同时维护:根 `Cargo.toml` 的 `[workspace.package].version`、`apps/src-tauri/Cargo.toml`、`apps/src-tauri/tauri.conf.json` 三处必须一致;发版用 `scripts/bump-version.ps1` 统一入口。

## 文档索引

- `README.md` — 项目介绍与专题文档索引(运行部署、FAQ、接口总表等在 `docs/zh-CN/report/`)
- `docs/zh-CN/ARCHITECTURE.md` — 完整目录职责与运行关系(英/俄/韩有对应翻译)
- `docs/zh-CN/TESTING.md` — 分场景的验证清单
- `CONTRIBUTING.md` — 提交边界、大文件阈值、禁止项
- `crates/service/src/gateway/README.md` — gateway 子域职责与路由策略语义
- `apps/AGENTS.md` — 前端工程规范(玻璃拟态、shadcn 约束、IPC transport 规则)
