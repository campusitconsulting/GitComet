//! Local, provider-independent review sessions.
//!
//! The on-disk file belongs to the repository's *common* Git directory so all
//! linked worktrees and local agents see the same comments without adding an
//! untracked file to each worktree. Callers resolve the common directory once
//! (for example with `git rev-parse --git-common-dir`) and pass it to
//! [`review_store_path`].

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const REVIEW_STORE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewEndpoint {
    Commit {
        oid: String,
    },
    /// An uncommitted worktree endpoint. `head` records the commit the worktree
    /// was based on when the review session was created.
    Worktree {
        path: PathBuf,
        head: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSide {
    Old,
    New,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Open,
    Resolved,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReviewAnchor {
    pub path: PathBuf,
    /// `None` makes this a file-level comment.
    pub side: Option<ReviewSide>,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    /// Normalized surrounding diff context, hashed by the UI/CLI. This lets a
    /// consumer re-anchor a comment after line numbers move.
    pub context_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReviewAuthor {
    pub name: String,
    /// Examples: `human`, `codex`, `claude`, or another stable agent id.
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReviewComment {
    pub id: String,
    pub anchor: ReviewAnchor,
    pub author: ReviewAuthor,
    pub body: String,
    pub status: ReviewStatus,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LocalReviewSession {
    pub id: String,
    pub title: String,
    pub base: ReviewEndpoint,
    pub head: ReviewEndpoint,
    pub status: ReviewStatus,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    #[serde(default)]
    pub comments: Vec<ReviewComment>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LocalReviewStore {
    pub schema_version: u32,
    /// Incremented for every successful mutation. External agents can use this
    /// as an inexpensive stale-read signal before applying a new edit.
    pub revision: u64,
    #[serde(default)]
    pub sessions: Vec<LocalReviewSession>,
}

impl Default for LocalReviewStore {
    fn default() -> Self {
        Self {
            schema_version: REVIEW_STORE_SCHEMA_VERSION,
            revision: 0,
            sessions: Vec::new(),
        }
    }
}

impl LocalReviewStore {
    pub fn upsert_session(&mut self, mut session: LocalReviewSession) {
        if let Some(existing) = self
            .sessions
            .iter_mut()
            .find(|existing| existing.id == session.id)
        {
            // Creation belongs to the stable session identity, not the latest
            // caller that changed its title/endpoints.
            session.created_at_unix_ms = existing.created_at_unix_ms;
            *existing = session;
        } else {
            self.sessions.push(session);
        }
        self.bump_revision();
    }

    pub fn remove_session(&mut self, session_id: &str) -> bool {
        let before = self.sessions.len();
        self.sessions.retain(|session| session.id != session_id);
        let changed = before != self.sessions.len();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn upsert_comment(&mut self, session_id: &str, mut comment: ReviewComment) -> bool {
        let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        else {
            return false;
        };
        if let Some(existing) = session
            .comments
            .iter_mut()
            .find(|existing| existing.id == comment.id)
        {
            comment.created_at_unix_ms = existing.created_at_unix_ms;
            *existing = comment;
        } else {
            session.comments.push(comment);
        }
        session.updated_at_unix_ms = session
            .comments
            .iter()
            .map(|comment| comment.updated_at_unix_ms)
            .max()
            .unwrap_or(session.updated_at_unix_ms);
        self.bump_revision();
        true
    }

    pub fn remove_comment(&mut self, session_id: &str, comment_id: &str) -> bool {
        let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        else {
            return false;
        };
        let before = session.comments.len();
        session.comments.retain(|comment| comment.id != comment_id);
        let changed = before != session.comments.len();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn set_comment_status(
        &mut self,
        session_id: &str,
        comment_id: &str,
        status: ReviewStatus,
        updated_at_unix_ms: i64,
    ) -> bool {
        let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        else {
            return false;
        };
        let Some(comment) = session
            .comments
            .iter_mut()
            .find(|comment| comment.id == comment_id)
        else {
            return false;
        };
        if comment.status == status && comment.updated_at_unix_ms == updated_at_unix_ms {
            return false;
        }
        comment.status = status;
        comment.updated_at_unix_ms = updated_at_unix_ms;
        session.updated_at_unix_ms = session.updated_at_unix_ms.max(updated_at_unix_ms);
        self.bump_revision();
        true
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

pub fn review_store_path(git_common_dir: &Path) -> PathBuf {
    git_common_dir.join("gitcomet").join("reviews-v1.json")
}

pub fn load_from_path(path: &Path) -> io::Result<LocalReviewStore> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(LocalReviewStore::default());
        }
        Err(error) => return Err(error),
    };
    let store: LocalReviewStore = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if store.schema_version != REVIEW_STORE_SCHEMA_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unsupported local review schema version {} (expected {})",
                store.schema_version, REVIEW_STORE_SCHEMA_VERSION
            ),
        ));
    }
    Ok(store)
}

/// Pretty JSON plus an atomic rename: readers either see the previous complete
/// revision or the next complete revision, never a partially written file.
pub fn persist_to_path(path: &Path, store: &LocalReviewStore) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "review path has no parent"))?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temp, store).map_err(io::Error::other)?;
    use std::io::Write as _;
    temp.write_all(b"\n")?;
    temp.as_file().sync_all()?;
    temp.persist(path).map(|_| ()).map_err(|error| error.error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str) -> LocalReviewSession {
        LocalReviewSession {
            id: id.into(),
            title: "Agent review".into(),
            base: ReviewEndpoint::Commit { oid: "aaa".into() },
            head: ReviewEndpoint::Commit { oid: "bbb".into() },
            status: ReviewStatus::Open,
            created_at_unix_ms: 10,
            updated_at_unix_ms: 10,
            comments: Vec::new(),
        }
    }

    fn comment(id: &str) -> ReviewComment {
        ReviewComment {
            id: id.into(),
            anchor: ReviewAnchor {
                path: "src/main.rs".into(),
                side: Some(ReviewSide::New),
                old_line: None,
                new_line: Some(42),
                context_hash: Some("sha256:context".into()),
            },
            author: ReviewAuthor {
                name: "Codex".into(),
                kind: "codex".into(),
            },
            body: "Please handle the error.".into(),
            status: ReviewStatus::Open,
            created_at_unix_ms: 20,
            updated_at_unix_ms: 20,
        }
    }

    #[test]
    fn comments_are_crud_by_stable_ids_and_preserve_creation_time() {
        let mut store = LocalReviewStore::default();
        store.upsert_session(session("review-1"));
        assert!(store.upsert_comment("review-1", comment("comment-1")));

        let mut edited = comment("comment-1");
        edited.body = "Updated".into();
        edited.created_at_unix_ms = 999;
        edited.updated_at_unix_ms = 30;
        assert!(store.upsert_comment("review-1", edited));

        let saved = &store.sessions[0].comments[0];
        assert_eq!(saved.body, "Updated");
        assert_eq!(saved.created_at_unix_ms, 20);
        assert_eq!(saved.updated_at_unix_ms, 30);
        assert!(store.set_comment_status("review-1", "comment-1", ReviewStatus::Resolved, 40,));
        assert!(store.remove_comment("review-1", "comment-1"));
        assert!(!store.remove_comment("review-1", "missing"));
    }

    #[test]
    fn json_round_trip_is_atomic_and_shared_under_common_git_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = review_store_path(dir.path());
        assert_eq!(path, dir.path().join("gitcomet/reviews-v1.json"));

        let mut store = LocalReviewStore::default();
        store.upsert_session(session("review-1"));
        assert!(store.upsert_comment("review-1", comment("comment-1")));
        persist_to_path(&path, &store).expect("persist");

        let loaded = load_from_path(&path).expect("load");
        assert_eq!(loaded, store);
        let raw = fs::read_to_string(path).expect("read json");
        assert!(raw.ends_with('\n'));
        assert!(raw.contains("\"schema_version\": 1"));
        assert!(raw.contains("\"context_hash\""));
    }

    #[test]
    fn missing_file_is_empty_and_future_schema_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("missing.json");
        assert_eq!(
            load_from_path(&path).expect("missing is empty"),
            LocalReviewStore::default()
        );

        fs::write(&path, r#"{"schema_version":99,"revision":0,"sessions":[]}"#)
            .expect("write future schema");
        assert_eq!(
            load_from_path(&path).expect_err("future schema").kind(),
            io::ErrorKind::InvalidData
        );
    }
}
