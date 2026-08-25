use super::*;

pub(super) fn model(host: &PopoverHost, repo_id: RepoId) -> ContextMenuModel {
    let repo = host.state.repos.iter().find(|repo| repo.id == repo_id);
    let current_scope = repo
        .map(|repo| repo.history_state.history_scope)
        .unwrap_or_default();
    let current_order = repo
        .map(|repo| repo.history_state.history_order)
        .unwrap_or_default();
    model_for_scope_and_order(repo_id, current_scope, current_order)
}

fn model_for_scope_and_order(
    repo_id: RepoId,
    current_scope: gitcomet_core::domain::LogScope,
    current_order: gitcomet_core::domain::HistoryOrder,
) -> ContextMenuModel {
    let mut items = vec![
        ContextMenuItem::Header("History mode".into()),
        ContextMenuItem::Separator,
    ];
    items.extend(
        crate::view::history_mode::history_mode_ui_specs()
            .iter()
            .map(|spec| ContextMenuItem::Entry {
                label: spec.label.into(),
                icon: (spec.mode == current_scope).then_some("icons/check.svg".into()),
                shortcut: Some(spec.shortcut.into()),
                disabled: false,
                action: Box::new(ContextMenuAction::SetHistoryScope {
                    repo_id,
                    scope: spec.mode,
                }),
            }),
    );
    items.extend([
        ContextMenuItem::Separator,
        ContextMenuItem::Header("Commit order".into()),
        ContextMenuItem::Separator,
    ]);
    for (order, label, shortcut) in [
        (
            gitcomet_core::domain::HistoryOrder::Date,
            "Date Order (Fast)",
            "Default; lightweight commit-time paging",
        ),
        (
            gitcomet_core::domain::HistoryOrder::Ancestor,
            "Ancestor Order (Topo)",
            "Topology-aware; shallow repositories use Date",
        ),
    ] {
        items.push(ContextMenuItem::Entry {
            label: label.into(),
            icon: (order == current_order).then_some("icons/check.svg".into()),
            shortcut: Some(shortcut.into()),
            disabled: false,
            action: Box::new(ContextMenuAction::SetHistoryOrder { repo_id, order }),
        });
    }
    ContextMenuModel::new(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_marks_current_history_mode() {
        let model = super::model_for_scope_and_order(
            RepoId(11),
            gitcomet_core::domain::LogScope::MergesOnly,
            gitcomet_core::domain::HistoryOrder::Ancestor,
        );

        assert!(model.items.iter().any(|item| {
            matches!(
                item,
                ContextMenuItem::Entry { label, icon, .. }
                    if label.as_ref() == "Merges only"
                        && icon
                            .as_ref()
                            .is_some_and(|icon| icon.as_ref() == "icons/check.svg")
            )
        }));
        assert!(model.items.iter().any(|item| matches!(
            item,
            ContextMenuItem::Entry { label, icon, .. }
                if label.as_ref() == "Ancestor Order (Topo)" && icon.is_some()
        )));
    }
}
