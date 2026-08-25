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

The UI family remains selectable under `Settings > General > UI Font`.
`UI font size` is independently selectable from 11–24px, while
`Diff/editor font size` is independently selectable from 9–28px and applies to
unified/split diff, file editor, focused diff and conflict canvases. `UI scale`
(80–200%) now controls geometry, spacing and controls rather than forcing the
text size. `System Default` resolves to the native macOS UI family used by
SourceTree. Graph geometry, font families, both font sizes, control scale,
highlight strength and special-node density can therefore be combined without
one preset overwriting the others.

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

- every local branch that owns a listed worktree can show an interactive
  worktree badge in the branch tree; `Settings > Git log > Show worktrees on
  branch rows` controls this independently and persists across launches;
- the separate `Worktrees` section can be expanded for path-oriented browsing
  or collapsed for a branch-oriented combined tree;
- selecting a worktree reveals its checked-out commit and local-change row in
  the shared history graph; double-click opens it as a repository tab;
- submodules live in the same project sidebar, expose recorded/checked-out
  revisions and status, and their changed pointer can be reviewed through the
  existing inline submodule diff pipeline.

Together, the independent badge toggle and the existing collapsible Worktrees
section provide all three presentations without another Git model: badges with
the section collapsed is the combined view, badges hidden with the section
expanded is the separate view, and badges shown with the section expanded is
the both view. Turning badges off never hides or unloads the standalone section.

Sidebar date metadata is intentionally conservative:

- branch rows show the ref's **tip commit date**, loaded through the existing
  batched `for-each-ref` metadata path. Both the row tooltip and date tooltip say
  that this is not a branch creation date; Git has no canonical one;
- worktree rows show the worktree directory's filesystem birth date when the
  platform/filesystem exposes it. Otherwise the row displays `date ?` and the
  tooltip explains that creation time is unavailable. Modification time is not
  substituted because normal work would make it look like a false creation
  date;
- compact row labels use `YYYY-MM-DD`; tooltips include minutes and explicitly
  label UTC. Metadata is cached with the sidebar/ref load state, so rendering
  rows does not run Git or filesystem queries per frame.

An earliest reflog timestamp could later be offered as a separately labelled
**estimated branch start**, but it must never replace or masquerade as the tip
commit date.

### A/B comparison wiring audit (2026-08-25)

The shelf currently reuses the production `CompareCommitRange` reducer and
`DiffTarget::CommitRange` backend path. Consequently an A/B pair of commits,
local branch tips, remote branch tips, tags, or the checked-out HEADs of two
linked worktrees is a real Git range comparison rather than a UI-only bookmark.
Selecting `Open diff` loads both the changed-file list and the whole-range patch
through the normal cancellable diff pipeline. Selecting a file then narrows the
patch to that path.

Worktree row menus expose both `Set worktree HEAD as comparison A/B` and
`Set worktree working state as comparison A/B` directly.
The saved label contains both the branch and worktree directory name so two
linked checkouts remain distinguishable. An unborn worktree has no commit
endpoint and therefore does not offer the HEAD action. A working-state endpoint
captures staged, unstaged and non-ignored untracked files through a private
temporary index and immutable Git tree; it does not checkout, stash, or mutate
either worktree's real index. Consequently commit↔worktree and two different
dirty worktree↔worktree comparisons are internally stable even while agents
continue editing after the diff opens.

Both A/B chips are direct, searchable endpoint pickers. They combine loaded
local and remote branches, local and remote tags, worktree HEADs, dirty
worktree states and history commits in one sectioned list; full OIDs and commit
summaries are searchable.
Context-menu actions remain as a faster route when the desired object is
already under the pointer.

Named pairs are persisted per repository path in `session.json`. Selecting a
saved pair restores A, B and the active comparison after restart while an
unsaved draft A/B remains intentionally temporary. Pairs containing live
worktree endpoints are session-only because their unreferenced snapshot trees
may be pruned by Git GC. Session writes merge open-repository shelves with
stored closed-repository shelves instead of replacing the whole map.

Submodule compatibility covers the full immutable A/B range. A changed gitlink in an arbitrary
commit range is flagged in `diff_range_files` and its selected range patch shows
the pointer change (`Subproject commit ...`). Selecting that gitlink now loads a
range-aware submodule summary: the selected parent commits are resolved to their
two recorded submodule OIDs and inner file changes/navigation are available when
both nested commits exist locally. Missing objects remain an explicit
`Submodule history is not available locally` state rather than a misleading
empty range.

## Completion checkpoint (2026-08-25)

- SourceTree/Wide/Classic layout presets, vertical review split and directional
  Details↔Diff resize are live and persisted.
- Date Order remains the fast default; the persisted per-repository Ancestor
  Order uses a topology-aware walk for dependency-first graph inspection.
- Refresh preserves selected commit, A/B/named range and the top visible commit
  plus its intra-row pixel offset; if that SHA disappeared after a rewrite the
  existing numeric scroll position is retained.
- Local review has inline/split markers, open/resolved thread list,
  resolve/reopen, explicit external reload and the shared agent-safe CLI.
- Dirty state from two linked worktrees can be captured and compared without
  mutating either checkout or index.

Deliberate follow-ups are narrower: re-anchor review comments by context hash,
support review threads on dirty snapshots, make markers directly filter their
line, and replace the graph column's conservative safety bound with a
viewport-derived maximum.

The v1 local JSON model and atomic storage contract are documented in
[`local-review-protocol.md`](local-review-protocol.md). The CLI facade and first
diff-line comment prompt are implemented; the schema remains independent of any
hosting provider.
