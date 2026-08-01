# 调整聚合 API 用量折叠与排序

## Goal

Improve the scanability of the Aggregate API management page by collapsing the
per-model daily usage table by default and showing Aggregate API routing order.

## Confirmed Facts

- `apps/src/app/aggregate-api/page.tsx` renders the "今日模型用量" table open at
  all times.
- The typed Aggregate API contract already exposes `AggregateApi.sort`, and the
  create/edit modal persists it.
- The upstream connection table presently renders `filteredApis` in the API
  response order.

## Requirements

- Add an explicit expand/collapse control to "今日模型用量". It is collapsed on
  the first render and may be toggled without refetching data.
- Add a visible sort-order column to the Aggregate API list.
- Sort the Aggregate API list in ascending numeric `sort` order before it is
  filtered and rendered. Use a stable deterministic secondary ordering for
  equal sort values.
- Keep the existing provider filter, usage summary, and API mutations working.

## Acceptance Criteria

- [x] On initial page load, the "今日模型用量" table body is hidden while its
  title and model count remain visible.
- [x] Activating the toggle shows the existing loading, empty, and model usage
  table states; activating it again hides them.
- [x] The API table includes a "排序" column displaying each API's `sort` value.
- [x] With multiple APIs, rows are displayed in ascending `sort` order both
  before and after provider filtering; equal values render consistently.

## Out of Scope

- Changing backend storage, routing behavior, or the Aggregate API edit form.
- Persisting the expanded/collapsed state across page reloads.
- Adding interactive sort-order editing directly in the list.

## Notes

- This is a frontend-only, lightweight task. Reuse existing Button, Card, Table,
  Tooltip, i18n, and Lucide patterns.
