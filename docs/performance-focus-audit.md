# Refresh, focus, and Git-work performance audit

Audit date: 2026-08-25. This note distinguishes behaviour already guarded by
code/tests from the broader downstream product target in
`git-browser-downstream.md`.

## Verified design

| Requirement | Current mechanism | Existing regression/performance coverage |
| --- | --- | --- |
| Do not start duplicate Git work for a burst of filesystem events | `RepoLoadsInFlight` gives each load kind one in-flight lane and one pending replay. A burst is coalesced; completion replays at most one request. | `external_git_state_refresh_is_coalesced_and_replayed_once`, `external_worktree_refresh_replays_coalesced_change_then_settles`, `a_burst_of_worktree_changes_coalesces_into_one_walk_at_a_time`, and the `fs_event` Criterion group. |
| Superseded history walks stop and cannot repaint stale rows | Every log request takes over a log-specific `CancellationToken`; gix checks it during the walk. Replies and streamed chunks carry a monotonically increasing `LogLoadSeq`; superseded results are ignored. | `author_filter_change_starts_its_load_while_a_walk_is_in_flight`, `cancelled_log_reply_is_not_reported_as_an_error`, `superseded_log_chunks_are_ignored`, and `cancelling_repo_loads_clears_the_scan_progress`. |
| Many repositories do not multiply history worker threads without a cap | The shared repository-load executor is capped at 1–2 workers. Filtered commit decoding has a process-wide eight-helper budget. Paged-walk, head-page, and file-follow caches are capped at 32, 32, and 16 entries respectively; pending decode state is one 2,048-commit batch per parked walk. | `repo_load_pool_is_capped_below_primary_pool`, gix log cache/bounded-page tests, `repo_switch/20_repos_all_hot`, `git_ops/log_walk_*`, and `history_cache_build_extreme_scale`. |
| Automatic refresh keeps the file/commit and A/B pair being reviewed | Worktree and Git-state notifications reload the existing `DiffTarget`; same-content diff replies avoid revision churn. A replaced history page reconciles selection by commit ID rather than row index, while A/B and the selected named pair remain untouched. | `external_worktree_change_refreshes_status_and_selected_diff`, `external_git_state_change_refreshes_history_and_selected_diff`, `automatic_git_state_refresh_preserves_both_comparison_slots_and_named_selection`, `diff_loaded_identical_content_skips_rev_bumps_and_keeps_blame`, and history selection reconciliation tests. |
| Switching among many open repositories avoids a redundant full metadata reload | Recently active, hydrated repositories take the primary refresh path. Leaving a repository cancels its old load epoch; stale results are dropped. | `set_active_repo_hot_switch_skips_secondary_refresh_when_metadata_is_ready`, `set_active_repo_reloads_cancelled_history_panes_for_existing_selection`, the `repo_switch` Criterion group, and `frame_timing/repo_switch_during_scroll`. |
| Refresh does not recreate the history renderer or keyboard focus handle | The root retains one `HistoryView` entity and applies state snapshots to it. Window-activation refresh is throttled, and activation coalesces with already-running loads. | `window_activation_dispatch_is_throttled_per_repo`, `repo_monitor_active_repo_activation_coalesces_with_in_flight_refresh`, and focus-preservation tests around diff search and pane resizing. |
| History refresh keeps the same visual commit under the viewport | Before a same-repository/same-scope log revision is replaced, `HistoryView` records the top visible commit ID and its intra-row pixel offset. After the asynchronous cache rebuild it resolves that ID through the new visible-index map and restores the exact offset; if the commit is absent, the existing numeric offset is left untouched. | `semantic_viewport_anchor_follows_its_commit_when_rows_are_inserted_above`, `semantic_viewport_anchor_preserves_partial_row_offset_and_clamps_at_end`, and `missing_semantic_viewport_commit_uses_the_existing_numeric_offset`. |
| Explicit repository reload keeps the active review position | Reload leaves the selected commit, A/B shelf, selected named pair, and active immutable range in place while refreshing Git metadata. A surviving selected commit is retained after the replacement page lands. | `reload_repo_preserves_the_selected_commit_and_ab_review_when_the_commit_survives`. |

## Honest limitations

- Explicit **Reload repository** still clears the back/forward navigation
  stacks because a rebase or amend can make historical snapshots invalid. It
  now preserves the current selected commit and A/B review itself. If an
  endpoint was genuinely deleted, opening/reloading that comparison reports the
  backend resolution error instead of silently moving the review elsewhere.
- The executor has a bounded worker count but uses an unbounded task queue.
  Coalescing and cancellation keep normal refresh bursts small, and cancelled
  queued tasks are skipped before starting, but there is no hard queue-length
  bound. A stress test that rapidly activates many repositories should measure
  queue depth before calling this a formal resource bound.
- The real-repository Criterion harness is opt-in through
  `GITCOMET_PERF_REAL_REPO_ROOT`. Without fresh snapshot fixtures and sidecars,
  synthetic benchmarks prove structural budgets and relative speed only; they
  do not establish an authoritative latency number for the user's ERP trees.

## Focused verification commands

Use these during development instead of the multi-hour full suite:

```sh
cargo test -p gitcomet-state external_git_state_refresh_is_coalesced_and_replayed_once
cargo test -p gitcomet-state external_worktree_refresh_replays_coalesced_change_then_settles
cargo test -p gitcomet-state set_active_repo_reloads_cancelled_history_panes_for_existing_selection
cargo test -p gitcomet-state stale_log_loaded_result_replays_latest_pending_scope_switch
cargo test -p gitcomet-state superseded_log_chunks_are_ignored
cargo test -p gitcomet-state a_burst_of_worktree_changes_coalesces_into_one_walk_at_a_time
cargo test -p gitcomet-state repo_monitor_active_repo_activation_coalesces_with_in_flight_refresh
cargo test -p gitcomet-ui-gpui --features runtime-shaders,benchmarks repo_switch_refocus_same_repo_stays_on_primary_refresh_path
cargo test -p gitcomet-ui-gpui --features runtime-shaders history_horizontal_wheel_does_not_scroll_vertically
cargo bench -p gitcomet-ui-gpui --bench performance -- repo_switch
cargo bench -p gitcomet-ui-gpui --bench performance -- fs_event
cargo bench -p gitcomet-ui-gpui --bench performance -- branch_sidebar
```

For an authoritative release baseline, run `scripts/run-full-perf-suite.sh`
with a fresh-reference stamp and a populated real-repository fixture root on a
stable runner. Do not compare Criterion timings from different runner classes.
