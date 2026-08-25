use super::GixRepo;
use crate::util::{path_buf_from_git_bytes, run_git_capture_bytes, run_git_with_output};
use gitcomet_core::domain::{CommitId, Worktree};
use gitcomet_core::error::{Error, ErrorKind};
use gitcomet_core::path_utils::canonicalize_or_original;
use gitcomet_core::services::{CommandOutput, Result};
use std::path::Path;

impl GixRepo {
    pub(super) fn list_worktrees_impl(&self) -> Result<Vec<Worktree>> {
        let mut cmd = self.git_workdir_cmd();
        cmd.arg("worktree").arg("list").arg("--porcelain").arg("-z");
        let output = run_git_capture_bytes(cmd, "git worktree list --porcelain -z")?;
        parse_git_worktree_list_porcelain_z(&output)
    }

    pub(super) fn snapshot_worktree_impl(&self, worktree: &Path) -> Result<CommitId> {
        let temp = tempfile::tempdir().map_err(|error| Error::new(ErrorKind::Io(error.kind())))?;
        let index = temp.path().join("snapshot.index");
        let command = |args: &[&str]| {
            let mut cmd = crate::util::git_workdir_cmd_for(worktree);
            cmd.env("GIT_INDEX_FILE", &index).args(args);
            cmd
        };

        // Seed from HEAD so tracked-but-ignored files remain part of the
        // snapshot. Unborn worktrees have no HEAD and start from an empty tree.
        if run_git_capture_bytes(command(&["read-tree", "HEAD"]), "git read-tree HEAD").is_err() {
            run_git_capture_bytes(command(&["read-tree", "--empty"]), "git read-tree --empty")?;
        }
        run_git_capture_bytes(command(&["add", "-A", "--", "."]), "git add snapshot")?;
        let output = run_git_capture_bytes(command(&["write-tree"]), "git write-tree snapshot")?;
        let tree_id = String::from_utf8(output).map_err(|_| {
            Error::new(ErrorKind::Backend(
                "git write-tree returned non-UTF-8".into(),
            ))
        })?;
        let tree_id = tree_id.trim();
        if tree_id.is_empty() {
            return Err(Error::new(ErrorKind::Backend(
                "git write-tree returned an empty object id".into(),
            )));
        }
        Ok(CommitId(tree_id.to_string().into()))
    }

    pub(super) fn add_worktree_with_output_impl(
        &self,
        path: &Path,
        reference: Option<&str>,
    ) -> Result<CommandOutput> {
        let mut cmd = self.git_workdir_cmd();
        cmd.arg("worktree").arg("add").arg(path);
        let label = if let Some(reference) = reference {
            cmd.arg(reference);
            format!("git worktree add {} {}", path.display(), reference)
        } else {
            format!("git worktree add {}", path.display())
        };
        run_git_with_output(cmd, &label)
    }

    pub(super) fn remove_worktree_with_output_impl(&self, path: &Path) -> Result<CommandOutput> {
        let mut cmd = self.git_workdir_cmd();
        cmd.arg("worktree").arg("remove").arg(path);
        run_git_with_output(cmd, &format!("git worktree remove {}", path.display()))
    }

    pub(super) fn force_remove_worktree_with_output_impl(
        &self,
        path: &Path,
    ) -> Result<CommandOutput> {
        let mut cmd = self.git_workdir_cmd();
        cmd.arg("worktree").arg("remove").arg("--force").arg(path);
        run_git_with_output(
            cmd,
            &format!("git worktree remove --force {}", path.display()),
        )
    }
}

fn parse_git_worktree_list_porcelain_z(output: &[u8]) -> Result<Vec<Worktree>> {
    let mut out = Vec::new();
    let mut current: Option<Worktree> = None;

    for field in output.split(|b| *b == b'\0') {
        if field.is_empty() {
            if let Some(mut wt) = current.take() {
                canonicalize_worktree_path(&mut wt);
                out.push(wt);
            }
            continue;
        }

        if let Some(rest) = field.strip_prefix(b"worktree ") {
            if let Some(mut wt) = current.take() {
                canonicalize_worktree_path(&mut wt);
                out.push(wt);
            }
            current = Some(Worktree {
                path: path_buf_from_git_bytes(rest, "git worktree list path")?,
                head: None,
                branch: None,
                detached: false,
            });
            continue;
        }

        let Some(wt) = current.as_mut() else {
            continue;
        };

        if let Some(rest) = field.strip_prefix(b"HEAD ") {
            if !rest.is_empty() {
                wt.head = Some(CommitId(String::from_utf8_lossy(rest).into_owned().into()));
            }
        } else if let Some(rest) = field.strip_prefix(b"branch ") {
            let branch = String::from_utf8_lossy(rest);
            if let Some(stripped) = branch.strip_prefix("refs/heads/") {
                wt.branch = Some(stripped.to_string());
            } else if !branch.is_empty() {
                wt.branch = Some(branch.into_owned());
            }
        } else if field == b"detached" {
            wt.detached = true;
            wt.branch = None;
        }
    }

    if let Some(mut wt) = current.take() {
        canonicalize_worktree_path(&mut wt);
        out.push(wt);
    }

    Ok(out)
}

fn canonicalize_worktree_path(worktree: &mut Worktree) {
    worktree.path = canonicalize_or_original(worktree.path.clone());
}

