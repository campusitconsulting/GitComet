# Commit history ordering

GitComet exposes two independent history choices in the history header menu:

- **Date Order (Fast)** is the default. It keeps the existing lightweight gix
  commit-time walk, paged and resumable. This is the right mode for normal work
  in deep or many-ref repositories.
- **Ancestor Order (Topo)** uses gix's topology-aware `TopoOrder` walk. No
  reachable parent is shown before one of its children, including histories
  with skewed commit clocks. This is closer to SourceTree's Ancestor Order and
  usually produces clearer, less interleaved lanes.

The choice is saved per repository. It is part of both the in-flight request
identity and backend page/walk cache keys, so switching order cancels the old
request and an opaque resume token can never continue a differently ordered
walk. First-parent history stays on the lightweight walker because a single
parent chain is already intrinsically topological.

## Performance and edge cases

Date remains the default deliberately. On the ERP repository with a 2,000-row
sample on 2026-08-25, Date measured about **0.319 s** and Ancestor about
**1.399 s** on the same machine (roughly 4.4x slower). This is a local
measurement, not a universal benchmark, but it accurately describes the
tradeoff: topo ordering builds and retains graph state before yielding rows.

gix's topo builder currently computes indegrees eagerly. Cancellation is
checked immediately before and after that build and during subsequent walking,
but the build itself has no interrupt callback. A superseded result is still
rejected by its request sequence.

In a shallow clone, recorded parents beyond the shallow boundary are absent by
design and gix topo construction cannot follow them. Ancestor therefore falls
back explicitly to the boundary-aware Date walker for that repository. The UI
states this fallback instead of failing the history load.
