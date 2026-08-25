use gitcomet_core::diff::AnnotatedDiffLine;
use gitcomet_core::domain::DiffLineKind;
use gitcomet_state::local_review::{
    LocalReviewSession, ReviewAnchor, ReviewAuthor, ReviewComment, ReviewEndpoint, ReviewSide,
    ReviewStatus,
};
use gitcomet_state::model::{RepoId, RepoState};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::DiffTextRegion;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LocalReviewCommentDraft {
    pub(super) repo_id: RepoId,
    pub(super) workdir: PathBuf,
    pub(super) session_id: String,
    pub(super) title: String,
    pub(super) base_oid: String,
    pub(super) head_oid: String,
    pub(super) path: PathBuf,
    pub(super) side: ReviewSide,
    pub(super) old_line: Option<u32>,
    pub(super) new_line: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LocalReviewRangeContext {
    repo_id: RepoId,
    workdir: PathBuf,
    session_id: String,
    title: String,
    base_oid: String,
    head_oid: String,
}

pub(super) fn range_context(repo: &RepoState) -> Option<LocalReviewRangeContext> {
    let range = repo.history_state.range_selection.as_ref()?;
    let range_head = range.to.as_ref()?;
    let shelf_a = repo.comparison_shelf.a.as_ref()?;
    let shelf_b = repo.comparison_shelf.b.as_ref()?;
    if shelf_a.commit_id() != Some(&range.from) || shelf_b.commit_id() != Some(range_head) {
        return None;
    }
    let base_oid = shelf_a.commit_id()?.as_ref().to_string();
    let head_oid = shelf_b.commit_id()?.as_ref().to_string();
    Some(LocalReviewRangeContext {
        repo_id: repo.id,
        workdir: repo.spec.workdir.clone(),
        session_id: format!("ab:{base_oid}..{head_oid}"),
        title: format!("{} → {}", shelf_a.label, shelf_b.label),
        base_oid,
        head_oid,
    })
}

pub(super) fn draft_for_diff_line(
    context: &LocalReviewRangeContext,
    path: PathBuf,
    line: &AnnotatedDiffLine,
    region: DiffTextRegion,
) -> Option<LocalReviewCommentDraft> {
    let (side, old_line, new_line) = match line.kind {
        DiffLineKind::Add => (ReviewSide::New, None, line.new_line),
        DiffLineKind::Remove => (ReviewSide::Old, line.old_line, None),
        DiffLineKind::Context if region == DiffTextRegion::SplitLeft => {
            (ReviewSide::Old, line.old_line, line.new_line)
        }
        DiffLineKind::Context => (ReviewSide::New, line.old_line, line.new_line),
        DiffLineKind::Header | DiffLineKind::Hunk => return None,
    };
    if matches!(side, ReviewSide::Old) && old_line.is_none()
        || matches!(side, ReviewSide::New) && new_line.is_none()
    {
        return None;
    }

    Some(LocalReviewCommentDraft {
        repo_id: context.repo_id,
        workdir: context.workdir.clone(),
        session_id: context.session_id.clone(),
        title: context.title.clone(),
        base_oid: context.base_oid.clone(),
        head_oid: context.head_oid.clone(),
        path,
        side,
        old_line,
        new_line,
    })
}

pub(super) fn persistence_payload(
    draft: &LocalReviewCommentDraft,
    body: String,
    comment_id: String,
    now_unix_ms: i64,
) -> (LocalReviewSession, ReviewComment) {
    let session = LocalReviewSession {
        id: draft.session_id.clone(),
        title: draft.title.clone(),
        base: ReviewEndpoint::Commit {
            oid: draft.base_oid.clone(),
        },
        head: ReviewEndpoint::Commit {
            oid: draft.head_oid.clone(),
        },
        status: ReviewStatus::Open,
        created_at_unix_ms: now_unix_ms,
        updated_at_unix_ms: now_unix_ms,
        comments: Vec::new(),
    };
    let comment = ReviewComment {
        id: comment_id,
        anchor: ReviewAnchor {
            path: draft.path.clone(),
            side: Some(draft.side),
            old_line: draft.old_line,
            new_line: draft.new_line,
            context_hash: None,
        },
        author: ReviewAuthor {
            name: "Local reviewer".into(),
            kind: "human".into(),
        },
        body,
        status: ReviewStatus::Open,
        created_at_unix_ms: now_unix_ms,
        updated_at_unix_ms: now_unix_ms,
    };
    (session, comment)
}

pub(super) fn next_comment_id(now_unix_ms: i64) -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("ui-{now_unix_ms}-{sequence}")
}

pub(super) fn loaded_session(
    repo: &RepoState,
) -> Option<&gitcomet_state::local_review::LocalReviewSession> {
    let gitcomet_state::model::Loadable::Ready(Some(session)) = &repo.local_review.session else {
        return None;
    };
    Some(session)
}

/// Count cached comments anchored to one rendered patch row. Unified context
/// rows accept anchors from both sides; a split column only accepts its side.
/// This is memory-only and safe to call from row rendering.
pub(super) fn comment_count_for_diff_line(
    session: Option<&gitcomet_state::local_review::LocalReviewSession>,
    path: &std::path::Path,
    line: &AnnotatedDiffLine,
    region: DiffTextRegion,
) -> usize {
    let Some(session) = session else { return 0 };
    session
        .comments
        .iter()
        .filter(|comment| comment.anchor.path == path)
        .filter(|comment| match (region, comment.anchor.side) {
            (DiffTextRegion::SplitLeft, Some(ReviewSide::Old)) => {
                comment.anchor.old_line == line.old_line
            }
            (DiffTextRegion::SplitRight, Some(ReviewSide::New)) => {
                comment.anchor.new_line == line.new_line
            }
            (DiffTextRegion::Inline, Some(ReviewSide::Old)) => {
                comment.anchor.old_line == line.old_line
            }
            (DiffTextRegion::Inline, Some(ReviewSide::New)) => {
                comment.anchor.new_line == line.new_line
            }
            (_, None) => false,
            _ => false,
        })
        .count()
}

