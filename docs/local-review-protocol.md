# Local review protocol v1

Git Browser reviews are local Git data, not GitHub pull requests. The shared
file for all linked worktrees is:

```text
<git-common-dir>/gitcomet/reviews-v1.json
```

Using the common Git directory is intentional: comments stay out of the working
tree, do not become accidental commits, and remain visible from every worktree
and every local agent attached to the repository. Removing the repository also
removes its local review data. Export/import can be added independently when a
review needs to travel to another machine.

## Schema

The root object contains `schema_version`, a monotonically changing `revision`,
and `sessions`. A session pins two endpoints and owns its comments:

```json
{
  "schema_version": 1,
  "revision": 3,
  "sessions": [
    {
      "id": "review-payroll-lock",
      "title": "Payroll advisory lock",
      "base": { "kind": "commit", "oid": "4aad377c..." },
      "head": { "kind": "commit", "oid": "2b3d30b8..." },
      "status": "open",
      "created_at_unix_ms": 1787677200000,
      "updated_at_unix_ms": 1787677300000,
      "comments": [
        {
          "id": "comment-01",
          "anchor": {
            "path": "apps/api/payroll.py",
            "side": "new",
            "old_line": null,
            "new_line": 42,
            "context_hash": "sha256:..."
          },
          "author": { "name": "Codex", "kind": "codex" },
          "body": "This retry path can enqueue the payment twice.",
          "status": "open",
          "created_at_unix_ms": 1787677250000,
          "updated_at_unix_ms": 1787677250000
        }
      ]
    }
  ]
}
```

An endpoint is either an immutable commit or a worktree snapshot reference:

```json
{ "kind": "commit", "oid": "..." }
{ "kind": "worktree", "path": "/abs/worktree", "head": "..." }
```

Line numbers make the common case cheap. `context_hash` lets a consumer
re-anchor a comment if the diff moves after an agent edit. A missing `side`
makes the anchor file-level.

## Agent workflow

The supported integration surface is the `gitcomet review` JSON CLI. It finds
the same store from the main checkout, a linked worktree, or a directory inside
either one:

```sh
# Read the store and its current revision.
gitcomet review --repo /path/to/worktree list

# Bare revisions and commit:<revision> are resolved to immutable full OIDs.
# worktree:<path> records the canonical path and its current HEAD.
gitcomet review --repo /path/to/worktree create-session \
  --id review-payroll-lock \
  --title "Payroll advisory lock" \
  --base origin/develop \
  --head worktree:/path/to/agent-worktree

gitcomet review --repo /path/to/worktree show review-payroll-lock

gitcomet review --repo /path/to/worktree --expect-revision 1 add-comment \
  review-payroll-lock \
  --id comment-01 \
  --path apps/api/payroll.py \
  --side new --new-line 42 \
  --context-hash sha256:... \
  --author Codex --author-kind codex \
  --body "This retry path can enqueue the payment twice."

gitcomet review --repo /path/to/worktree --expect-revision 2 resolve-comment \
  review-payroll-lock comment-01
```

Every successful command writes one JSON document to stdout. `list` returns
the root store. `show` returns `{schema_version, revision, session}`; mutation
commands return the changed session or comment together with the resulting
`revision`. Errors go to stderr and use GitComet's error exit code (`2`). IDs
are deliberately caller-supplied so agents can use stable, meaningful values
and retries cannot silently create duplicates.

Mutations acquire `<git-common-dir>/gitcomet/reviews-v1.lock`, then re-read the
store before changing it. The optional `--expect-revision` check runs while
that lock is held. Cooperating agents should pass the revision from their last
read; a mismatch fails without writing and tells the agent to re-read and
retry. A lock older than 30 seconds is treated as abandoned, and lock waits are
bounded to five seconds. The final JSON is still written by temporary file,
`fsync`, and atomic rename.

Recommended workflow:

1. Run `gitcomet review --repo <worktree> list`.
2. Select the requested session and process only comments whose status is
   `open`.
3. Apply code changes in the relevant worktree.
4. Resolve handled comments with `--expect-revision <last-read-revision>`.
5. If the revision changed, re-read and reconcile before retrying.

Direct JSON editing is unsupported. A non-CLI consumer must implement the same
locking, revision check, temporary-file write, `fsync`, and atomic rename
protocol or it can overwrite another agent's update.

Agents should not rewrite unknown fields or downgrade `schema_version`.
