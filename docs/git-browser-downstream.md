# Git Browser downstream roadmap

This fork keeps GitComet's Git engine, virtualized commit DAG, diff renderer,
worktree/submodule support, monitoring and comparison reducer. Its downstream
product layer is a fast local-review workspace for many concurrent agent
worktrees.

## Product target

- SourceTree-like history/DAG stays visible while reviewing a diff.
- Existing changed-file Details pane sits beside the existing Diff pane.
- Any commit/ref/worktree endpoint can be pinned as A or B without modifier-key
  multi-selection or scrolling both rows into view.
- Named local diff sessions retain endpoints, selected file/hunk and review
  state; later, local comments are exposed to AI agents through a small
  CLI/IPC protocol.
- Worktrees and submodules remain one project context and can also be scoped
  individually.
- Refresh preserves focus, selection, A/B and semantic scroll state; Git work
  stays cancellable and bounded rather than starting a process storm.

## Upstream boundary

Do not duplicate the existing `RangeSelection`, `DiffTarget::CommitRange`,
range-file loading, worktree dirty scans, submodule diff paths or theme engine.
Generic fixes such as graph topology, commit search and double-click-to-open
remain candidates for small upstream patches. The SourceTree composition,
named diff-session shelf and agent review protocol remain downstream until an
upstream extension point is agreed.

## First composition checkpoint (2026-08-25)

- `WorkspaceLayoutPreset`: `Classic`, `SourceTreeReview`, `WideReview`.
- Persisted, clamped `review_split_percent` (20–80%, default 56%).
- One `HistoryView` entity reused by the root layout; no duplicate renderer,
  pagination, subscription or cache.
- `MainPanePresentation::DiffOnly` reserves the lower-right surface for the
  existing diff while keeping upstream `LegacyAuto` intact.
- SourceTree Review is the downstream default: History above, Details and Diff
  below. Classic remains available as a safe fallback.
- The first boundary is static. The next patch adds its drag interaction and
  persists the already-defined split value.

Validation:

```sh
cargo check -p gitcomet-ui-gpui --features runtime-shaders
cargo test -p gitcomet-ui-gpui --lib \
  --features runtime-shaders \
  full_chrome_layout_only_caches_always_mounted_subviews
cargo test -p gitcomet-state
```

On macOS the runtime-shaders feature avoids requiring a separately downloaded
Xcode Metal Toolchain for development and CI checks.

## Next patches

1. Horizontal history/review drag handle with semantic focus preservation.
2. Layout selector and details-pane placement-aware resize/collapse.
3. Explicit A/B shelf over the existing comparison reducer.
4. Graph-topology regression fixture for the reported `8c2c21cc` case.
5. Commit search/reveal and direct commit/range-diff gestures.
6. Review font size, SourceTree-like theme/density and local comment store.
