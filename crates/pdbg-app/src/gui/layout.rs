use super::*;

#[derive(Default)]
pub(crate) struct SearchControlsOutput {
    pub(crate) submit: bool,
    pub(crate) cancel: bool,
    pub(crate) clear: bool,
}

pub(crate) fn draw_search_controls(
    ui: &mut egui::Ui,
    query: &mut String,
    hint: &str,
    busy: bool,
) -> SearchControlsOutput {
    let mut output = SearchControlsOutput::default();
    let height = ui.spacing().interact_size.y;
    let spacing = ui.spacing().item_spacing.x;
    let action_label = if busy { "Cancel" } else { "Search" };
    let action_width = if busy { 70.0 } else { 66.0 };
    let clear_width = 56.0;
    let min_edit_width = 96.0;
    let available_width = ui.available_width();
    let inline_min_width = min_edit_width + action_width + clear_width + spacing * 2.0;

    if available_width >= inline_min_width {
        ui.horizontal(|ui| {
            let edit_width =
                (ui.available_width() - action_width - clear_width - spacing * 2.0).max(24.0);
            let response = ui
                .add_enabled_ui(!busy, |ui| {
                    ui.add_sized(
                        egui::vec2(edit_width, height),
                        TextEdit::singleline(query).hint_text(hint),
                    )
                })
                .inner;
            output.submit |=
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if busy {
                output.cancel |= ui
                    .add_sized(
                        egui::vec2(action_width, height),
                        egui::Button::new(action_label),
                    )
                    .clicked();
            } else {
                output.submit |= ui
                    .add_sized(
                        egui::vec2(action_width, height),
                        egui::Button::new(action_label),
                    )
                    .clicked();
            }
            output.clear |= ui
                .add_sized(egui::vec2(clear_width, height), egui::Button::new("Clear"))
                .clicked();
        });
    } else {
        let edit_width = ui.available_width().max(24.0);
        let response = ui
            .add_enabled_ui(!busy, |ui| {
                ui.add_sized(
                    egui::vec2(edit_width, height),
                    TextEdit::singleline(query).hint_text(hint),
                )
            })
            .inner;
        output.submit |=
            response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        ui.horizontal_wrapped(|ui| {
            if busy {
                output.cancel |= ui
                    .add_sized(
                        egui::vec2(action_width, height),
                        egui::Button::new(action_label),
                    )
                    .clicked();
            } else {
                output.submit |= ui
                    .add_sized(
                        egui::vec2(action_width, height),
                        egui::Button::new(action_label),
                    )
                    .clicked();
            }
            output.clear |= ui
                .add_sized(egui::vec2(clear_width, height), egui::Button::new("Clear"))
                .clicked();
        });
    }

    output
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PanelWidthSpec {
    pub(crate) min: f32,
    pub(crate) default: f32,
    pub(crate) max: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WorkspacePanelLayout {
    pub(crate) left: PanelWidthSpec,
    pub(crate) right: PanelWidthSpec,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WorkspaceRects {
    pub(crate) left: egui::Rect,
    pub(crate) left_splitter: egui::Rect,
    pub(crate) center: egui::Rect,
    pub(crate) right_splitter: egui::Rect,
    pub(crate) right: egui::Rect,
}

pub(crate) fn workspace_panel_layout(available_width: f32) -> WorkspacePanelLayout {
    let compact = available_width < COMPACT_LAYOUT_WIDTH;
    let left = if compact {
        PanelWidthSpec {
            min: COMPACT_LEFT_PANEL_MIN_WIDTH,
            default: COMPACT_LEFT_PANEL_DEFAULT_WIDTH,
            max: LEFT_PANEL_MAX_WIDTH,
        }
    } else {
        PanelWidthSpec {
            min: LEFT_PANEL_MIN_WIDTH,
            default: LEFT_PANEL_DEFAULT_WIDTH,
            max: LEFT_PANEL_MAX_WIDTH,
        }
    };
    let right = if compact {
        PanelWidthSpec {
            min: COMPACT_RIGHT_PANEL_MIN_WIDTH,
            default: COMPACT_RIGHT_PANEL_DEFAULT_WIDTH,
            max: RIGHT_PANEL_MAX_WIDTH,
        }
    } else {
        PanelWidthSpec {
            min: RIGHT_PANEL_MIN_WIDTH,
            default: RIGHT_PANEL_DEFAULT_WIDTH,
            max: RIGHT_PANEL_MAX_WIDTH,
        }
    };

    WorkspacePanelLayout { left, right }
}

pub(crate) fn workspace_min_center_width(total_width: f32) -> f32 {
    WORKSPACE_MIN_CENTER_WIDTH.min((total_width * 0.35).max(220.0))
}

pub(crate) fn clamp_workspace_widths(
    left_width: &mut f32,
    right_width: &mut f32,
    layout: WorkspacePanelLayout,
    total_width: f32,
) {
    *left_width = left_width.clamp(layout.left.min, layout.left.max);
    *right_width = right_width.clamp(layout.right.min, layout.right.max);

    let splitters = WORKSPACE_SPLITTER_WIDTH * 2.0;
    let side_budget = (total_width - workspace_min_center_width(total_width) - splitters).max(0.0);
    let side_width = *left_width + *right_width;
    if side_width <= side_budget || side_width <= f32::EPSILON {
        return;
    }

    let overflow = side_width - side_budget;
    let left_shrink_room = (*left_width - layout.left.min).max(0.0);
    let right_shrink_room = (*right_width - layout.right.min).max(0.0);
    let shrink_room = left_shrink_room + right_shrink_room;
    if shrink_room <= f32::EPSILON {
        return;
    }

    *left_width -= overflow * (left_shrink_room / shrink_room);
    *right_width -= overflow * (right_shrink_room / shrink_room);
}

pub(crate) fn workspace_rects(
    available_rect: egui::Rect,
    left_width: f32,
    right_width: f32,
) -> WorkspaceRects {
    let left = egui::Rect::from_min_max(
        available_rect.min,
        egui::pos2(
            (available_rect.left() + left_width).min(available_rect.right()),
            available_rect.bottom(),
        ),
    );
    let left_splitter = egui::Rect::from_min_max(
        egui::pos2(left.right(), available_rect.top()),
        egui::pos2(
            (left.right() + WORKSPACE_SPLITTER_WIDTH).min(available_rect.right()),
            available_rect.bottom(),
        ),
    );
    let right = egui::Rect::from_min_max(
        egui::pos2(
            (available_rect.right() - right_width).max(available_rect.left()),
            available_rect.top(),
        ),
        available_rect.max,
    );
    let right_splitter = egui::Rect::from_min_max(
        egui::pos2(
            (right.left() - WORKSPACE_SPLITTER_WIDTH).max(available_rect.left()),
            available_rect.top(),
        ),
        egui::pos2(right.left(), available_rect.bottom()),
    );
    let center = egui::Rect::from_min_max(
        egui::pos2(left_splitter.right(), available_rect.top()),
        egui::pos2(
            right_splitter.left().max(left_splitter.right()),
            available_rect.bottom(),
        ),
    );

    WorkspaceRects {
        left,
        left_splitter,
        center,
        right_splitter,
        right,
    }
}

pub(crate) fn show_framed_child<R>(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    frame: egui::Frame,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect);
    child.expand_to_include_rect(rect);
    frame
        .show(&mut child, |ui| {
            ui.set_min_size(ui.available_size());
            add_contents(ui)
        })
        .inner
}

pub(crate) fn draw_workspace_splitter(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    response: &egui::Response,
) {
    let fill = if response.dragged() {
        theme().accent
    } else if response.hovered() {
        theme().strong_border
    } else {
        theme().border
    };
    ui.painter().rect_filled(rect, 0.0, fill);
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
}

pub(crate) fn top_bar_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).size(12.0).color(if enabled {
            theme().top_bar_text
        } else {
            theme().top_bar_muted
        }))
        .fill(Color32::from_rgb(42, 54, 68)),
    )
}

pub(crate) fn top_bar_icon_button(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    hover_text: impl Into<String>,
) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).size(16.0).color(if enabled {
            theme().top_bar_text
        } else {
            theme().top_bar_muted
        }))
        .fill(Color32::from_rgb(42, 54, 68))
        .min_size(egui::vec2(30.0, 24.0)),
    )
    .on_hover_text(hover_text.into())
}

pub(crate) fn option_text(value: Option<&str>) -> &str {
    value.unwrap_or("-")
}