#[cfg(test)]
mod tests {
    use super::{GixRepo, parse_git_worktree_list_porcelain_z};
    use gitcomet_core::path_utils::canonicalize_or_original;
    use gitcomet_core::services::GitRepository;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn git(repo: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("utf8 git output")
            .trim()
            .to_string()
    }

    fn write(path: &Path, text: &str) {
        std::fs::write(path, text).expect("write fixture");
    }

    #[test]
    fn snapshots_two_linked_worktrees_without_mutating_their_indexes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let main = temp.path().join("main");
        let left = temp.path().join("left");
        let right = temp.path().join("right");
        std::fs::create_dir(&main).expect("main dir");
        git(&main, &["init"]);
        git(&main, &["config", "user.name", "GitComet Test"]);
        git(&main, &["config", "user.email", "gitcomet@example.invalid"]);
        write(&main.join("shared.txt"), "base\n");
        git(&main, &["add", "."]);
        git(&main, &["commit", "-m", "base"]);
        git(
            &main,
            &["worktree", "add", "-b", "left", left.to_str().unwrap()],
        );
        git(
            &main,
            &["worktree", "add", "-b", "right", right.to_str().unwrap()],
        );

        write(&left.join("shared.txt"), "left\n");
        write(&left.join("left-only.txt"), "left only\n");
        git(&left, &["add", "shared.txt"]);
        write(&right.join("shared.txt"), "right\n");
        write(&right.join("right-only.txt"), "right only\n");
        let left_status = git(&left, &["status", "--porcelain=v1"]);
        let right_status = git(&right, &["status", "--porcelain=v1"]);

        let opened = crate::open::open_worktree_repo(&main).expect("open main");
        let repo = GixRepo::new(main.clone(), opened.into_sync());
        let left_tree = repo.snapshot_worktree(&left).expect("snapshot left");
        let right_tree = repo.snapshot_worktree(&right).expect("snapshot right");
        let mut files = repo
            .diff_range_files(&left_tree, Some(&right_tree))
            .expect("diff snapshot trees");
        files.sort_by(|a, b| a.path.cmp(&b.path));

        assert_eq!(
            files
                .iter()
                .map(|file| file.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["left-only.txt", "right-only.txt", "shared.txt"]
        );
        assert_eq!(git(&left, &["status", "--porcelain=v1"]), left_status);
        assert_eq!(git(&right, &["status", "--porcelain=v1"]), right_status);
    }

    #[test]
    fn parse_git_worktree_list_porcelain_z_parses_regular_and_detached_entries() {
        let parsed = parse_git_worktree_list_porcelain_z(
            b"worktree /repo\0HEAD 1111111111111111111111111111111111111111\0branch refs/heads/main\0\0worktree /repo-linked\0HEAD 2222222222222222222222222222222222222222\0detached\0\0",
        )
        .unwrap();

        assert_eq!(parsed.len(), 2);

        assert_eq!(parsed[0].path, PathBuf::from("/repo"));
        assert_eq!(
            parsed[0].head.as_ref().map(|id| id.as_ref()),
            Some("1111111111111111111111111111111111111111")
        );
        assert_eq!(parsed[0].branch.as_deref(), Some("main"));
        assert!(!parsed[0].detached);

        assert_eq!(parsed[1].path, PathBuf::from("/repo-linked"));
        assert_eq!(
            parsed[1].head.as_ref().map(|id| id.as_ref()),
            Some("2222222222222222222222222222222222222222")
        );
        assert!(parsed[1].branch.is_none());
        assert!(parsed[1].detached);
    }

    #[test]
    fn parse_git_worktree_list_porcelain_z_ignores_noise_before_first_worktree() {
        let parsed = parse_git_worktree_list_porcelain_z(
            b"HEAD deadbeef\0branch refs/heads/ignored\0\0worktree /repo\0branch feature/topic\0\0",
        )
        .unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].path, PathBuf::from("/repo"));
        assert_eq!(parsed[0].branch.as_deref(), Some("feature/topic"));
        assert!(parsed[0].head.is_none());
    }

    #[test]
    fn parse_git_worktree_list_porcelain_z_skips_empty_head_values() {
        let parsed = parse_git_worktree_list_porcelain_z(
            b"worktree /repo\0HEAD \0branch refs/heads/main\0\0",
        )
        .unwrap();

        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].head.is_none());
        assert_eq!(parsed[0].branch.as_deref(), Some("main"));
    }

    #[test]
    fn parse_git_worktree_list_porcelain_z_preserves_newlines_in_paths() {
        let parsed = parse_git_worktree_list_porcelain_z(
            b"worktree /repo\nlinked\0HEAD 1111111111111111111111111111111111111111\0detached\0\0",
        )
        .unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].path, PathBuf::from("/repo\nlinked"));
        assert!(parsed[0].detached);
    }

    #[test]
    fn parse_git_worktree_list_porcelain_z_canonicalizes_existing_worktree_paths() {
        let root = std::env::temp_dir().join(format!(
            "gitcomet-worktree-parse-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let nested = root.join("repo");
        std::fs::create_dir_all(&nested).unwrap();

        let input = format!(
            "worktree {}\0HEAD 1111111111111111111111111111111111111111\0branch refs/heads/main\0\0",
            nested.join("..").join("repo").display()
        );
        let parsed = parse_git_worktree_list_porcelain_z(input.as_bytes()).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].path, canonicalize_or_original(nested.clone()));
    }
}
