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
- The history/review boundary is a true drag handle. It uses minimum pixel
  heights rather than a rigid percentage band, persists its last position and
  restores keyboard focus after the drag.
- GitComet's per-commit, selected-lane and selected-branch highlighting stays;
  SourceTree is only the reference for stronger stroke/node visual weight.
- Persisted ratios are defaults, not locks. Dragging uses minimum pixel sizes
  and collapse instead of a permanent percentage range.

### Measured SourceTree graph geometry

The graph rhythm is taken from the original 144dpi, 2x-Retina SourceTree
reference screenshot rather than estimated by eye:

- 11pt lane-centre pitch (22 physical pixels);
- 2pt lane stroke (4 physical pixels);
- 7pt ordinary commit dot (14 physical pixels);
- 20pt commit-row pitch (40 physical pixels);
- 11pt graph inset and approximately 5pt rounded elbows.

GitComet's semantic colours, selected-lane wash, merge/stash icons, branch
selection and worktree nodes remain layered on top of this geometry.

The base history profile is selectable under
`Settings > Git Log > History appearance profile`. `SourceTree` applies the
compact measurements above; `GitComet` restores the roomier original rhythm.
The profile owns spacing and graph geometry only. Theme colours, fonts,
highlight strength and special commit symbols remain independent settings.

Graph emphasis is independently configurable under `Settings > Git Log`:

- highlight strength: selected only 0% (no dimming or greying of other lanes),
  minimal 10%, subtle 20%, balanced 35% (default), or strong 55%; the same value
  controls lane, node, message-border and summary-text wash so a row never
  renders at contradictory strengths. Previously saved custom values remain
  valid but are not promoted as presets. Turning lane highlighting off remains
  a separate option;
- special commit nodes: plain dots, small 7pt discs with 4.5pt glyphs (default),
  or the original detailed 16pt GitComet symbols. Pictograms appear only on
  merge and stash commits; ordinary commits remain dots. The small symbols leave
  clear space inside the measured 11pt lane pitch.

The UI family remains selectable under `Settings > General > UI Font`, and
overall text/control sizing remains independently selectable under
`Settings > General > UI scale`. `System Default` resolves to the native macOS
UI family used by SourceTree. Thus SourceTree graph geometry, font family,
font/control scale, highlight strength and special-node density can be combined
without one preset overwriting the others.

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

### Reported `8c2c21cc` topology

The commit was located in `/Users/aatamano/Development/ERP`. It is the second
parent of merge `5cba589c`; its own parent is `d943dd9c`. A regression fixture
now keeps that lane continuous while the date-ordered commits from an unrelated
branch pass beside it.

The current production computation already passes this fixture, including with
the real local and remote branch heads, and the list-to-graph row mapping also
remains aligned when worktree rows are inserted. Consequently this patch does
not claim a production fix: reproducing the screenshot discrepancy still needs
the exact GitComet history scope, ordering/loading state and a screenshot that
shows the affected SHA.

### Existing worktree and submodule integration

The fork does not need a second Git model for these objects:

- every local branch that owns a listed worktree already receives an
  interactive worktree badge in the branch tree;
- the separate `Worktrees` section can be expanded for path-oriented browsing
  or collapsed for a branch-oriented combined tree;
- selecting a worktree reveals its checked-out commit and local-change row in
  the shared history graph; double-click opens it as a repository tab;
- submodules live in the same project sidebar, expose recorded/checked-out
  revisions and status, and their changed pointer can be reviewed through the
  existing inline submodule diff pipeline.

The missing product controls are therefore small: an explicit switch for
worktree badges versus the separate list, and honest date metadata. Git does
not store a canonical branch creation date; the UI must label an earliest
available reflog time as estimated and keep the branch tip-commit date distinct.
Worktree creation can use filesystem birth time where supported, with a clearly
labelled fallback rather than presenting an mtime as exact creation time.

## Next patches

1. Placement-aware file-tree/diff resize and replacement of the graph column's
   enlarged safety bound with a maximum derived from the viewport.
2. A/B shelf UI over the comparison reducer, including named reusable pairs.
3. Commit search/reveal and direct commit/range-diff gestures.
4. Local review comment store and AI-agent exchange format.

The v1 local JSON model and atomic storage contract are documented in
[`local-review-protocol.md`](local-review-protocol.md). The remaining work is UI,
reducer/effect wiring and a small CLI facade; the schema is already independent
of any hosting provider.
