# Comparing linked worktree states

GitComet can compare the complete local state of two linked worktrees without
checking out either branch and without changing either worktree's real index.

## Usage

Choose either endpoint in the A/B comparison shelf. Dirty linked worktrees are
listed under **Live worktree states**, separately from their immutable entries
under **Worktree HEADs**. Choose two states and press **Open diff**. Commit ↔
worktree and worktree ↔ worktree combinations use the same flow. Endpoint
selection stays in the searchable shelf so worktree context menus remain
aligned with upstream GitComet.

The picker wording is intentional:

- **Worktree HEAD** means the immutable commit currently checked out there.
- **Working state** means its tracked files plus staged, unstaged, and
  non-ignored untracked files at capture time. Ignored files are excluded.

## Snapshot semantics

Pressing **Open diff** captures every live endpoint into a Git tree. Each
capture uses a private temporary index seeded from that worktree's HEAD, then
applies `git add -A` and `git write-tree`. It never writes the worktree's real
index and never modifies or checks out files. Writing the tree necessarily adds
otherwise unreachable blob/tree objects to the repository object database.

Both resulting tree ids are immutable, so the displayed file list and every
per-file diff remain internally consistent even if an agent continues editing
one of the worktrees afterward. Reopening the comparison captures a new pair of
trees. A late snapshot result is discarded if A or B changed while it was
running.

The filesystem cannot be locked across the entire `git add` traversal. If an
agent writes a file during capture, the tree represents whatever Git observed
during that traversal, just like a concurrent `git status`/`git diff` scan.

## Persistence

Named commit/ref pairs persist per repository. Pairs containing a live
worktree endpoint are deliberately session-only and the **Save pair** action is
disabled for them. Snapshot trees have no permanent ref and may be pruned by
Git garbage collection; silently restoring such a pair after restart would
therefore be unreliable. A future durable-snapshot feature can opt into hidden
refs explicitly, rather than changing repository refs as a side effect of a
local review.
