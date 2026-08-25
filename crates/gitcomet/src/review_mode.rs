//! Machine-readable CLI facade for repository-local reviews.

use crate::cli::{ReviewAction, ReviewArgs, ReviewCommentSide, exit_code};
use gitcomet_state::local_review::{
    LocalReviewSession, LocalReviewStore, REVIEW_STORE_SCHEMA_VERSION, ReviewAnchor, ReviewAuthor,
    ReviewComment, ReviewEndpoint, ReviewSide, ReviewStatus, load_from_path, persist_to_path,
    review_store_path,
};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const LOCK_WAIT: Duration = Duration::from_secs(5);
const STALE_LOCK_AGE: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewRunResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Serialize)]
struct SessionOutput<'a> {
    schema_version: u32,
    revision: u64,
    session: &'a LocalReviewSession,
}

#[derive(Serialize)]
struct CommentOutput<'a> {
    schema_version: u32,
    revision: u64,
    session_id: &'a str,
    comment: &'a ReviewComment,
}

pub fn run_review(args: &ReviewArgs) -> Result<ReviewRunResult, String> {
    run_review_at(args, now_unix_ms()?)
}

fn run_review_at(args: &ReviewArgs, now: i64) -> Result<ReviewRunResult, String> {
    let common_dir = resolve_git_common_dir(&args.repo)?;
    let store_path = review_store_path(&common_dir);

    let output = match &args.action {
        ReviewAction::List => {
            let store = load_store(&store_path)?;
            require_revision(&store, args.expect_revision)?;
            json_line(&store)?
        }
        ReviewAction::Show { session_id } => {
            require_value(session_id, "session id")?;
            let store = load_store(&store_path)?;
            require_revision(&store, args.expect_revision)?;
            let session = find_session(&store, session_id)?;
            json_line(&SessionOutput {
                schema_version: REVIEW_STORE_SCHEMA_VERSION,
                revision: store.revision,
                session,
            })?
        }
        action => {
            let _lock = ReviewWriterLock::acquire(&store_path)?;
            let mut store = load_store(&store_path)?;
            require_revision(&store, args.expect_revision)?;

            match action {
                ReviewAction::CreateSession {
                    id,
                    title,
                    base,
                    head,
                } => {
                    require_value(id, "session id")?;
                    require_value(title, "title")?;
                    if store.sessions.iter().any(|session| session.id == *id) {
                        return Err(format!(
                            "review session '{id}' already exists; choose a new id"
                        ));
                    }
                    let base = resolve_endpoint(base, &args.repo, &common_dir)?;
                    let head = resolve_endpoint(head, &args.repo, &common_dir)?;
                    store.upsert_session(LocalReviewSession {
                        id: id.clone(),
                        title: title.clone(),
                        base,
                        head,
                        status: ReviewStatus::Open,
                        created_at_unix_ms: now,
                        updated_at_unix_ms: now,
                        comments: Vec::new(),
                    });
                    persist_store(&store_path, &store)?;
                    let session = find_session(&store, id)?;
                    json_line(&SessionOutput {
                        schema_version: REVIEW_STORE_SCHEMA_VERSION,
                        revision: store.revision,
                        session,
                    })?
                }
                ReviewAction::AddComment {
                    session_id,
                    id,
                    path,
                    side,
                    old_line,
                    new_line,
                    context_hash,
                    author,
                    author_kind,
                    body,
                } => {
                    require_value(session_id, "session id")?;
                    require_value(id, "comment id")?;
                    require_value(author, "author")?;
                    require_value(author_kind, "author kind")?;
                    require_value(body, "comment body")?;
                    if path.as_os_str().is_empty() {
                        return Err("comment path must not be empty".into());
                    }
                    validate_anchor(*side, *old_line, *new_line)?;
                    let session = find_session(&store, session_id)?;
                    if session.comments.iter().any(|comment| comment.id == *id) {
                        return Err(format!(
                            "comment '{id}' already exists in session '{session_id}'; choose a new id"
                        ));
                    }
                    let comment = ReviewComment {
                        id: id.clone(),
                        anchor: ReviewAnchor {
                            path: path.clone(),
                            side: side.map(|side| match side {
                                ReviewCommentSide::Old => ReviewSide::Old,
                                ReviewCommentSide::New => ReviewSide::New,
                            }),
                            old_line: *old_line,
                            new_line: *new_line,
                            context_hash: context_hash.clone(),
                        },
                        author: ReviewAuthor {
                            name: author.clone(),
                            kind: author_kind.clone(),
                        },
                        body: body.clone(),
                        status: ReviewStatus::Open,
                        created_at_unix_ms: now,
                        updated_at_unix_ms: now,
                    };
                    // Existence was checked above, so false can only indicate
                    // an inconsistent store mutation.
                    if !store.upsert_comment(session_id, comment) {
                        return Err(format!("review session '{session_id}' was not found"));
                    }
                    persist_store(&store_path, &store)?;
                    let comment = find_comment(&store, session_id, id)?;
                    json_line(&CommentOutput {
                        schema_version: REVIEW_STORE_SCHEMA_VERSION,
                        revision: store.revision,
                        session_id,
                        comment,
                    })?
                }
                ReviewAction::ResolveComment {
                    session_id,
                    comment_id,
                } => {
                    require_value(session_id, "session id")?;
                    require_value(comment_id, "comment id")?;
                    let existing = find_comment(&store, session_id, comment_id)?;
                    // Resolving is idempotent: if an agent did not receive the
                    // first command's output, retrying does not bump revision
                    // or rewrite timestamps a second time.
                    if existing.status != ReviewStatus::Resolved {
                        if !store.set_comment_status(
                            session_id,
                            comment_id,
                            ReviewStatus::Resolved,
                            now,
                        ) {
                            return Err(format!(
                                "failed to resolve comment '{comment_id}' in session '{session_id}'"
                            ));
                        }
                        persist_store(&store_path, &store)?;
                    }
                    let comment = find_comment(&store, session_id, comment_id)?;
                    json_line(&CommentOutput {
                        schema_version: REVIEW_STORE_SCHEMA_VERSION,
                        revision: store.revision,
                        session_id,
                        comment,
                    })?
                }
                ReviewAction::List | ReviewAction::Show { .. } => unreachable!(),
            }
        }
    };

    Ok(ReviewRunResult {
        stdout: output,
        stderr: String::new(),
        exit_code: exit_code::SUCCESS,
    })
}

