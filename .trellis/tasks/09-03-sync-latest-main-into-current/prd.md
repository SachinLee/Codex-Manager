# 同步最新main并合并当前分支

## Goal

将远端 `origin/main` 更新到可见的最新提交，并把它合并进当前分支 `codex/integrate-main-20260717`，同时保留当前分支已有提交、未提交修改和未跟踪工作，不因同步操作覆盖当前分支新增功能。

## Background
- 当前分支为 `codex/integrate-main-20260717`，合并前 `HEAD` 为 `df17b58a291a52345bcc7a0918a0b73399d72656`。
- 合并前工作区基线为 24 个未暂存修改文件和 16 个未跟踪文件；合并后通过 `stash@{0}` 恢复，当前工作区显示 26 个未暂存修改文件和 16 个未跟踪文件，其中增加了本任务规划/证据文件。
- 本次 `git fetch origin main` 将 `origin/main` 从 `c4b463606ef0cee266be2a0aa00fe96e1ecf967f` 更新到 `a4805dd4b5b6a64312c305c9a35554b7f334102c`。

## Requirements

1. 只在当前分支上操作；不重置、覆盖、删除或强制改写当前分支已有提交。
2. 在合并前安全保存当前工作区的已暂存、未暂存及未跟踪内容，合并完成后恢复。
3. 从 `origin` 获取最新 `main`，以获取后的 `origin/main` 作为合并源，不使用可能过期的本地 `main`。
4. 将 `origin/main` 合并到当前分支；出现冲突时逐项处理，保留当前分支新增功能，并纳入 `main` 的独立变更。
5. 不提交当前分支原有的业务修改；只允许产生用于记录主分支同步的合并提交（若Git实际需要）。
6. 合并结束后确认工作区中原有修改和未跟踪内容仍在，且当前分支包含获取到的 `origin/main` 提交。

## Acceptance Criteria

- [x] `origin/main` 已在本次操作中成功获取并指向远端最新可用提交 `a4805dd4b5b6a64312c305c9a35554b7f334102c`。
- [x] 当前分支仍为 `codex/integrate-main-20260717`，合并提交 `7f57e2b48fcb334c601b4ad6b2c7ecf75a4dd19b` 的父提交包含 `origin/main`，且 `git merge-base --is-ancestor origin/main HEAD` 返回成功。
- [x] 合并过程已完成且 `git diff --name-only --diff-filter=U` 无输出；`git grep` 未发现受跟踪源文件中的冲突标记。
- [x] 合并前工作区通过 `git stash push --include-untracked` 保存，并使用 `git stash apply --index stash@{0}` 恢复；原有修改和未跟踪内容仍在，且 stash 保留以便回滚。
- [ ] 完成产品构建验证；`cargo check -p codexmanager-core` 通过，`pnpm -C apps run build` 仍因当前合并后的既有 settings/user-agent 接口不一致失败，`cargo check -p codexmanager-service` 仍有跨模块合并编译错误，详见 outcome.md。

## Out of Scope

- 不修改或清理当前分支业务代码，不重构、不格式化、不升级依赖。
- 不推送远端、不创建Pull Request、不归档任务、不替用户提交业务改动。
- 不处理与本次合并无关的其他工作区变化。

## Risks and Handling

- 工作区变化量较大且跨多个包，保存/恢复或合并时可能产生冲突；优先使用可回滚的临时保存机制，冲突时人工按文件保留双方有效内容。
- `.omp/.runtime/` 与既有 Trellis 任务目录属于当前未跟踪内容；保存范围需包含未跟踪文件，恢复后不得因忽略或清理而丢失。

## Open Questions

无。采用 `origin/main` 作为远端主分支，采用先保存工作区、获取远端、合并、恢复工作区的保守流程。