pub(super) fn comment_count_for_anchor(
    session: Option<&gitcomet_state::local_review::LocalReviewSession>,
    path: &std::path::Path,
    side: ReviewSide,
    line: Option<u32>,
) -> usize {
    let Some(session) = session else { return 0 };
    let Some(line) = line else { return 0 };
    session
        .comments
        .iter()
        .filter(|comment| comment.anchor.path == path && comment.anchor.side == Some(side))
        .filter(|comment| match side {
            ReviewSide::Old => comment.anchor.old_line == Some(line),
            ReviewSide::New => comment.anchor.new_line == Some(line),
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitcomet_core::domain::{CommitId, RepoSpec};
    use gitcomet_state::model::{ComparisonMark, RangeSelection};

    fn line(kind: DiffLineKind, old_line: Option<u32>, new_line: Option<u32>) -> AnnotatedDiffLine {
        AnnotatedDiffLine {
            kind,
            text: "+changed".into(),
            old_line,
            new_line,
        }
    }

    fn repo() -> RepoState {
        let mut repo = RepoState::new_opening(
            RepoId(7),
            RepoSpec {
                workdir: "/tmp/local-review-ui".into(),
            },
        );
        let a = ComparisonMark::commit(CommitId("aaaaaaaa".into()), "main");
        let b = ComparisonMark::commit(CommitId("bbbbbbbb".into()), "agent/worktree");
        repo.comparison_shelf.a = Some(a.clone());
        repo.comparison_shelf.b = Some(b.clone());
        repo.history_state.range_selection = Some(RangeSelection {
            from: a.commit_id().cloned().expect("commit A"),
            to: b.commit_id().cloned(),
            from_label: a.label,
            to_label: b.label,
        });
        repo
    }

    #[test]
    fn added_and_removed_lines_map_to_cli_compatible_sides() {
        let repo = repo();
        let context = range_context(&repo).expect("range context");
        let added = draft_for_diff_line(
            &context,
            "src/lib.rs".into(),
            &line(DiffLineKind::Add, None, Some(42)),
            DiffTextRegion::Inline,
        )
        .expect("added-line draft");
        assert_eq!(added.side, ReviewSide::New);
        assert_eq!(added.old_line, None);
        assert_eq!(added.new_line, Some(42));

        let removed = draft_for_diff_line(
            &context,
            "src/lib.rs".into(),
            &line(DiffLineKind::Remove, Some(17), None),
            DiffTextRegion::SplitLeft,
        )
        .expect("removed-line draft");
        assert_eq!(removed.side, ReviewSide::Old);
        assert_eq!(removed.old_line, Some(17));
        assert_eq!(removed.new_line, None);
    }

    #[test]
    fn payload_uses_v1_session_endpoints_and_anchor() {
        let repo = repo();
        let context = range_context(&repo).expect("range context");
        let draft = draft_for_diff_line(
            &context,
            "src/lib.rs".into(),
            &line(DiffLineKind::Add, None, Some(42)),
            DiffTextRegion::Inline,
        )
        .expect("draft");
        let (session, comment) =
            persistence_payload(&draft, "Please revisit this".into(), "ui-1".into(), 123);
        assert_eq!(session.id, "ab:aaaaaaaa..bbbbbbbb");
        assert_eq!(
            session.base,
            ReviewEndpoint::Commit {
                oid: "aaaaaaaa".into()
            }
        );
        assert_eq!(comment.anchor.path, PathBuf::from("src/lib.rs"));
        assert_eq!(comment.anchor.side, Some(ReviewSide::New));
        assert_eq!(comment.anchor.new_line, Some(42));
        assert_eq!(comment.author.kind, "human");
    }

    #[test]
    fn draft_is_disabled_when_the_visible_range_is_not_the_shelf_pair() {
        let mut repo = repo();
        repo.history_state.range_selection.as_mut().unwrap().to = Some(CommitId("cccccccc".into()));
        assert!(range_context(&repo).is_none());
    }

    #[test]
    fn marker_counts_follow_path_side_and_line_for_inline_and_split() {
        let repo = repo();
        let context = range_context(&repo).expect("range context");
        let draft = draft_for_diff_line(
            &context,
            "src/lib.rs".into(),
            &line(DiffLineKind::Add, None, Some(42)),
            DiffTextRegion::Inline,
        )
        .expect("draft");
        let (mut session, comment) =
            persistence_payload(&draft, "new-side".into(), "new-1".into(), 1);
        session.comments.push(comment);
        let mut old = session.comments[0].clone();
        old.id = "old-1".into();
        old.anchor.side = Some(ReviewSide::Old);
        old.anchor.old_line = Some(17);
        old.anchor.new_line = None;
        old.status = ReviewStatus::Resolved;
        session.comments.push(old);

        let context_line = line(DiffLineKind::Context, Some(17), Some(42));
        assert_eq!(
            comment_count_for_diff_line(
                Some(&session),
                std::path::Path::new("src/lib.rs"),
                &context_line,
                DiffTextRegion::Inline,
            ),
            2
        );
        assert_eq!(
            comment_count_for_anchor(
                Some(&session),
                std::path::Path::new("src/lib.rs"),
                ReviewSide::New,
                Some(42),
            ),
            1
        );
    }
}
