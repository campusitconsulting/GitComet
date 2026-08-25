use super::*;

use crate::view::shortcut_labels::secondary_shortcut;

pub(super) fn model(
    repo_id: RepoId,
    path: &std::path::Path,
    branch: Option<&str>,
    head: Option<&CommitId>,
) -> ContextMenuModel {
    let mut items = vec![ContextMenuItem::Header("Worktree".into())];
    items.push(ContextMenuItem::Label(
        components::ContextMenuText::path_single_line(path.display().to_string()),
    ));
    items.push(ContextMenuItem::Separator);
    items.push(ContextMenuItem::Entry {
        label: "Open in new tab".into(),
        icon: Some("icons/open_external.svg".into()),
        shortcut: None,
        disabled: false,
        action: Box::new(ContextMenuAction::OpenRepo {
            path: path.to_path_buf(),
        }),
    });
    if crate::external_editor::configured_setting().is_some() {
        items.push(ContextMenuItem::Entry {
            label: "Open in code editor".into(),
            icon: Some("icons/open_external.svg".into()),
            shortcut: Some(secondary_shortcut("Shift+E").into()),
            disabled: false,
            action: Box::new(ContextMenuAction::OpenInCodeEditor {
                repo_id: None,
                path: path.to_path_buf(),
            }),
        });
    }
    if let Some(head) = head {
        let worktree_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("worktree");
        let endpoint_label = match branch {
            Some(branch) => format!("{branch} · {worktree_name}"),
            None => format!(
                "{} · {worktree_name}",
                head.as_ref().chars().take(8).collect::<String>()
            ),
        };
        let endpoint = gitcomet_state::model::ComparisonMark::commit(head.clone(), endpoint_label);
        items.push(ContextMenuItem::Separator);
        for (slot, slot_label) in [
            (gitcomet_state::model::ComparisonSlot::A, "A"),
            (gitcomet_state::model::ComparisonSlot::B, "B"),
        ] {
            items.push(ContextMenuItem::Entry {
                label: format!("Set worktree HEAD as comparison {slot_label}").into(),
                icon: Some("icons/git_branch.svg".into()),
                shortcut: None,
                disabled: false,
                action: Box::new(ContextMenuAction::SetComparisonSlot {
                    repo_id,
                    slot,
                    endpoint: endpoint.clone(),
                }),
            });
        }
    }
    let worktree_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("worktree");
    let dirty_label = format!(
        "{} · {worktree_name} working state",
        branch.unwrap_or("detached")
    );
    items.push(ContextMenuItem::Separator);
    for (slot, slot_label) in [
        (gitcomet_state::model::ComparisonSlot::A, "A"),
        (gitcomet_state::model::ComparisonSlot::B, "B"),
    ] {
        items.push(ContextMenuItem::Entry {
            label: format!("Set worktree working state as comparison {slot_label}").into(),
            icon: Some("icons/folder.svg".into()),
            shortcut: None,
            disabled: false,
            action: Box::new(ContextMenuAction::SetComparisonSlot {
                repo_id,
                slot,
                endpoint: gitcomet_state::model::ComparisonMark::worktree_dirty(
                    path.to_path_buf(),
                    dirty_label.clone(),
                ),
            }),
        });
    }
    items.push(ContextMenuItem::Separator);
    items.push(ContextMenuItem::Entry {
        label: "Remove…".into(),
        icon: Some("icons/trash.svg".into()),
        shortcut: None,
        disabled: false,
        action: Box::new(ContextMenuAction::OpenPopover {
            kind: PopoverKind::worktree(
                repo_id,
                WorktreePopoverKind::RemoveConfirm {
                    path: path.to_path_buf(),
                    branch: branch.map(ToOwned::to_owned),
                },
            ),
        }),
    });

    ContextMenuModel::new(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_includes_open_in_new_tab() {
        let repo_id = RepoId(1);
        let path = std::path::PathBuf::from("/tmp/worktree");
        let model = model(repo_id, &path, None, None);

        let open_action = model
            .items
            .iter()
            .find_map(|item| match item {
                ContextMenuItem::Entry { label, action, .. }
                    if label.as_ref() == "Open in new tab" =>
                {
                    Some((**action).clone())
                }
                _ => None,
            })
            .expect("expected Open in new tab entry");

        assert!(matches!(
            open_action,
            ContextMenuAction::OpenRepo { path: open_path } if open_path == path
        ));
    }

    #[test]
    fn model_routes_remove_through_branch_aware_confirm_when_branch_is_provided() {
        let repo_id = RepoId(1);
        let path = std::path::PathBuf::from("/tmp/worktree");
        let model = model(repo_id, &path, Some("feature/workspace"), None);

        let remove_action = model
            .items
            .iter()
            .find_map(|item| match item {
                ContextMenuItem::Entry { label, action, .. } if label.as_ref() == "Remove…" => {
                    Some((**action).clone())
                }
                _ => None,
            })
            .expect("expected Remove entry");

        assert!(matches!(
            remove_action,
            ContextMenuAction::OpenPopover {
                kind: PopoverKind::Repo {
                    repo_id: rid,
                    kind: RepoPopoverKind::Worktree(WorktreePopoverKind::RemoveConfirm {
                        path: remove_path,
                        branch: Some(branch),
                    }),
                },
            } if rid == repo_id && remove_path == path && branch == "feature/workspace"
        ));
    }

    #[test]
    fn model_can_put_the_worktree_head_in_either_comparison_slot() {
        let repo_id = RepoId(1);
        let path = std::path::PathBuf::from("/tmp/worktrees/payments");
        let head = CommitId("0123456789abcdef".into());
        let model = model(repo_id, &path, Some("feature/payments"), Some(&head));

        let actions = model
            .items
            .iter()
            .filter_map(|item| match item {
                ContextMenuItem::Entry { action, .. } => match &**action {
                    ContextMenuAction::SetComparisonSlot { slot, endpoint, .. } => {
                        Some((*slot, endpoint.clone()))
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(actions.len(), 4);
        assert_eq!(actions[0].0, gitcomet_state::model::ComparisonSlot::A);
        assert_eq!(actions[1].0, gitcomet_state::model::ComparisonSlot::B);
        assert!(
            actions
                .iter()
                .take(2)
                .all(|(_, endpoint)| endpoint.commit_id() == Some(&head))
        );
        assert!(
            actions
                .iter()
                .take(2)
                .all(|(_, endpoint)| endpoint.label == "feature/payments · payments")
        );
        assert!(actions.iter().skip(2).all(|(_, endpoint)| matches!(
            &endpoint.endpoint,
            gitcomet_state::model::ComparisonEndpoint::WorktreeDirty { path: dirty_path }
                if dirty_path == &path
        )));
    }

    #[test]
    fn model_does_not_offer_comparison_for_an_unborn_worktree() {
        let model = model(
            RepoId(1),
            std::path::Path::new("/tmp/worktrees/empty"),
            None,
            None,
        );

        assert!(!model.items.iter().any(|item| matches!(
            item,
            ContextMenuItem::Entry { action, .. }
                if matches!(&**action, ContextMenuAction::SetComparisonSlot { endpoint, .. }
                    if endpoint.commit_id().is_some())
        )));
    }
}
