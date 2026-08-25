use super::*;
use gitcomet_state::model::{ComparisonMark, ComparisonSlot};
use std::hash::Hash;
use std::rc::Rc;

const LOCAL_SECTION: &str = "Local branches";
const REMOTE_SECTION: &str = "Remote branches";
const TAG_SECTION: &str = "Tags";
const REMOTE_TAG_SECTION: &str = "Remote tags";
const WORKTREE_SECTION: &str = "Worktree HEADs";
const DIRTY_WORKTREE_SECTION: &str = "Live worktree states";
const COMMIT_SECTION: &str = "Loaded commits";

pub(super) const LIST_MAX_HEIGHT_PX: f32 = 420.0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ComparisonEndpoint {
    pub(super) repo_id: RepoId,
    pub(super) slot: ComparisonSlot,
    pub(super) mark: ComparisonMark,
}

fn short_sha(id: &CommitId) -> &str {
    id.as_ref().get(..8).unwrap_or(id.as_ref())
}

fn endpoint_item(
    title: impl Into<SharedString>,
    id: &CommitId,
    detail: impl Into<SharedString>,
    section: &'static str,
    icon: &'static str,
) -> components::PickerPromptItem {
    let detail: SharedString = detail.into();
    // Keep the complete object id searchable. The compact primary line only
    // shows eight characters, but pasting a full SHA must still find the row.
    let searchable_detail = format!("{}  •  {}", id.as_ref(), detail.as_ref());
    components::PickerPromptItem::from_parts([
        components::PickerPromptItemPart::new(title)
            .profile(components::TextTruncationProfile::End),
        components::PickerPromptItemPart::separator("  "),
        components::PickerPromptItemPart::new(short_sha(id).to_string())
            .flexible(false)
            .profile(components::TextTruncationProfile::End),
    ])
    .secondary_parts([components::PickerPromptItemPart::new(searchable_detail)
        .profile(components::TextTruncationProfile::Path)])
    .section(section)
    .icon(icon)
}

fn push_endpoint(
    items: &mut Vec<components::PickerPromptItem>,
    payloads: &mut Vec<ComparisonEndpoint>,
    repo_id: RepoId,
    slot: ComparisonSlot,
    item: components::PickerPromptItem,
    commit_id: CommitId,
    label: String,
) {
    items.push(item);
    payloads.push(ComparisonEndpoint {
        repo_id,
        slot,
        mark: ComparisonMark::commit(commit_id, label),
    });
}

