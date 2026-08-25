use super::*;
#[cfg(test)]
use gitcomet_state::model::ComparisonShelf;
use gitcomet_state::model::{ComparisonMark, ComparisonSlot, NamedComparison};

#[derive(Clone, Debug, Eq, PartialEq)]
struct ComparisonShelfPresentation {
    repo_id: RepoId,
    a: Option<ComparisonMark>,
    b: Option<ComparisonMark>,
    named: Vec<NamedComparison>,
    selected_name: Option<String>,
}

impl ComparisonShelfPresentation {
    fn from_repo(repo: &RepoState) -> Self {
        let shelf = &repo.comparison_shelf;
        Self {
            repo_id: repo.id,
            a: shelf.a.clone(),
            b: shelf.b.clone(),
            named: shelf.named.clone(),
            selected_name: shelf.selected_name.clone(),
        }
    }

    fn can_open(&self) -> bool {
        matches!((&self.a, &self.b), (Some(a), Some(b)) if a.commit_id != b.commit_id)
    }

    fn automatic_name(&self) -> Option<String> {
        let (Some(a), Some(b)) = (&self.a, &self.b) else {
            return None;
        };
        Some(format!("{} → {}", a.label, b.label))
    }
}

fn endpoint_caption(slot: ComparisonSlot, endpoint: Option<&ComparisonMark>) -> String {
    let slot = match slot {
        ComparisonSlot::A => "A",
        ComparisonSlot::B => "B",
    };
    match endpoint {
        Some(endpoint) => format!("{slot}: {}", endpoint.label),
        None => format!("{slot}: choose a commit"),
    }
}

impl GitCometView {
    fn comparison_endpoint_control(
        &mut self,
        theme: AppTheme,
        repo_id: RepoId,
        slot: ComparisonSlot,
        endpoint: Option<ComparisonMark>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = ui_scale::UiScale::from_percent(self.ui_scale_percent);
        let slot_key = match slot {
            ComparisonSlot::A => "a",
            ComparisonSlot::B => "b",
        };
        let caption: SharedString = endpoint_caption(slot, endpoint.as_ref()).into();
        let full_caption = endpoint
            .as_ref()
            .map(|endpoint| format!("{} ({})", endpoint.label, endpoint.commit_id.as_ref()))
            .unwrap_or_else(|| "Right-click a commit and choose Set as comparison".to_string());
        let has_endpoint = endpoint.is_some();
        let label = div()
            .id(format!("comparison_endpoint_{slot_key}"))
            .h(scale.px(26.0))
            .min_w(scale.px(126.0))
            .max_w(scale.px(250.0))
            .px(scale.px(8.0))
            .flex()
            .items_center()
            .overflow_hidden()
            .whitespace_nowrap()
            .rounded(scale.px(theme.radii.control))
            .border_1()
            .border_color(if has_endpoint {
                theme.colors.interaction.selected_indicator
            } else {
                theme.colors.stroke.control
            })
            .bg(theme.colors.surface.panel)
            .text_sm()
            .text_color(if has_endpoint {
                theme.colors.foreground.primary
            } else {
                theme.colors.foreground.secondary
            })
            .child(caption)
            .gitcomet_tooltip(theme, full_caption.into());

        div()
            .flex()
            .items_center()
            .gap(scale.px(2.0))
            .child(label)
            .when(has_endpoint, |row| {
                row.child(
                    components::Button::new(format!("comparison_clear_{slot_key}"), "×")
                        .style(components::ButtonStyle::Transparent)
                        .borderless()
                        .no_focus()
                        .on_click(theme, cx, move |this, _event, _window, _cx| {
                            this.store
                                .dispatch(Msg::ClearComparisonSlot { repo_id, slot });
                        })
                        .gitcomet_tooltip(theme, format!("Clear comparison {slot_key}").into()),
                )
            })
            .into_any_element()
    }