fn load_store(path: &Path) -> Result<LocalReviewStore, String> {
    load_from_path(path).map_err(|error| {
        format!(
            "failed to read local reviews at {}: {error}",
            path.display()
        )
    })
}

fn persist_store(path: &Path, store: &LocalReviewStore) -> Result<(), String> {
    persist_to_path(path, store).map_err(|error| {
        format!(
            "failed to write local reviews at {}: {error}",
            path.display()
        )
    })
}

fn json_line(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map(|json| format!("{json}\n"))
        .map_err(|error| format!("failed to encode local review JSON: {error}"))
}

fn require_value(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{label} must not be empty"))
    } else {
        Ok(())
    }
}

fn require_revision(store: &LocalReviewStore, expected: Option<u64>) -> Result<(), String> {
    if let Some(expected) = expected
        && store.revision != expected
    {
        return Err(format!(
            "local review revision changed: expected {expected}, found {}; re-read the store and retry",
            store.revision
        ));
    }
    Ok(())
}

fn find_session<'a>(
    store: &'a LocalReviewStore,
    session_id: &str,
) -> Result<&'a LocalReviewSession, String> {
    store
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| format!("review session '{session_id}' was not found"))
}

fn find_comment<'a>(
    store: &'a LocalReviewStore,
    session_id: &str,
    comment_id: &str,
) -> Result<&'a ReviewComment, String> {
    find_session(store, session_id)?
        .comments
        .iter()
        .find(|comment| comment.id == comment_id)
        .ok_or_else(|| {
            format!("comment '{comment_id}' was not found in review session '{session_id}'")
        })
}

fn validate_anchor(
    side: Option<ReviewCommentSide>,
    old_line: Option<u32>,
    new_line: Option<u32>,
) -> Result<(), String> {
    if old_line == Some(0) || new_line == Some(0) {
        return Err("diff line numbers are one-based and must be greater than zero".into());
    }
    match side {
        None if old_line.is_some() || new_line.is_some() => {
            Err("--side is required when --old-line or --new-line is present".into())
        }
        Some(ReviewCommentSide::Old) if old_line.is_none() => {
            Err("--old-line is required for --side old".into())
        }
        Some(ReviewCommentSide::New) if new_line.is_none() => {
            Err("--new-line is required for --side new".into())
        }
        _ => Ok(()),
    }
}

fn now_unix_ms() -> Result<i64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
        .as_millis();
    i64::try_from(millis).map_err(|_| "current Unix timestamp does not fit in i64".into())
}

