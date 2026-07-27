# 移除赞助与推荐菜单

## Goal

Remove the "赞助与推荐" menu entry from the CodexManager application shell so users cannot access it through the primary navigation.

## Confirmed Facts

- The entry is the `/author` top-level route in `apps/src/lib/app-shell/top-level-routes.ts`.
- Its sidebar icon mapping is in `apps/src/components/layout/sidebar.tsx`.
- The page is registered in the shell keep-alive map and root page path list.
- Existing author-page coverage targets the standalone `/author/` page, while `apps/tests/top-level-routes.test.mjs` verifies the top-level route set.

## Requirements

- Remove `/author` from the application shell's top-level route configuration.
- Remove its sidebar icon mapping and keep-alive page registration.
- Remove `/author` from the root page path allowlist.
- Update affected route assertions so the top-level route contract remains accurate.
- Keep the author page, its assets, runtime capability, and author-content data behavior unchanged.

## Acceptance Criteria

- [ ] The sidebar contains no "赞助与推荐" menu item for any role.
- [ ] `/author` is absent from the top-level route configuration and shell page registrations.
- [ ] Existing standalone author-page behavior remains available and is not modified by this task.
- [ ] `pnpm -C apps run test:navigation` passes.
- [ ] `pnpm -C apps run build` passes.

## Out Of Scope

- Removing the `/author` route, page implementation, assets, translations, runtime capability, or remote content endpoint.
- Changing unrelated recommendations, including gateway concurrency guidance.