    pub(super) fn comparison_shelf_bar(
        &mut self,
        theme: AppTheme,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let Some(presentation) = self
            .active_repo()
            .map(ComparisonShelfPresentation::from_repo)
        else {
            return div().into_any_element();
        };
        let repo_id = presentation.repo_id;
        let can_open = presentation.can_open();
        let can_save = can_open;
        let scale = ui_scale::UiScale::from_percent(self.ui_scale_percent);

        let mut bar = div()
            .id("comparison_shelf")
            .debug_selector(|| "comparison_shelf".to_string())
            .w_full()
            .flex()
            .flex_col()
            .gap(scale.px(4.0))
            .px(scale.px(8.0))
            .py(scale.px(5.0))
            .border_b_1()
            .border_color(theme.colors.stroke.subtle)
            .bg(theme.colors.surface.chrome)
            .child(
                div()
                    .id("comparison_shelf_controls_scroll")
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(scale.px(6.0))
                    .overflow_x_scroll()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.colors.foreground.secondary)
                            .child("COMPARE"),
                    )
                    .child(self.comparison_endpoint_control(
                        theme,
                        repo_id,
                        ComparisonSlot::A,
                        presentation.a.clone(),
                        cx,
                    ))
                    .child(
                        components::Button::new("comparison_swap", "⇄")
                            .style(components::ButtonStyle::Transparent)
                            .borderless()
                            .no_focus()
                            .disabled(presentation.a.is_none() && presentation.b.is_none())
                            .on_click(theme, cx, move |this, _event, _window, _cx| {
                                this.store.dispatch(Msg::SwapComparisonSlots { repo_id });
                            })
                            .gitcomet_tooltip(theme, "Swap A and B".into()),
                    )
                    .child(self.comparison_endpoint_control(
                        theme,
                        repo_id,
                        ComparisonSlot::B,
                        presentation.b.clone(),
                        cx,
                    ))
                    .child(
                        components::Button::new("comparison_open", "Open diff")
                            .style(components::ButtonStyle::Filled)
                            .no_focus()
                            .disabled(!can_open)
                            .on_click(theme, cx, {
                                let a = presentation.a.clone();
                                let b = presentation.b.clone();
                                move |this, _event, _window, _cx| {
                                    let (Some(a), Some(b)) = (a.clone(), b.clone()) else {
                                        return;
                                    };
                                    this.store.dispatch(Msg::CompareCommitRange {
                                        repo_id,
                                        from: a.commit_id,
                                        to: b.commit_id,
                                        from_label: a.label,
                                        to_label: b.label,
                                    });
                                }
                            }),
                    )
                    .child(
                        components::Button::new("comparison_save", "Save pair")
                            .style(components::ButtonStyle::Outlined)
                            .no_focus()
                            .disabled(!can_save)
                            .on_click(theme, cx, {
                                let a = presentation.a.clone();
                                let b = presentation.b.clone();
                                let name = presentation.automatic_name();
                                move |this, _event, _window, _cx| {
                                    let (Some(a), Some(b), Some(name)) =
                                        (a.clone(), b.clone(), name.clone())
                                    else {
                                        return;
                                    };
                                    this.store.dispatch(Msg::AddNamedComparison {
                                        repo_id,
                                        name,
                                        a,
                                        b,
                                    });
                                }
                            }),
                    )
                    .child(
                        components::Button::new("comparison_clear", "Clear")
                            .style(components::ButtonStyle::Transparent)
                            .no_focus()
                            .disabled(presentation.a.is_none() && presentation.b.is_none())
                            .on_click(theme, cx, move |this, _event, _window, _cx| {
                                this.store.dispatch(Msg::ClearComparisonSlot {
                                    repo_id,
                                    slot: ComparisonSlot::A,
                                });
                                this.store.dispatch(Msg::ClearComparisonSlot {
                                    repo_id,
                                    slot: ComparisonSlot::B,
                                });
                            }),
                    ),
            );

        if !presentation.named.is_empty() {
            let mut saved = div()
                .id("comparison_shelf_saved_scroll")
                .w_full()
                .max_h(scale.px(30.0))
                .flex()
                .items_center()
                .overflow_x_scroll()
                .gap(scale.px(4.0))
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(theme.colors.foreground.secondary)
                        .child("Saved:"),
                );
            for (index, pair) in presentation.named.into_iter().enumerate() {
                let name = pair.name.clone();
                let selected = presentation.selected_name.as_deref() == Some(name.as_str());
                saved = saved.child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(scale.px(1.0))
                        .child(
                            components::Button::new(format!("comparison_named_{index}"), pair.name)
                                .style(components::ButtonStyle::Subtle)
                                .selected(selected)
                                .no_focus()
                                .on_click(theme, cx, {
                                    let name = name.clone();
                                    move |this, _event, _window, _cx| {
                                        this.store.dispatch(Msg::SelectNamedComparison {
                                            repo_id,
                                            name: name.clone(),
                                        });
                                    }
                                }),
                        )
                        .child(
                            components::Button::new(format!("comparison_remove_{index}"), "×")
                                .style(components::ButtonStyle::Transparent)
                                .borderless()
                                .no_focus()
                                .on_click(theme, cx, move |this, _event, _window, _cx| {
                                    this.store.dispatch(Msg::RemoveNamedComparison {
                                        repo_id,
                                        name: name.clone(),
                                    });
                                }),
                        ),
                );
            }
            bar = bar.child(saved);
        }

        bar.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitcomet_core::domain::RepoSpec;

    fn endpoint(id: &str, label: &str) -> ComparisonMark {
        ComparisonMark {
            commit_id: CommitId(id.to_string().into()),
            label: label.to_string(),
        }
    }

    fn repo_with_shelf(shelf: ComparisonShelf) -> RepoState {
        let mut repo = RepoState::new_opening(
            RepoId(7),
            RepoSpec {
                workdir: "/tmp/comparison-shelf-presentation".into(),
            },
        );
        repo.comparison_shelf = shelf;
        repo
    }

    #[test]
    fn open_requires_two_distinct_endpoints() {
        let a = endpoint("aaaa", "main");
        let mut presentation =
            ComparisonShelfPresentation::from_repo(&repo_with_shelf(ComparisonShelf {
                a: Some(a.clone()),
                ..Default::default()
            }));
        assert!(!presentation.can_open());

        presentation.b = Some(a);
        assert!(!presentation.can_open());
        presentation.b = Some(endpoint("bbbb", "feature"));
        assert!(presentation.can_open());
        assert_eq!(
            presentation.automatic_name().as_deref(),
            Some("main → feature")
        );
    }

    #[test]
    fn presentation_preserves_named_selection_and_endpoint_labels() {
        let a = endpoint("aaaa", "main");
        let b = endpoint("bbbb", "feature");
        let presentation =
            ComparisonShelfPresentation::from_repo(&repo_with_shelf(ComparisonShelf {
                a: Some(a.clone()),
                b: Some(b.clone()),
                named: vec![NamedComparison {
                    name: "review".to_string(),
                    a,
                    b,
                }],
                selected_name: Some("review".to_string()),
            }));

        assert_eq!(presentation.named.len(), 1);
        assert_eq!(presentation.selected_name.as_deref(), Some("review"));
        assert_eq!(
            endpoint_caption(ComparisonSlot::A, presentation.a.as_ref()),
            "A: main"
        );
        assert_eq!(
            endpoint_caption(ComparisonSlot::B, None),
            "B: choose a commit"
        );
    }
}
