use super::*;
use gitcomet_state::local_review::{ReviewSide, ReviewStatus};

pub(super) fn panel(
    this: &mut PopoverHost,
    repo_id: RepoId,
    cx: &mut gpui::Context<PopoverHost>,
) -> gpui::Div {
    let theme = this.theme;
    let snapshot = this
        .state
        .repos
        .iter()
        .find(|repo| repo.id == repo_id)
        .map(|repo| repo.local_review.clone());

    let mut body = div().flex().flex_col().gap_2().px_2().py_2();
    match snapshot.as_ref().map(|snapshot| &snapshot.session) {
        Some(Loadable::Loading) => {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(theme.colors.foreground.secondary)
                    .child("Loading local review threads…"),
            );
        }
        Some(Loadable::Error(error)) => {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(theme.colors.status.danger.foreground)
                    .child(error.clone()),
            );
        }
        Some(Loadable::Ready(Some(session))) => {
            if session.comments.is_empty() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(theme.colors.foreground.secondary)
                        .child("No comments in this A/B review yet."),
                );
            } else {
                let mut comments = session.comments.clone();
                comments.sort_by_key(|comment| {
                    (
                        matches!(comment.status, ReviewStatus::Resolved),
                        comment.created_at_unix_ms,
                    )
                });
                for comment in comments {
                    let side = match comment.anchor.side {
                        Some(ReviewSide::Old) => "old",
                        Some(ReviewSide::New) => "new",
                        None => "file",
                    };
                    let line = comment
                        .anchor
                        .new_line
                        .or(comment.anchor.old_line)
                        .map(|line| format!(" line {line}"))
                        .unwrap_or_default();
                    let status = comment.status;
                    let comment_id = comment.id.clone();
                    let next_status = match status {
                        ReviewStatus::Open => ReviewStatus::Resolved,
                        ReviewStatus::Resolved => ReviewStatus::Open,
                    };
                    let action_label = match next_status {
                        ReviewStatus::Open => "Reopen",
                        ReviewStatus::Resolved => "Resolve",
                    };
                    body = body.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .p_2()
                            .border_1()
                            .border_color(theme.colors.stroke.default)
                            .rounded_md()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_xs()
                                    .text_color(theme.colors.foreground.secondary)
                                    .child(format!(
                                        "{} · {side}{line} · {} · {:?}",
                                        comment.anchor.path.display(),
                                        comment.author.name,
                                        status
                                    ))
                                    .child(
                                        components::Button::new(
                                            format!("local_review_status_{comment_id}"),
                                            action_label,
                                        )
                                        .style(components::ButtonStyle::Subtle)
                                        .on_click(
                                            theme,
                                            cx,
                                            move |this, _event, _window, cx| {
                                                this.set_local_review_comment_status(
                                                    repo_id,
                                                    comment_id.clone(),
                                                    next_status,
                                                    cx,
                                                );
                                            },
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.colors.foreground.primary)
                                    .child(comment.body),
                            ),
                    );
                }
            }
        }
        Some(Loadable::Ready(None)) | Some(Loadable::NotLoaded) | None => {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(theme.colors.foreground.secondary)
                    .child("No local review session exists for this A/B pair."),
            );
        }
    }

    let revision = snapshot.map_or(0, |snapshot| snapshot.store_revision);
    div()
        .flex()
        .flex_col()
        .w(px(520.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(popover_title(format!(
                    "Review threads · revision {revision}"
                )))
                .pr_2()
                .child(
                    components::Button::new("local_review_reload", "Reload")
                        .style(components::ButtonStyle::Subtle)
                        .on_click(theme, cx, move |this, _event, _window, _cx| {
                            this.reload_local_review_session(repo_id);
                        }),
                ),
        )
        .child(div().border_t_1().border_color(theme.colors.stroke.default))
        .child(
            body.id("local_review_threads_scroll")
                .max_h(px(520.0))
                .overflow_y_scroll(),
        )
}
