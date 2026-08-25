//! Local, provider-independent review sessions.
//!
//! The on-disk file belongs to the repository's *common* Git directory so all
//! linked worktrees and local agents see the same comments without adding an
//! untracked file to each worktree. Callers resolve the common directory once
//! (for example with `git rev-parse --git-common-dir`) and pass it to
//! [`review_store_path`].

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

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

const REVIEW_LOCK_WAIT: Duration = Duration::from_secs(5);
const REVIEW_STALE_LOCK_AGE: Duration = Duration::from_secs(30);

/// Resolve the common Git directory without invoking Git.
///
/// A normal checkout owns a `.git` directory. Linked worktrees and submodules
/// instead carry a `.git` file pointing at their administrative directory,
/// whose optional `commondir` file points back to storage shared by every
/// checkout. Keeping this resolver in the schema crate lets the UI and CLI use
/// the same sidecar location without putting filesystem work on the UI thread.
pub fn resolve_git_common_dir(workdir: &Path) -> io::Result<PathBuf> {
    let dot_git = workdir.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else {
        let pointer = fs::read_to_string(&dot_git)?;
        let value = pointer
            .trim()
            .strip_prefix("gitdir:")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid Git directory pointer at {}", dot_git.display()),
                )
            })?;
        let value = PathBuf::from(value);
        if value.is_absolute() {
            value
        } else {
            workdir.join(value)
        }
    };
    let git_dir = fs::canonicalize(git_dir)?;
    let common_dir = match fs::read_to_string(git_dir.join("commondir")) {
        Ok(value) => {
            let value = value.trim();
            if value.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("empty commondir in {}", git_dir.display()),
                ));
            }
            let value = PathBuf::from(value);
            if value.is_absolute() {
                value
            } else {
                git_dir.join(value)
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => git_dir,
        Err(error) => return Err(error),
    };
    fs::canonicalize(common_dir)
}

struct ReviewWriterLock {
    path: PathBuf,
}

impl ReviewWriterLock {
    fn acquire(store_path: &Path) -> io::Result<Self> {
        let parent = store_path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "review path has no parent")
        })?;
        fs::create_dir_all(parent)?;
        let path = parent.join("reviews-v1.lock");
        let deadline = Instant::now() + REVIEW_LOCK_WAIT;
        loop {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    let stale = fs::metadata(&path)
                        .and_then(|metadata| metadata.modified())
                        .and_then(|modified| {
                            SystemTime::now()
                                .duration_since(modified)
                                .map_err(io::Error::other)
                        })
                        .is_ok_and(|age| age >= REVIEW_STALE_LOCK_AGE);
                    if stale {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!(
                                "timed out waiting for another local review writer at {}",
                                path.display()
                            ),
                        ));
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for ReviewWriterLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Atomically add a UI/agent-compatible comment to the repository-local store.
///
/// The lock name and stale-lock policy intentionally match the CLI protocol.
/// An existing session is preserved instead of being replaced, so a UI retry
/// cannot discard comments another agent added to the same A/B session.
pub fn persist_comment_for_workdir(
    workdir: &Path,
    session: LocalReviewSession,
    comment: ReviewComment,
) -> io::Result<(PathBuf, u64)> {
    let common_dir = resolve_git_common_dir(workdir)?;
    let path = review_store_path(&common_dir);
    let _lock = ReviewWriterLock::acquire(&path)?;
    let mut store = load_from_path(&path)?;

    if !store.sessions.iter().any(|saved| saved.id == session.id) {
        store.upsert_session(session.clone());
    }
    if store
        .sessions
        .iter()
        .find(|saved| saved.id == session.id)
        .is_some_and(|saved| saved.comments.iter().any(|saved| saved.id == comment.id))
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("local review comment '{}' already exists", comment.id),
        ));
    }
    if !store.upsert_comment(&session.id, comment) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("local review session '{}' was not found", session.id),
        ));
    }
    persist_to_path(&path, &store)?;
    Ok((path, store.revision))
}

/// Load one session from the shared sidecar without holding a writer lock.
/// Atomic rename guarantees that readers see either the old or new document.
pub fn load_session_for_workdir(
    workdir: &Path,
    session_id: &str,
) -> io::Result<(PathBuf, u64, Option<LocalReviewSession>)> {
    let common_dir = resolve_git_common_dir(workdir)?;
    let path = review_store_path(&common_dir);
    let store = load_from_path(&path)?;
    let session = store
        .sessions
        .into_iter()
        .find(|session| session.id == session_id);
    Ok((path, store.revision, session))
}

