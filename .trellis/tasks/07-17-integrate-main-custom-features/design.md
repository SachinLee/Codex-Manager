# Design: main integration with custom feature preservation

## Integration strategy

Create a merge commit on `codex/integrate-main-20260717` with `main` as the
architectural baseline for conflicts. Preserve branch-only modules where their
contracts remain valid, and port behavior rather than selecting entire legacy
files in conflicts.

The integration is deliberately a merge, not a rebase: the source branch has
already merged main repeatedly and contains large combined feature commits.

## Architectural decisions

### Model catalog and billing

- Model catalog v2 remains the sole model/routing/pricing runtime.
- Do not resurrect `apikey_models`, remote synchronization, or legacy compiled
  `PRICE_SEEDS` as a runtime fallback.
- Add v2 extensions after main migration 116. Existing custom migration IDs are
  not renamed or deleted; bridge logic detects legacy custom data and ports it
  into v2 structures without overwriting user-edited values.
- Represent long-context boundaries through v2 tier minima: `272001` represents
  strict `>272000`, and `200000` represents inclusive `>=200000`.
- Store cache-write prices, billing mode, provider reported cost, local estimate,
  variance, and multiplier result in typed v2 structures/snapshots; use integer
  micro-USD for all final charged amounts.

### Gateway and recovery

- Main's candidate-specific request isolation is the request construction
  boundary.
- Capability routing stays in focused modules, but its integration is adapted to
  main's candidate executor and transport pipeline.
- A typed failure disposition controls health effects, same-candidate retry,
  failover, observation writing, and cooldown. Capability failures do not poison
  supplier health.
- Every retry derives from the immutable normalized original body. Delivery start
  is a hard retry boundary.

### Logs, RPC, and UI

- Main's request-log visibility and clear semantics are retained.
- Custom session/conversation fields, model aggregation, pricing audit, and
  attempt events are additive and use existing filter/authorization boundaries.
- Main's model catalog and page structure is retained; branch-specific focused
  components are mounted through typed API wrappers and the existing transport
  chain.
- Tauri registry, service RPC dispatch, Web command mappings, TypeScript types,
  normalization, and locale dictionaries change as one contract.

### Post-merge correctness remediation

- Historical request-token reads use a single raw-plus-complete-hourly query
  shape. The raw and hourly key predicates are generated and bound separately
  in SQL order so small `IN` filters and large temporary-table filters behave
  identically.
- Member request-log list-with-summary uses the same owned-key scope for both
  the page and summary projections. Model aggregation projects an explicitly
  named normalized model to avoid ambiguous joined columns.
- HTTP and WebSocket response usage share the same cache-write field semantics.
  Dashboard range validation is applied at the service boundary before storage
  reads.
- Frontend source assertions normalize line endings; locale coverage is fixed
  in message catalogs rather than weakening the checks.

## Conflict resolution order

1. Core migration/storage and model catalog v2.
2. Service billing, request logging, and authorization/RPC contracts.
3. Gateway candidate pipeline, recovery, cooldown, and capability modules.
4. Tauri command registry/startup integration.
5. Frontend typed clients, hooks, pages/components, i18n, and tests.
6. Documentation and Trellis metadata.

## Rollback

All work occurs on the dedicated integration branch. Before any merge commit,
`git merge --abort` returns it to the custom-feature baseline. Additive database
migrations are not destructively rolled back; operational fallback remains the
existing capability-routing `off` mode and code-level reversion.
