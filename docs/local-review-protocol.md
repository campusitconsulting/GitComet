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

1. Resolve the repository common directory with
   `git rev-parse --path-format=absolute --git-common-dir`.
2. Read `gitcomet/reviews-v1.json`; a missing file means an empty store.
3. Select the requested session and process only comments whose status is
   `open`.
4. Apply code changes in the relevant worktree.
5. Update comment status/body/timestamp through Git Browser's CRUD API (the CLI
   surface is the next integration layer), preserving stable ids.
6. Re-read `revision` before writing if the agent edits the JSON directly. The
   application writes through a temporary file and atomic rename, so readers
   never observe partial JSON; direct writers must do the same.

Agents should not rewrite unknown fields or downgrade `schema_version`.