/// Resolve or reopen one comment under the same writer lock used by comment
/// creation and the CLI. Re-reading after lock acquisition prevents an older UI
/// snapshot from overwriting comments written by an external agent.
pub fn set_comment_status_for_workdir(
    workdir: &Path,
    session_id: &str,
    comment_id: &str,
    status: ReviewStatus,
    updated_at_unix_ms: i64,
) -> io::Result<(PathBuf, u64)> {
    let common_dir = resolve_git_common_dir(workdir)?;
    let path = review_store_path(&common_dir);
    let _lock = ReviewWriterLock::acquire(&path)?;
    let mut store = load_from_path(&path)?;
    let Some(comment) = store
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .and_then(|session| {
            session
                .comments
                .iter()
                .find(|comment| comment.id == comment_id)
        })
    else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("local review comment '{session_id}/{comment_id}' was not found"),
        ));
    };
    if comment.status == status {
        return Ok((path, store.revision));
    }
    if !store.set_comment_status(session_id, comment_id, status, updated_at_unix_ms) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("local review comment '{session_id}/{comment_id}' was not found"),
        ));
    }
    persist_to_path(&path, &store)?;
    Ok((path, store.revision))
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

    #[test]
    fn linked_worktree_comments_share_the_common_store_without_replacing_agent_comments() {
        let dir = tempfile::tempdir().expect("tempdir");
        let common = dir.path().join("main.git");
        let admin = common.join("worktrees/review-one");
        let worktree = dir.path().join("review-one");
        fs::create_dir_all(&admin).expect("admin dir");
        fs::create_dir_all(&worktree).expect("worktree dir");
        fs::write(
            worktree.join(".git"),
            "gitdir: ../main.git/worktrees/review-one\n",
        )
        .expect("gitdir pointer");
        fs::write(admin.join("commondir"), "../..\n").expect("commondir pointer");

        let canonical_common = fs::canonicalize(&common).expect("canonical common dir");
        assert_eq!(
            resolve_git_common_dir(&worktree).expect("resolve"),
            canonical_common
        );

        let first = comment("agent-comment");
        let (path, first_revision) =
            persist_comment_for_workdir(&worktree, session("ab-session"), first)
                .expect("first comment");
        assert_eq!(path, canonical_common.join("gitcomet/reviews-v1.json"));

        let mut second = comment("ui-comment");
        second.body = "Comment from the diff UI".into();
        let (_, second_revision) =
            persist_comment_for_workdir(&worktree, session("ab-session"), second)
                .expect("second comment");
        assert!(second_revision > first_revision);

        let saved = load_from_path(&path).expect("load shared store");
        assert_eq!(saved.sessions.len(), 1);
        assert_eq!(saved.sessions[0].comments.len(), 2);
        assert!(
            saved.sessions[0]
                .comments
                .iter()
                .any(|comment| comment.id == "agent-comment")
        );
        assert!(
            saved.sessions[0]
                .comments
                .iter()
                .any(|comment| comment.id == "ui-comment")
        );
    }

    #[test]
    fn submodule_gitfile_stores_reviews_in_the_module_admin_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let module_admin = dir.path().join("super/.git/modules/child");
        let submodule = dir.path().join("super/child");
        fs::create_dir_all(&module_admin).expect("module admin dir");
        fs::create_dir_all(&submodule).expect("submodule worktree");
        fs::write(submodule.join(".git"), "gitdir: ../.git/modules/child\n")
            .expect("submodule gitdir pointer");

        let canonical_admin = fs::canonicalize(&module_admin).expect("canonical module admin");
        assert_eq!(
            resolve_git_common_dir(&submodule).expect("resolve submodule"),
            canonical_admin
        );
        let (path, revision) = persist_comment_for_workdir(
            &submodule,
            session("submodule-review"),
            comment("submodule-comment"),
        )
        .expect("persist submodule comment");
        assert_eq!(path, canonical_admin.join("gitcomet/reviews-v1.json"));
        assert_eq!(revision, 2);
    }

    #[test]
    fn load_and_resolve_thread_re_read_the_shared_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join(".git")).expect("git dir");
        persist_comment_for_workdir(dir.path(), session("ab-session"), comment("thread-1"))
            .expect("persist comment");

        let (_, before_revision, loaded) =
            load_session_for_workdir(dir.path(), "ab-session").expect("load session");
        assert_eq!(
            loaded.as_ref().unwrap().comments[0].status,
            ReviewStatus::Open
        );

        let (_, resolved_revision) = set_comment_status_for_workdir(
            dir.path(),
            "ab-session",
            "thread-1",
            ReviewStatus::Resolved,
            99,
        )
        .expect("resolve");
        assert!(resolved_revision > before_revision);
        let (_, _, loaded) =
            load_session_for_workdir(dir.path(), "ab-session").expect("reload session");
        assert_eq!(
            loaded.as_ref().unwrap().comments[0].status,
            ReviewStatus::Resolved
        );
        assert_eq!(loaded.as_ref().unwrap().comments[0].updated_at_unix_ms, 99);
    }
}
