use super::*;

pub(super) fn panel(
    this: &mut PopoverHost,
    draft: &crate::view::local_review_ui::LocalReviewCommentDraft,
    cx: &mut gpui::Context<PopoverHost>,
) -> gpui::Div {
    let theme = this.theme;
    let scaled_px = super::popover_scaled_px_fn(cx);
    let can_submit = this
        .local_review_comment_input
        .read_with(cx, |input, _| !input.text().trim().is_empty());
    let side = match draft.side {
        gitcomet_state::local_review::ReviewSide::Old => "old",
        gitcomet_state::local_review::ReviewSide::New => "new",
    };
    let line = match draft.side {
        gitcomet_state::local_review::ReviewSide::Old => draft.old_line,
        gitcomet_state::local_review::ReviewSide::New => draft.new_line,
    }
    .unwrap_or_default();

    div()
        .flex()
        .flex_col()
        .w(scaled_px(480.0))
        .child(popover_title("Add local review comment"))
        .child(div().border_t_1().border_color(theme.colors.stroke.default))
        .child(
            div()
                .px_2()
                .pt_2()
                .text_xs()
                .text_color(theme.colors.foreground.secondary)
                .child(format!(
                    "{} · {side} line {line} · {}",
                    draft.path.display(),
                    draft.title
                )),
        )
        .child(
            div()
                .px_2()
                .py_2()
                .w_full()
                .min_w(px(0.0))
                .child(this.local_review_comment_input.clone()),
        )
        .child(
            div()
                .px_2()
                .pb_2()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    cancel_button(
                        "local_review_comment_cancel",
                        "local_review_comment_cancel_hint",
                        theme,
                    )
                    .focus_handle(this.local_review_comment_focus.cancel.clone())
                    .on_click(theme, cx, |this, _event, window, cx| {
                        this.dismiss_prompt_popover(window, cx);
                    }),
                )
                .child(
                    components::Button::new("local_review_comment_add", "Add comment")
                        .focus_handle(this.local_review_comment_focus.submit.clone())
                        .style(components::ButtonStyle::Filled)
                        .disabled(!can_submit)
                        .on_click(theme, cx, |this, _event, _window, cx| {
                            this.submit_local_review_comment(cx);
                        }),
                ),
        )
}
