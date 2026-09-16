# 执行计划：同步最新 main 并合并当前分支

### Slice 1: AC-001 - 建立工作区保护基线

- Behavior: 在任何改变分支历史前，完整记录当前分支、HEAD、工作区修改和未跟踪路径。
- Code boundary: Git 工作区与 `.trellis/tasks/09-03-sync-latest-main-into-current/` 证据文件。
- Test seam: 操作前后状态清单的集合比较。
- Implementation: 保存 tracked staged/unstaged 与 untracked 内容；不包含无关的产品提交。
- Validation: `git status --short --untracked-files=all`、保存物状态和路径清单。
- Rollback: 若保存失败，停止，不 fetch/merge。

### Slice 2: AC-002 - 获取并合并 origin/main

- Behavior: 当前分支历史包含本次 fetch 得到的 `origin/main`，无未解决冲突。
- Code boundary: 当前 Git 分支；禁止切换到或修改本地 `main`。
- Implementation: 执行 `git fetch origin main`，记录 fetch 后 SHA；随后普通 `git merge origin/main`。冲突时逐项手工融合，禁止无差别覆盖策略。
- Validation: 当前分支名、fetch 后 SHA、merge 结果、`git ls-files -u`、祖先关系和合并状态。
- Dependencies: Slice 1 完成。
- Rollback: 仅在无法安全解决时保留现场并记录；不得 reset/clean 覆盖工作区。

### Slice 3: AC-003 - 恢复并核验原有工作区

- Behavior: 合并前所有已修改路径和未跟踪路径在合并后仍可见，且原有业务修改未被提交。
- Code boundary: Git 临时保存内容与工作区恢复结果。
- Implementation: 恢复保存内容；仅处理恢复叠加冲突，保留双方有效变更。
- Validation: 对比操作前后的 tracked diff、untracked 路径集合、暂存状态和冲突状态；运行 `git diff --check`。
- Dependencies: Slice 2 完成且无 merge 冲突。
- Rollback: 恢复冲突时保留保存物和现场，禁止清理或强制覆盖。

### Slice 4: AC-004 - 记录证据与完成任务

- Behavior: 任务记录包含实际 fetch、merge、恢复和验证结果；未验证内容明确标注风险。
- Code boundary: task outcome/evidence 文件；不修改产品代码。
- Implementation: 写入命令结果、SHA、路径集合比较、冲突检查和剩余风险。
- Validation: 复核工作区仍处于用户可继续工作的状态；不执行 push、PR、archive 或业务提交。
- Dependencies: Slice 3 完成。
- Rollback: 证据记录错误时修正记录，不回滚已验证的 Git 集成。