fn rows(
    repo: &RepoState,
    slot: ComparisonSlot,
) -> (
    Vec<components::PickerPromptItem>,
    Vec<ComparisonEndpoint>,
    Option<usize>,
) {
    let mut items = Vec::new();
    let mut payloads = Vec::new();

    if let Loadable::Ready(branches) = &repo.branches {
        for branch in branches.iter() {
            push_endpoint(
                &mut items,
                &mut payloads,
                repo.id,
                slot,
                endpoint_item(
                    branch.name.clone(),
                    &branch.target,
                    "local branch",
                    LOCAL_SECTION,
                    "icons/git_branch.svg",
                ),
                branch.target.clone(),
                branch.name.clone(),
            );
        }
    }

    if let Loadable::Ready(branches) = &repo.remote_branches {
        for branch in branches.iter().filter(|branch| branch.name != "HEAD") {
            let label = format!("{}/{}", branch.remote, branch.name);
            push_endpoint(
                &mut items,
                &mut payloads,
                repo.id,
                slot,
                endpoint_item(
                    label.clone(),
                    &branch.target,
                    "remote-tracking branch",
                    REMOTE_SECTION,
                    "icons/cloud.svg",
                ),
                branch.target.clone(),
                label,
            );
        }
    }

    if let Loadable::Ready(tags) = &repo.tags {
        for tag in tags.iter() {
            push_endpoint(
                &mut items,
                &mut payloads,
                repo.id,
                slot,
                endpoint_item(
                    tag.name.clone(),
                    &tag.target,
                    "tag",
                    TAG_SECTION,
                    "icons/tag.svg",
                ),
                tag.target.clone(),
                tag.name.clone(),
            );
        }
    }

    if let Loadable::Ready(tags) = &repo.remote_tags {
        for tag in tags.iter() {
            let label = format!("{}/{}", tag.remote, tag.name);
            push_endpoint(
                &mut items,
                &mut payloads,
                repo.id,
                slot,
                endpoint_item(
                    label.clone(),
                    &tag.target,
                    "remote tag",
                    REMOTE_TAG_SECTION,
                    "icons/tag.svg",
                ),
                tag.target.clone(),
                label,
            );
        }
    }

    if let Loadable::Ready(worktrees) = &repo.worktrees {
        for worktree in worktrees.iter() {
            let Some(head) = &worktree.head else { continue };
            let path = worktree.path.display().to_string();
            let label = worktree
                .branch
                .as_ref()
                .map(|branch| {
                    format!(
                        "{branch} @ {}",
                        worktree
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    )
                })
                .unwrap_or_else(|| {
                    format!(
                        "{} @ {}",
                        short_sha(head),
                        worktree
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    )
                });
            push_endpoint(
                &mut items,
                &mut payloads,
                repo.id,
                slot,
                endpoint_item(
                    label.clone(),
                    head,
                    path,
                    WORKTREE_SECTION,
                    "icons/folder.svg",
                ),
                head.clone(),
                label,
            );

            let is_dirty = matches!(&repo.worktree_dirty, Loadable::Ready(summaries)
                if summaries.iter().any(|summary| summary.path == worktree.path));
            if is_dirty {
                let dirty_label = format!(
                    "{} · working state",
                    worktree.branch.as_deref().unwrap_or("detached")
                );
                items.push(
                    components::PickerPromptItem::from_parts([
                        components::PickerPromptItemPart::new(dirty_label.clone())
                            .profile(components::TextTruncationProfile::End),
                    ])
                    .secondary_parts([components::PickerPromptItemPart::new(
                        worktree.path.display().to_string(),
                    )
                    .profile(components::TextTruncationProfile::Path)])
                    .section(DIRTY_WORKTREE_SECTION)
                    .icon("icons/folder.svg"),
                );
                payloads.push(ComparisonEndpoint {
                    repo_id: repo.id,
                    slot,
                    mark: ComparisonMark::worktree_dirty(worktree.path.clone(), dirty_label),
                });
            }
        }
    }

    if let Loadable::Ready(page) = &repo.log {
        for commit in &page.commits {
            let label = short_sha(&commit.id).to_string();
            push_endpoint(
                &mut items,
                &mut payloads,
                repo.id,
                slot,
                endpoint_item(
                    label.clone(),
                    &commit.id,
                    commit.summary.to_string(),
                    COMMIT_SECTION,
                    "icons/git_branch.svg",
                ),
                commit.id.clone(),
                label,
            );
        }
    }

    let selected = match slot {
        ComparisonSlot::A => repo.comparison_shelf.a.as_ref(),
        ComparisonSlot::B => repo.comparison_shelf.b.as_ref(),
    };
    let marked_index = selected.and_then(|selected| {
        payloads
            .iter()
            .position(|endpoint| endpoint.mark.endpoint == selected.endpoint)
    });
    (items, payloads, marked_index)
}

fn picker_state(this: &PopoverHost) -> Option<(&RepoState, ComparisonSlot)> {
    let Some(PopoverKind::ComparisonEndpointPicker { repo_id, slot }) = this.popover else {
        return None;
    };
    let repo = this.state.repos.iter().find(|repo| repo.id == repo_id)?;
    Some((repo, slot))
}