fn run_git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| format!("failed to run git for {}: {error}", repo.display()))?;
    if !output.status.success() {
        let details = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let details = if details.is_empty() {
            format!("exit status {}", output.status)
        } else {
            details
        };
        return Err(format!(
            "failed to inspect Git repository at {}: {details}",
            repo.display()
        ));
    }
    let value = String::from_utf8(output.stdout)
        .map_err(|_| "git returned non-UTF-8 output while locating review data".to_string())?;
    let value = value.trim();
    if value.is_empty() {
        Err(format!(
            "git returned an empty result for repository {}",
            repo.display()
        ))
    } else {
        Ok(value.to_string())
    }
}

/// Resolve the common Git directory shared by the main checkout and every
/// linked worktree. `--path-format=absolute` also handles callers located in a
/// nested directory without interpreting Git's relative path in the wrong cwd.
pub(crate) fn resolve_git_common_dir(repo: &Path) -> Result<PathBuf, String> {
    let path = run_git(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    fs::canonicalize(&path).map_err(|error| {
        format!(
            "failed to resolve Git common directory {}: {error}",
            Path::new(&path).display()
        )
    })
}

fn resolve_endpoint(
    specification: &str,
    repo: &Path,
    expected_common_dir: &Path,
) -> Result<ReviewEndpoint, String> {
    require_value(specification, "review endpoint")?;
    if let Some(path) = specification.strip_prefix("worktree:") {
        require_value(path, "worktree endpoint path")?;
        let candidate = Path::new(path);
        let candidate = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            repo.join(candidate)
        };
        let canonical = fs::canonicalize(&candidate).map_err(|error| {
            format!(
                "failed to resolve worktree endpoint {}: {error}",
                candidate.display()
            )
        })?;
        let common_dir = resolve_git_common_dir(&canonical)?;
        if common_dir != expected_common_dir {
            return Err(format!(
                "worktree endpoint {} belongs to a different Git repository",
                canonical.display()
            ));
        }
        let head = run_git(&canonical, &["rev-parse", "--verify", "HEAD^{commit}"])?;
        return Ok(ReviewEndpoint::Worktree {
            path: canonical,
            head: Some(head),
        });
    }

    let revision = specification
        .strip_prefix("commit:")
        .unwrap_or(specification);
    require_value(revision, "commit revision")?;
    let commit_expression = format!("{revision}^{{commit}}");
    let oid = run_git(repo, &["rev-parse", "--verify", &commit_expression])
        .map_err(|error| format!("failed to resolve commit endpoint '{revision}': {error}"))?;
    Ok(ReviewEndpoint::Commit { oid })
}

struct ReviewWriterLock {
    path: PathBuf,
}

impl ReviewWriterLock {
    fn acquire(store_path: &Path) -> Result<Self, String> {
        let parent = store_path
            .parent()
            .ok_or_else(|| "local review store path has no parent".to_string())?;
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create local review directory {}: {error}",
                parent.display()
            )
        })?;
        let path = parent.join("reviews-v1.lock");
        let deadline = Instant::now() + LOCK_WAIT;
        loop {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if lock_is_stale(&path) {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    if Instant::now() >= deadline {
                        return Err(format!(
                            "timed out waiting for another local review writer at {}; retry the command",
                            path.display()
                        ));
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                Err(error) => {
                    return Err(format!(
                        "failed to acquire local review writer lock {}: {error}",
                        path.display()
                    ));
                }
            }
        }
    }
}

