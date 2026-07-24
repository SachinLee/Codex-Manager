# Skills Repository UI Design QA

## Evidence

- Source visual truth:
  - `/tmp/codex-clipboard-N72mW9.png` — cc-switch repository catalog.
  - `/tmp/codex-clipboard-0Yn2wn.png` — cc-switch repository management.
  - `/tmp/codex-clipboard-6YiNS8.png` — user-reported wide-screen filter collision.
- Browser-rendered implementation:
  - `/tmp/codex-skills-layout-qa.ac7m5E/skills-repository-catalog.png`.
  - `/tmp/codex-design-qa/skills-repository-management-final3.png`.
- Viewport: 2048 × 1189 for final visual captures; the same flow and filter geometry assertions also passed at the tighter 1280 × 800 desktop viewport (and the earlier 1440 × 900 pass).
- State: Simplified Chinese, `tech` theme with classic appearance, connected service, Skills installation selected, repository catalog populated with the four built-in repositories. The management capture shows the repository dialog open.
- Full-view comparison evidence:
  - `/tmp/codex-design-qa/comparison-final-catalog-full.png`.
  - `/tmp/codex-design-qa/comparison-final-management-full.png`.
  - `/tmp/codex-skills-layout-qa.ac7m5E/comparison-full.png` — reported layout beside the corrected browser render.
- Focused comparison evidence:
  - `/tmp/codex-design-qa/comparison-final-catalog-focus.png` — search, filters, grid density, cards, and actions.
  - `/tmp/codex-design-qa/comparison-final-management-focus.png` — add-repository form and synchronized repository rows.
  - `/tmp/codex-skills-layout-qa.ac7m5E/comparison-filter-focus.png` — before/after filter geometry at wide desktop size.

## Findings

No actionable P0, P1, or P2 differences remain.

- Fonts and typography: both targets use a compact system sans-serif hierarchy. CodexManager keeps its existing Segoe UI/PingFang SC stack, weights, monospaced paths, truncation, and line heights; all labels remain readable without collision.
- Spacing and layout rhythm: the final wide layout uses three catalog columns, consistent card heights, and a wider repository dialog. The filter toolbar now uses explicit `minmax(0,1fr) / 14rem / 10rem / auto` tracks, so the search field retains the available width while repository, status, and refresh controls remain separate. The 1440 × 900 run correctly falls back to the denser two-column catalog layout.
- Colors and visual tokens: the implementation intentionally retains CodexManager's blue primary and glass-console tokens instead of copying cc-switch's green accent. Contrast, selected tabs, status badges, and destructive actions remain semantically clear.
- Image and icon fidelity: the reference contains no required raster product imagery. All visible actions use the repository's Lucide icon system; no placeholder imagery, CSS drawings, emoji, or hand-authored SVG substitutes were introduced.
- Copy and content: the menu is “Skills 与插件”, the outer tabs distinguish “Skills 安装” from “Codex 插件安装”, and repository/status filters now render localized labels rather than raw `all` values. The four built-in repositories match the source behavior.
- Interaction and accessibility: tabs, search, repository dialog open/close, built-in delete protection, plugin scrolling, install confirmation, and long error-toast scrolling were exercised. Controls have role/name coverage in Playwright, and the final run recorded no page or console errors.
- Intentional adaptation: repository management is a modal instead of a dedicated page, and the CodexManager shell remains visible. This follows the existing app navigation and dialog system while preserving the source workflow and information architecture.

## Comparison History

1. Initial capture: `/tmp/codex-design-qa/skills-repository-catalog.png` showed the workspace stuck at the fade animation's zero-opacity state in the static-export browser. Removed the redundant PageWorkspace entrance animation; the post-fix catalog is fully opaque in `skills-repository-catalog-final3.png`.
2. First post-opacity pass: filters exposed raw `all` values, the wide catalog stayed at two columns, and repository management was cramped. Added localized selected-value rendering, a three-column 2XL grid, four representative built-in repository rows, and a responsive 980px dialog.
3. First final comparison: the original full and focused comparison images showed no remaining P0/P1/P2 mismatch in the mocked catalog.
4. User wide-screen report: both Select triggers retained `w-full` inside a flex row, consumed half of the toolbar each, and collapsed the flexible search field to zero width. Replaced the row with explicit responsive grid tracks and added browser geometry assertions for search width, fixed filter widths, and non-overlap. The latest full and focused comparison images show the search field restored with all four controls aligned; the Playwright flow passed with zero page or console errors.

## Implementation Checklist

- [x] Preserve the existing CodexManager design system and navigation shell.
- [x] Match the repository / skills.sh discovery structure and searchable card catalog.
- [x] Show all four built-in repositories with sync state and refresh controls.
- [x] Separate standalone Skills installation from full Codex plugin installation.
- [x] Keep search, repository filter, status filter, and refresh controls in independent non-overlapping grid tracks.
- [x] Verify primary interactions and browser console state.

## Follow-up Polish

- P3: a future iteration may add optional compact/list density controls for repositories containing hundreds of Skills. This is not required for the current workflow.

final result: passed