fn rows_signature(this: &PopoverHost) -> u64 {
    super::rows_cache::signature(|hasher| {
        let Some((repo, slot)) = picker_state(this) else {
            return;
        };
        repo.id.hash(hasher);
        (match slot {
            ComparisonSlot::A => 0u8,
            ComparisonSlot::B => 1u8,
        })
        .hash(hasher);
        repo.branches_rev.hash(hasher);
        repo.remote_branches_rev.hash(hasher);
        repo.tags_rev.hash(hasher);
        repo.remote_tags_rev.hash(hasher);
        repo.worktrees_rev.hash(hasher);
        repo.worktree_dirty_rev.hash(hasher);
        repo.log_rev.hash(hasher);
        repo.comparison_shelf
            .a
            .as_ref()
            .map(|mark| &mark.endpoint)
            .hash(hasher);
        repo.comparison_shelf
            .b
            .as_ref()
            .map(|mark| &mark.endpoint)
            .hash(hasher);
    })
}

pub(super) fn cached(
    this: &PopoverHost,
    query: &str,
) -> Rc<super::rows_cache::CachedRows<ComparisonEndpoint>> {
    let key = super::rows_cache::RowsCacheKey::new(
        super::rows_cache::RowsCacheOwner::ComparisonEndpoint,
        rows_signature(this),
        query,
    );
    super::rows_cache::get_or_build(&this.comparison_endpoint_rows_cache, key, |_now| {
        let Some((repo, slot)) = picker_state(this) else {
            return (Vec::new(), Vec::new(), None);
        };
        rows(repo, slot)
    })
}

pub(super) fn nav_targets(this: &PopoverHost, query: &str) -> Vec<ComparisonEndpoint> {
    cached(this, query).filtered_payloads()
}

pub(super) fn activate(
    this: &mut PopoverHost,
    endpoint: ComparisonEndpoint,
    cx: &mut gpui::Context<PopoverHost>,
) {
    this.store.dispatch(Msg::SetComparisonSlot {
        repo_id: endpoint.repo_id,
        slot: endpoint.slot,
        endpoint: endpoint.mark,
    });
    this.close_popover(cx);
}