impl Drop for ReviewWriterLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn lock_is_stale(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .and_then(|modified| modified.elapsed().map_err(std::io::Error::other))
        .is_ok_and(|age| age >= STALE_LOCK_AGE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn git(dir: &Path, args: &[&OsStr]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &[OsStr::new("init")]);
        git(
            dir.path(),
            &[
                OsStr::new("config"),
                OsStr::new("user.name"),
                OsStr::new("Test"),
            ],
        );
        git(
            dir.path(),
            &[
                OsStr::new("config"),
                OsStr::new("user.email"),
                OsStr::new("test@example.invalid"),
            ],
        );
        fs::write(dir.path().join("file.txt"), "first\n").expect("write fixture");
        git(dir.path(), &[OsStr::new("add"), OsStr::new("file.txt")]);
        git(
            dir.path(),
            &[OsStr::new("commit"), OsStr::new("-m"), OsStr::new("first")],
        );
        dir
    }

    fn args(repo: &Path, action: ReviewAction) -> ReviewArgs {
        ReviewArgs {
            repo: repo.to_path_buf(),
            expect_revision: None,
            action,
        }
    }

    #[test]
    fn main_checkout_and_linked_worktree_share_common_review_store() {
        let repo = init_repo();
        let worktree_parent = tempfile::tempdir().expect("worktree parent");
        let worktree = worktree_parent.path().join("linked");
        git(
            repo.path(),
            &[
                OsStr::new("worktree"),
                OsStr::new("add"),
                OsStr::new("--detach"),
                worktree.as_os_str(),
            ],
        );

        assert_eq!(
            resolve_git_common_dir(repo.path()).expect("main common dir"),
            resolve_git_common_dir(&worktree).expect("worktree common dir")
        );

        run_review_at(
            &args(
                &worktree,
                ReviewAction::CreateSession {
                    id: "review-1".into(),
                    title: "Shared review".into(),
                    base: "HEAD".into(),
                    head: format!("worktree:{}", worktree.display()),
                },
            ),
            100,
        )
        .expect("create through linked worktree");

        let result = run_review_at(&args(repo.path(), ReviewAction::List), 200)
            .expect("list through main checkout");
        let store: LocalReviewStore = serde_json::from_str(&result.stdout).expect("store json");
        assert_eq!(store.sessions.len(), 1);
        assert_eq!(store.sessions[0].id, "review-1");
        assert!(matches!(
            store.sessions[0].head,
            ReviewEndpoint::Worktree { .. }
        ));
    }

    #[test]
    fn create_comment_resolve_and_revision_guard_are_json_first() {
        let repo = init_repo();
        let created = run_review_at(
            &args(
                repo.path(),
                ReviewAction::CreateSession {
                    id: "review-1".into(),
                    title: "Agent review".into(),
                    base: "HEAD".into(),
                    head: "commit:HEAD".into(),
                },
            ),
            100,
        )
        .expect("create session");
        let created: serde_json::Value =
            serde_json::from_str(&created.stdout).expect("created JSON");
        assert_eq!(created["revision"], 1);
        assert_eq!(created["session"]["base"]["kind"], "commit");

        let mut add = args(
            repo.path(),
            ReviewAction::AddComment {
                session_id: "review-1".into(),
                id: "comment-1".into(),
                path: "src/main.rs".into(),
                side: Some(ReviewCommentSide::New),
                old_line: None,
                new_line: Some(42),
                context_hash: Some("sha256:context".into()),
                author: "Codex".into(),
                author_kind: "codex".into(),
                body: "Handle this error".into(),
            },
        );
        add.expect_revision = Some(1);
        let added = run_review_at(&add, 200).expect("add comment");
        let added: serde_json::Value = serde_json::from_str(&added.stdout).expect("comment JSON");
        assert_eq!(added["revision"], 2);
        assert_eq!(added["comment"]["status"], "open");

        let stale = run_review_at(&add, 250).expect_err("stale revision rejected");
        assert!(stale.contains("expected 1, found 2"));

        let resolved = run_review_at(
            &args(
                repo.path(),
                ReviewAction::ResolveComment {
                    session_id: "review-1".into(),
                    comment_id: "comment-1".into(),
                },
            ),
            300,
        )
        .expect("resolve comment");
        let resolved: serde_json::Value =
            serde_json::from_str(&resolved.stdout).expect("resolved JSON");
        assert_eq!(resolved["revision"], 3);
        assert_eq!(resolved["comment"]["status"], "resolved");

        let retried = run_review_at(
            &args(
                repo.path(),
                ReviewAction::ResolveComment {
                    session_id: "review-1".into(),
                    comment_id: "comment-1".into(),
                },
            ),
            350,
        )
        .expect("resolve retry is idempotent");
        let retried: serde_json::Value = serde_json::from_str(&retried.stdout).expect("retry JSON");
        assert_eq!(retried["revision"], 3);
        assert_eq!(retried["comment"]["updated_at_unix_ms"], 300);

        let shown = run_review_at(
            &args(
                repo.path(),
                ReviewAction::Show {
                    session_id: "review-1".into(),
                },
            ),
            400,
        )
        .expect("show session");
        let shown: serde_json::Value = serde_json::from_str(&shown.stdout).expect("show JSON");
        assert_eq!(shown["session"]["comments"][0]["updated_at_unix_ms"], 300);
        assert_eq!(shown["session"]["updated_at_unix_ms"], 300);
    }

    #[test]
    fn invalid_anchor_and_foreign_worktree_are_rejected() {
        assert!(
            validate_anchor(Some(ReviewCommentSide::Old), None, Some(1))
                .expect_err("missing old line")
                .contains("--old-line")
        );

        let repo = init_repo();
        let foreign = init_repo();
        let common = resolve_git_common_dir(repo.path()).expect("common dir");
        let error = resolve_endpoint(
            &format!("worktree:{}", foreign.path().display()),
            repo.path(),
            &common,
        )
        .expect_err("foreign worktree");
        assert!(error.contains("different Git repository"));
    }
}
