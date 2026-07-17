# 实施计划：聚合 API 冷却状态与手动重置

## 测试先行

1. 为 aggregate API cooldown 写失败测试：快照在阈值前/后正确反映失败数与倒计时；手动 reset 清理失败状态及 policy action。
2. 为 aggregate API RPC dispatch 写失败测试：runtime status list 返回结构，reset 仅接受存在的 API。

## 服务端实现

3. 给冷却模块增加公开的内部 snapshot 和单个 state clear 语义；将 policy action 的单目标清理封装在 routing 模块。
4. 在 gateway facade 导出 snapshot/reset 能力。
5. 在 aggregate API service/RPC dispatch 注册 list/reset handler，复用存在性校验与 service 访问控制。
6. 同步 Tauri command registry、service-mode Web RPC mapping（如该命令链需要）和 RPC method dispatch。

## 前端实现

7. 增加 runtime status 类型、typed account-client 方法与 Web command 映射。
8. 在聚合 API 页面单独查询 runtime status，按 id 合并，局部倒计时刷新。
9. 收窄第一列，增加“路由状态”列；将现有“状态”标题改为“启用”。
10. 为冷却行接入 Tooltip 和 ConfirmDialog/mutation/toast；确认后使 runtime status 刷新。

## 验证

11. 先运行新增的 Rust 定向测试，再运行相关 service package 测试。
12. 运行 `pnpm -C apps run build`；若 runtime transport 触及对应脚本，再运行 `pnpm -C apps run test:runtime`。
13. 审查 diff，确认没有暴露 secret、没有写入 SQLite、没有影响未冷却 API 的路由。
