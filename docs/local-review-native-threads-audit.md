# Native local review threads: implementation audit

| Requirement | Implementation evidence | Status |
| --- | --- | --- |
| Load current A/B session | `compare_range` schedules `LoadLocalReviewSession`; `RepoState.local_review` rejects stale session replies | Implemented for commit↔commit shelf A/B |
| No render-path sidecar I/O | Load/add/status operations run on `session_persist_executor`; rows read the cached session only | Implemented |
| Inline markers/count | Ordinary inline rows count old/new anchors; split columns count only their own side and render `💬 N` | Implemented |
| Open/resolved thread list | Diff-line menu opens `LocalReviewThreads`, ordered open first with status shown | Implemented |
| Resolve/reopen | Status mutation uses the shared writer lock, re-reads under lock, persists atomically, then reloads | Implemented |
| Own-write refresh | Successful add and status mutations schedule a session reload | Implemented |
| External CLI/agent writes | Review list has explicit Reload; reads rely on atomic rename and do not take the writer lock | Implemented without idle polling |
| Worktree/submodule storage | Common-dir resolver handles `.git` directory, linked-worktree `commondir`, and submodule `.git` pointer | Covered by state tests |

Deliberate follow-ups: dirty-worktree review endpoints, context-hash re-anchoring,
clicking a marker to filter to that exact line, and marker overlays in the
collapsed/context-expanded projection. These do not block local commit A/B
review or CLI-agent consumption of the same schema-v1 sidecar.