pub(super) fn panel(
    this: &mut PopoverHost,
    _repo_id: RepoId,
    slot: ComparisonSlot,
    cx: &mut gpui::Context<PopoverHost>,
) -> gpui::Div {
    let theme = this.theme;
    let ui_scale = super::popover_ui_scale(cx);
    let ui_scale_percent = ui_scale.percent();
    let scaled_px = |value: f32| super::popover_scaled_px_from_percent(value, ui_scale_percent);
    let Some(search) = this.comparison_endpoint_search_input.clone() else {
        return components::context_menu_label(
            theme,
            ui_scale_percent,
            "Search input not initialized",
            Some(this.tooltip_host.clone()),
            cx,
        );
    };
    let query = search.read(cx).text().trim().to_string();
    let built = cached(this, &query);
    let payloads = Rc::clone(&built.payloads);
    let slot_name = match slot {
        ComparisonSlot::A => "A",
        ComparisonSlot::B => "B",
    };
    let content = div()
        .child(popover_title(format!("Choose comparison {slot_name}")))
        .child(div().border_t_1().border_color(theme.colors.stroke.default))
        .child(
            components::PickerPrompt::new(search, this.picker_prompt_scroll.clone())
                .prebuilt_items(Rc::clone(&built.items), Rc::clone(&built.layout))
                .tooltip_host(this.tooltip_host.clone())
                .empty_text("No loaded commits or refs")
                .max_height(scaled_px(LIST_MAX_HEIGHT_PX))
                .selected_index(this.comparison_endpoint_selected_index)
                .marked_index(built.marked_index)
                .render(
                    theme,
                    ui_scale_percent,
                    cx,
                    move |this, ix, _event, _window, cx| {
                        let Some(endpoint) = payloads.get(ix).cloned() else {
                            return;
                        };
                        activate(this, endpoint, cx);
                    },
                ),
        );
    components::context_menu(theme, content).w(super::LARGE_PICKER_WIDTH.preferred_px(ui_scale))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitcomet_core::domain::{
        Branch, Commit, CommitParentIds, LogPage, RemoteBranch, RemoteTag, RepoSpec, Tag, Worktree,
        WorktreeDirtySummary,
    };
    use std::sync::Arc;
    use std::time::SystemTime;

    fn id(value: &str) -> CommitId {
        CommitId(value.to_string().into())
    }

    #[test]
    fn rows_include_refs_worktrees_and_loaded_commits_with_stable_targets() {
        let mut repo = RepoState::new_opening(
            RepoId(9),
            RepoSpec {
                workdir: "/tmp/picker".into(),
            },
        );
        repo.branches = Loadable::Ready(Arc::new(vec![Branch {
            name: "main".into(),
            target: id("11111111aaaaaaaa"),
            upstream: None,
            divergence: None,
        }]));
        repo.tags = Loadable::Ready(Arc::new(vec![Tag {
            name: "v1.0".into(),
            target: id("22222222bbbbbbbb"),
        }]));
        repo.remote_branches = Loadable::Ready(Arc::new(vec![RemoteBranch {
            remote: "origin".into(),
            name: "develop".into(),
            target: id("55555555eeeeeeee"),
        }]));
        repo.remote_tags = Loadable::Ready(Arc::new(vec![RemoteTag {
            remote: "origin".into(),
            name: "v1.1".into(),
            target: id("66666666ffffffff"),
        }]));
        repo.worktrees = Loadable::Ready(Arc::new(vec![Worktree {
            path: "/tmp/picker-agent".into(),
            head: Some(id("33333333cccccccc")),
            branch: Some("agent/one".into()),
            detached: false,
        }]));
        repo.worktree_dirty = Loadable::Ready(Arc::new(vec![WorktreeDirtySummary {
            path: "/tmp/picker-agent".into(),
            head: Some(id("33333333cccccccc")),
            branch: Some("agent/one".into()),
            detached: false,
            added: 0,
            modified: 1,
            deleted: 0,
            staged: Vec::new(),
            unstaged: Vec::new(),
        }]));
        repo.log = Loadable::Ready(Arc::new(LogPage {
            commits: vec![Commit {
                id: id("44444444dddddddd"),
                parent_ids: CommitParentIds::new(),
                summary: "fix picker".into(),
                author: "Ada".into(),
                time: SystemTime::UNIX_EPOCH,
            }],
            next_cursor: None,
        }));

        let (items, payloads, _) = rows(&repo, ComparisonSlot::B);
        assert_eq!(items.len(), 7);
        assert_eq!(
            payloads
                .iter()
                .map(|row| row.mark.label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "main",
                "origin/develop",
                "v1.0",
                "origin/v1.1",
                "agent/one @ picker-agent",
                "agent/one · working state",
                "44444444"
            ]
        );
        assert!(
            payloads
                .iter()
                .all(|row| row.slot == ComparisonSlot::B && row.repo_id == RepoId(9))
        );
        let full_sha_match = components::picker_prompt_layout(&items, "44444444dddddddd");
        assert_eq!(
            full_sha_match.item_indices,
            vec![6],
            "pasting a full SHA must find a commit whose primary row shows only the short SHA"
        );
        assert!(matches!(
            &payloads[5].mark.endpoint,
            gitcomet_state::model::ComparisonEndpoint::WorktreeDirty { path }
                if path == std::path::Path::new("/tmp/picker-agent")
        ));

        repo.comparison_shelf.b = Some(ComparisonMark::commit(id("11111111aaaaaaaa"), "main"));
        let (_, _, marked_index) = rows(&repo, ComparisonSlot::B);
        assert_eq!(
            marked_index,
            Some(0),
            "opening a picker must mark, not replace, the shelf's current endpoint"
        );
    }
}
