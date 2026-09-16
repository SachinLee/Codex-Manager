# 技术设计：安全同步并合并 origin/main

## Context

当前分支包含尚未提交的产品修改、文档修改和未跟踪任务/临时文件。目标是把本次 `git fetch origin main` 得到的远端 `origin/main` 集成到当前分支，同时保证工作区内容和当前分支历史不被覆盖。

## Proposed solution

采用可回滚的工作区保护流程：

1. 在当前分支记录基线分支名、HEAD、`origin/main`、工作区状态及所有未跟踪路径。
2. 用 Git 的临时保存机制保存未暂存、已暂存和未跟踪内容；不使用 reset、clean 或强制改写历史。
3. 执行 `git fetch origin main`，以 fetch 后的 `origin/main` 作为唯一合并源。
4. 在当前分支执行普通 `git merge origin/main`。若发生冲突，逐项比较当前分支与 main 的有效变更，优先保留当前分支业务功能并纳入 main 的独立变更；不得使用无差别 `ours`/`theirs` 覆盖。
5. 恢复临时保存的工作区内容；若恢复时发生冲突，仅处理保存内容与 main 合并结果之间的叠加冲突，不提交原有业务修改。
6. 核验分支、合并祖先关系、冲突状态、原始修改路径和原始未跟踪路径均仍存在，并记录证据。

## Invariants

- 当前分支名称不变。
- 当前分支已有提交不被重置、删除或改写。
- 原有工作区内容不因同步操作静默丢失。
- 合并源必须是本次 fetch 后的 `origin/main`。
- 除 Git 必要的合并提交外，不创建原有业务修改提交。

## Failure modes and rollback

- fetch 失败：不进行合并，保留已保存工作区并按原状态恢复。
- merge 冲突：保持冲突现场，不继续恢复工作区，先完成逐文件解决并确认 `git diff --check` 与冲突标记清零。
- 工作区恢复冲突：保留合并结果与保存记录，逐文件解决；禁止用 reset/clean 丢弃任一侧内容。
- 任何无法安全判定的覆盖风险：停止自动操作，记录路径和状态，等待人工决策。

## Compatibility

仅修改 Git 历史和工作区状态，不改变产品代码、配置、依赖或远端状态。若 `origin/main` 已是当前 HEAD 的祖先，merge 可能产生无变更结果；仍需完成 fetch 和祖先关系核验。

## Verification

核验 `git status --short --untracked-files=all`、`git diff --name-status`、未跟踪路径清单、`git merge-base --is-ancestor <fetched-origin-main> HEAD`、`git ls-files -u` 和当前分支名。以操作前记录的路径集合与操作后集合进行逐项比较。
