use crate::AppState;
use eframe::egui::{
    self, Color32, FontDefinitions, FontFamily, FontId, RichText, ScrollArea, TextEdit, TextStyle,
};
use pdbg_core::{escape_pdf_text, DiagnosticSeverity, EgressFormat, EscapedText};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const VIRTUAL_TREE_ROWS: usize = 1_000_000;
const STREAM_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const HEX_WINDOW_BYTES: usize = 512;
const COPY_LIMIT_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Default)]
pub struct GuiRunOptions {
    pub smoke_exit_after: Option<Duration>,
}

pub fn run_gui_with_options(options: GuiRunOptions) -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("pdbg UI Shell Spike")
            .with_inner_size([1440.0, 900.0]),
        ..Default::default()
    };

    eframe::run_native(
        "pdbg UI Shell Spike",
        native_options,
        Box::new(move |cc| {
            configure_egui(&cc.egui_ctx);
            Ok(Box::new(GuiShellApp::new_with_options(options)))
        }),
    )
}

fn configure_egui(ctx: &egui::Context) {
    ctx.set_fonts(pdbg_fonts());
    ctx.set_global_style(pdbg_style());
}

fn pdbg_fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.families.insert(
        FontFamily::Name("pdbg-sans".into()),
        vec![
            "Ubuntu-Light".to_string(),
            "NotoEmoji-Regular".to_string(),
            "emoji-icon-font".to_string(),
        ],
    );
    fonts.families.insert(
        FontFamily::Name("pdbg-mono".into()),
        vec![
            "Hack".to_string(),
            "Ubuntu-Light".to_string(),
            "NotoEmoji-Regular".to_string(),
        ],
    );
    fonts
}

fn pdbg_style() -> egui::Style {
    let mut style = egui::Style::default();
    let sans = FontFamily::Name("pdbg-sans".into());
    let mono = FontFamily::Name("pdbg-mono".into());

    style.text_styles = BTreeMap::from([
        (TextStyle::Heading, FontId::new(16.0, sans.clone())),
        (TextStyle::Body, FontId::new(12.5, sans.clone())),
        (TextStyle::Button, FontId::new(12.0, sans.clone())),
        (TextStyle::Small, FontId::new(11.0, sans)),
        (TextStyle::Monospace, FontId::new(11.0, mono)),
    ]);
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 5.0);
    style.spacing.interact_size = egui::vec2(28.0, 24.0);
    style.spacing.window_margin = egui::Margin::same(10);

    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = PdbgTheme::PANEL;
    visuals.window_fill = PdbgTheme::SURFACE;
    visuals.faint_bg_color = PdbgTheme::CANVAS;
    visuals.extreme_bg_color = PdbgTheme::CODE_BG;
    visuals.text_edit_bg_color = Some(PdbgTheme::CODE_BG);
    visuals.code_bg_color = PdbgTheme::CODE_BG;
    visuals.hyperlink_color = PdbgTheme::ACCENT;
    visuals.warn_fg_color = PdbgTheme::WARN_FG;
    visuals.error_fg_color = PdbgTheme::ERROR_FG;
    visuals.selection.bg_fill = PdbgTheme::SELECTED_BG;
    visuals.selection.stroke = egui::Stroke::new(1.0, PdbgTheme::ACCENT);
    visuals.widgets.noninteractive.fg_stroke.color = PdbgTheme::TEXT;
    visuals.widgets.inactive.weak_bg_fill = PdbgTheme::CHIP_BG;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, PdbgTheme::BORDER);
    visuals.widgets.hovered.weak_bg_fill = PdbgTheme::SELECTED_BG;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, PdbgTheme::ACCENT);
    visuals.widgets.active.weak_bg_fill = PdbgTheme::SELECTED_BG;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.5, PdbgTheme::ACCENT);
    visuals.widgets.open.weak_bg_fill = PdbgTheme::SELECTED_BG;
    visuals.button_frame = true;
    visuals.striped = true;
    style.visuals = visuals;
    style
}

struct PdbgTheme;

impl PdbgTheme {
    const SURFACE: Color32 = Color32::from_rgb(251, 252, 253);
    const PANEL: Color32 = Color32::from_rgb(247, 249, 251);
    const CANVAS: Color32 = Color32::from_rgb(233, 237, 242);
    const PAGE: Color32 = Color32::from_rgb(255, 253, 248);
    const CODE_BG: Color32 = Color32::from_rgb(245, 247, 250);
    const CHIP_BG: Color32 = Color32::from_rgb(238, 243, 247);
    const SELECTED_BG: Color32 = Color32::from_rgb(232, 245, 246);
    const TEXT: Color32 = Color32::from_rgb(31, 41, 51);
    const MUTED: Color32 = Color32::from_rgb(104, 116, 131);
    const BORDER: Color32 = Color32::from_rgb(207, 215, 225);
    const ACCENT: Color32 = Color32::from_rgb(8, 127, 140);
    const OPERATOR: Color32 = Color32::from_rgb(215, 100, 53);
    const WARN_BG: Color32 = Color32::from_rgb(255, 244, 223);
    const WARN_FG: Color32 = Color32::from_rgb(184, 107, 0);
    const ERROR_BG: Color32 = Color32::from_rgb(255, 240, 238);
    const ERROR_FG: Color32 = Color32::from_rgb(180, 35, 24);

    fn severity_fg(severity: &DiagnosticSeverity) -> Color32 {
        match severity {
            DiagnosticSeverity::Info => Self::ACCENT,
            DiagnosticSeverity::Warning => Self::WARN_FG,
            DiagnosticSeverity::Error => Self::ERROR_FG,
        }
    }

    fn severity_bg(severity: &DiagnosticSeverity) -> Color32 {
        match severity {
            DiagnosticSeverity::Info => Self::SELECTED_BG,
            DiagnosticSeverity::Warning => Self::WARN_BG,
            DiagnosticSeverity::Error => Self::ERROR_BG,
        }
    }
}

pub struct GuiShellApp {
    state: Result<AppState, String>,
    launched_at: Instant,
    smoke_exit_after: Option<Duration>,
    tree: VirtualObjectTree,
    stream: LargeStreamModel,
    selected_row: usize,
    back_stack: Vec<usize>,
    forward_stack: Vec<usize>,
    selected_tab: InspectorTab,
    copied_excerpt: Option<EscapedText>,
    status_log: Vec<String>,
}

impl GuiShellApp {
    pub fn new() -> Self {
        Self::new_with_options(GuiRunOptions::default())
    }

    pub fn new_with_options(options: GuiRunOptions) -> Self {
        let state = AppState::new_headless().map_err(|err| err.message);
        Self {
            state,
            launched_at: Instant::now(),
            smoke_exit_after: options.smoke_exit_after,
            tree: VirtualObjectTree::new(VIRTUAL_TREE_ROWS),
            stream: LargeStreamModel::default(),
            selected_row: 0,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            selected_tab: InspectorTab::Object,
            copied_excerpt: None,
            status_log: vec![
                "fake shim opened fake.pdf".to_string(),
                "virtual object tree uses generated rows".to_string(),
                "large stream pane uses generated bytes".to_string(),
            ],
        }
    }

    fn summary_line(&self) -> String {
        match &self.state {
            Ok(state) => state
                .panels
                .summary
                .as_ref()
                .map(|summary| {
                    format!(
                        "{} | pages {} | xref {} | safe mode {}",
                        summary.file_path,
                        summary.page_count,
                        summary.xref_size,
                        summary.safety.safe_mode
                    )
                })
                .unwrap_or_else(|| "summary unavailable".to_string()),
            Err(err) => format!("failed to initialize fake shim: {err}"),
        }
    }

    fn selected_object_label(&self) -> String {
        self.tree.row_label(self.selected_row)
    }

    fn select_row_from_tree(&mut self, row: usize) {
        if self.selected_row == row {
            return;
        }
        self.selected_row = row;
        self.forward_stack.clear();
        self.status_log
            .push(format!("selected {}", self.tree.row_label(row)));
    }

    fn follow_reference(&mut self, row: usize) {
        if self.selected_row == row || row >= self.tree.row_count() {
            return;
        }
        self.back_stack.push(self.selected_row);
        self.forward_stack.clear();
        self.selected_row = row;
        self.selected_tab = InspectorTab::Object;
        self.status_log.push(format!(
            "resolved reference to {}",
            self.tree.row_label(row)
        ));
    }

    fn go_back(&mut self) {
        if let Some(row) = self.back_stack.pop() {
            self.forward_stack.push(self.selected_row);
            self.selected_row = row;
            self.status_log
                .push(format!("back to {}", self.tree.row_label(row)));
        }
    }

    fn go_forward(&mut self) {
        if let Some(row) = self.forward_stack.pop() {
            self.back_stack.push(self.selected_row);
            self.selected_row = row;
            self.status_log
                .push(format!("forward to {}", self.tree.row_label(row)));
        }
    }
}

impl eframe::App for GuiShellApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        egui::Panel::top("top_bar").show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("pdbg").strong());
                if ui
                    .add_enabled(!self.back_stack.is_empty(), egui::Button::new("Back"))
                    .clicked()
                {
                    self.go_back();
                }
                if ui
                    .add_enabled(!self.forward_stack.is_empty(), egui::Button::new("Forward"))
                    .clicked()
                {
                    self.go_forward();
                }
                ui.separator();
                ui.label(self.summary_line());
                ui.separator();
                ui.label(format!("scale {:.2}x", ctx.pixels_per_point()));
            });
        });

        egui::Panel::bottom("log_panel")
            .resizable(true)
            .default_size(150.0)
            .show_inside(ui, |ui| self.draw_log(ui));

        egui::Panel::left("document_tree")
            .resizable(true)
            .default_size(320.0)
            .size_range(220.0..=520.0)
            .show_inside(ui, |ui| self.draw_tree(ui));

        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(440.0)
            .size_range(320.0..=680.0)
            .show_inside(ui, |ui| self.draw_inspector(ui, &ctx));

        egui::CentralPanel::default().show_inside(ui, |ui| self.draw_page_preview(ui));

        if self
            .smoke_exit_after
            .is_some_and(|duration| self.launched_at.elapsed() >= duration)
        {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl GuiShellApp {
    fn draw_tree(&mut self, ui: &mut egui::Ui) {
        ui.heading("Document Tree");
        ui.horizontal(|ui| {
            ui.label("Rows");
            ui.monospace(self.tree.row_count().to_string());
        });

        let row_height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
        ScrollArea::vertical().show_rows(ui, row_height, self.tree.row_count(), |ui, range| {
            for row in range {
                let label = self.tree.row_label(row);
                let selected = row == self.selected_row;
                if ui.selectable_label(selected, label).clicked() {
                    self.select_row_from_tree(row);
                }
            }
        });
    }

    fn draw_page_preview(&mut self, ui: &mut egui::Ui) {
        ui.heading("Page Preview");
        ui.label("Fake page renderer surface");

        let available = ui.available_size();
        let desired = egui::vec2(available.x.max(320.0), available.y.max(360.0));
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, PdbgTheme::CANVAS);

        let margin = 28.0;
        let page_rect = egui::Rect::from_min_max(
            rect.min + egui::vec2(margin, margin),
            rect.max - egui::vec2(margin, margin),
        );
        painter.rect_filled(page_rect, 2.0, PdbgTheme::PAGE);
        painter.rect_stroke(
            page_rect,
            2.0,
            egui::Stroke::new(1.0, PdbgTheme::BORDER),
            egui::StrokeKind::Outside,
        );

        let content = page_rect.shrink(34.0);
        for index in 0..9 {
            let y = content.top() + index as f32 * 26.0;
            painter.line_segment(
                [
                    egui::pos2(content.left(), y),
                    egui::pos2(content.right() - (index % 3) as f32 * 54.0, y),
                ],
                egui::Stroke::new(3.0, PdbgTheme::MUTED),
            );
        }

        let highlight = egui::Rect::from_min_size(
            egui::pos2(content.left() + 36.0, content.top() + 190.0),
            egui::vec2(220.0, 86.0),
        );
        painter.rect_stroke(
            highlight,
            1.0,
            egui::Stroke::new(2.0, PdbgTheme::OPERATOR),
            egui::StrokeKind::Outside,
        );
        painter.text(
            highlight.left_top() + egui::vec2(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            self.selected_object_label(),
            egui::FontId::monospace(13.0),
            PdbgTheme::TEXT,
        );
    }

    fn draw_inspector(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Inspector");
        ui.horizontal(|ui| {
            self.tab_button(ui, InspectorTab::Object, "Object");
            self.tab_button(ui, InspectorTab::Stream, "Stream");
            self.tab_button(ui, InspectorTab::Diagnostics, "Diagnostics");
        });
        ui.separator();

        match self.selected_tab {
            InspectorTab::Object => self.draw_object_panel(ui),
            InspectorTab::Stream => self.draw_stream_panel(ui, ctx),
            InspectorTab::Diagnostics => self.draw_diagnostics_panel(ui),
        }
    }

    fn tab_button(&mut self, ui: &mut egui::Ui, tab: InspectorTab, label: &str) {
        if ui
            .selectable_label(self.selected_tab == tab, label)
            .on_hover_text(format!("{label} panel"))
            .clicked()
        {
            self.selected_tab = tab;
        }
    }

    fn draw_object_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(self.selected_object_label()).monospace());
        ui.separator();
        ui.label("Indirect references");
        ui.horizontal_wrapped(|ui| {
            for target in self.tree.reference_targets(self.selected_row) {
                if ui
                    .link(format!("{target} 0 R"))
                    .on_hover_text("Resolve reference and push navigation history")
                    .clicked()
                {
                    self.follow_reference(target);
                }
            }
        });
        ui.separator();
        if let Ok(state) = &self.state {
            if let Some(preview) = &state.panels.detail_preview {
                ui.label("Preview");
                ui.monospace(preview);
            }
            if let Some(summary) = &state.panels.summary {
                ui.separator();
                ui.label("Document summary");
                ui.monospace(format!("hash: {:?}", summary.file_hash));
                ui.monospace(format!("version: {:?}", summary.pdf_version));
                ui.monospace(format!(
                    "permissions: print={} copy={} modify={}",
                    summary.permissions.print, summary.permissions.copy, summary.permissions.modify
                ));
            }
        } else if let Err(err) = &self.state {
            ui.colored_label(Color32::from_rgb(190, 62, 62), err);
        }
    }

    fn draw_stream_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label("Offset");
            let offset_response = ui.add(
                egui::DragValue::new(&mut self.stream.offset)
                    .range(0..=STREAM_TOTAL_BYTES.saturating_sub(HEX_WINDOW_BYTES))
                    .speed(64),
            );
            if offset_response.changed() {
                self.stream.sync_hex_window();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Fallback byte range");
            ui.add(
                egui::DragValue::new(&mut self.stream.selection_offset)
                    .range(0..=STREAM_TOTAL_BYTES.saturating_sub(1))
                    .speed(64),
            );
            ui.add(
                egui::DragValue::new(&mut self.stream.selection_len)
                    .range(1..=32 * 1024)
                    .speed(16),
            );
        });
        ui.label(RichText::new("Hex view is read-only; drag-select text before copying.").small());

        self.stream.sync_hex_window();
        let output = TextEdit::multiline(&mut self.stream.hex_text)
            .font(egui::TextStyle::Monospace)
            .desired_rows(18)
            .code_editor()
            .show(ui);

        if let Some(cursor_range) = output.cursor_range.filter(|range| !range.is_empty()) {
            self.stream.selected_hex_text =
                Some(cursor_range.slice_str(&self.stream.hex_text).to_string());
        }
        if output.response.changed() {
            self.stream.reset_hex_window();
            self.status_log
                .push("ignored edit in read-only hex view".to_string());
        }

        if ui.button("Copy escaped excerpt").clicked() {
            let source = self.stream.copy_source_label();
            let escaped = self.stream.escaped_copy_text();
            ctx.copy_text(escaped.text.clone());
            self.status_log.push(format!(
                "copied bounded {source} excerpt{}",
                if escaped.truncated {
                    " (truncated)"
                } else {
                    ""
                }
            ));
            self.copied_excerpt = Some(escaped);
        }

        if let Some(escaped) = &self.copied_excerpt {
            ui.separator();
            ui.label(if escaped.truncated {
                "Last copied excerpt (truncated)"
            } else {
                "Last copied excerpt"
            });
            let mut copied = escaped.text.clone();
            ui.add(
                TextEdit::multiline(&mut copied)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(5)
                    .interactive(false),
            );
        }
    }

    fn draw_diagnostics_panel(&mut self, ui: &mut egui::Ui) {
        if let Ok(state) = &self.state {
            let diagnostics = state
                .panels
                .summary
                .as_ref()
                .map(|summary| summary.diagnostics.as_slice())
                .unwrap_or(&[]);
            if diagnostics.is_empty() {
                ui.label(RichText::new("No fake diagnostics").color(PdbgTheme::MUTED));
            } else {
                for diagnostic in diagnostics {
                    let color = PdbgTheme::severity_fg(&diagnostic.severity);
                    egui::Frame::new()
                        .fill(PdbgTheme::severity_bg(&diagnostic.severity))
                        .stroke(egui::Stroke::new(1.0, color))
                        .corner_radius(4)
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.colored_label(
                                color,
                                RichText::new(format!(
                                    "{} {} {}",
                                    diagnostic.severity.as_public_str(),
                                    diagnostic.code.as_public_str(),
                                    diagnostic.message
                                ))
                                .monospace(),
                            );
                        });
                }
            }
        }
    }

    fn draw_log(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Log");
            if ui.button("Clear").clicked() {
                self.status_log.clear();
            }
        });
        ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
            for line in &self.status_log {
                ui.monospace(line);
            }
        });
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InspectorTab {
    Object,
    Stream,
    Diagnostics,
}

#[derive(Clone, Debug)]
pub(crate) struct VirtualObjectTree {
    rows: usize,
}

impl VirtualObjectTree {
    pub(crate) fn new(rows: usize) -> Self {
        Self { rows }
    }

    pub(crate) fn row_count(&self) -> usize {
        self.rows + 1
    }

    pub(crate) fn row_label(&self, row: usize) -> String {
        if row == 0 {
            return "root / catalog".to_string();
        }
        format!(
            "obj {:06} 0 R  /FakeNode{}",
            row,
            row.wrapping_mul(31) % 997
        )
    }

    pub(crate) fn reference_targets(&self, row: usize) -> [usize; 3] {
        let base = row.max(1);
        [
            (base * 3 % self.rows).max(1),
            (base * 11 % self.rows).max(1),
            (base * 101 % self.rows).max(1),
        ]
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LargeStreamModel {
    offset: usize,
    selection_offset: usize,
    selection_len: usize,
    hex_text: String,
    hex_text_offset: usize,
    selected_hex_text: Option<String>,
}

impl Default for LargeStreamModel {
    fn default() -> Self {
        let mut model = Self {
            offset: 0,
            selection_offset: 0,
            selection_len: 256,
            hex_text: String::new(),
            hex_text_offset: usize::MAX,
            selected_hex_text: None,
        };
        model.sync_hex_window();
        model
    }
}

impl LargeStreamModel {
    pub(crate) fn hex_window(&self) -> String {
        self.hex_dump(self.offset, HEX_WINDOW_BYTES)
    }

    pub(crate) fn sync_hex_window(&mut self) {
        if self.hex_text_offset != self.offset {
            self.reset_hex_window();
            self.selected_hex_text = None;
        }
    }

    pub(crate) fn reset_hex_window(&mut self) {
        self.hex_text = self.hex_window();
        self.hex_text_offset = self.offset;
    }

    pub(crate) fn escaped_copy_text(&self) -> EscapedText {
        if let Some(selected) = self
            .selected_hex_text
            .as_ref()
            .filter(|selected| !selected.is_empty())
        {
            return escape_pdf_text(selected, EgressFormat::Markdown, COPY_LIMIT_BYTES);
        }
        self.escaped_range_selection()
    }

    pub(crate) fn copy_source_label(&self) -> &'static str {
        if self
            .selected_hex_text
            .as_ref()
            .is_some_and(|selected| !selected.is_empty())
        {
            "selected hex text"
        } else {
            "byte range"
        }
    }

    pub(crate) fn escaped_range_selection(&self) -> EscapedText {
        let text = self.hex_dump(
            self.selection_offset,
            self.selection_len.min(COPY_LIMIT_BYTES),
        );
        let max_bytes = COPY_LIMIT_BYTES.min(text.len());
        let mut escaped = escape_pdf_text(&text, EgressFormat::Markdown, max_bytes);
        escaped.truncated |= self.selection_len > COPY_LIMIT_BYTES;
        escaped
    }

    fn hex_dump(&self, offset: usize, len: usize) -> String {
        let start = offset.min(STREAM_TOTAL_BYTES);
        let end = start.saturating_add(len).min(STREAM_TOTAL_BYTES);
        let mut out = String::new();
        let mut line = start;
        while line < end {
            let line_end = (line + 16).min(end);
            out.push_str(&format!("{line:08x}  "));
            for index in line..line_end {
                out.push_str(&format!("{:02x} ", fake_stream_byte(index)));
            }
            for _ in line_end..line + 16 {
                out.push_str("   ");
            }
            out.push(' ');
            for index in line..line_end {
                let byte = fake_stream_byte(index);
                out.push(if byte.is_ascii_graphic() {
                    byte as char
                } else {
                    '.'
                });
            }
            out.push('\n');
            line = line_end;
        }
        out
    }
}

fn fake_stream_byte(index: usize) -> u8 {
    (index as u8).wrapping_mul(31).wrapping_add(17)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_tree_does_not_materialize_rows() {
        let tree = VirtualObjectTree::new(1_000_000);
        assert_eq!(tree.row_count(), 1_000_001);
        assert_eq!(tree.row_label(0), "root / catalog");
        assert_eq!(tree.row_label(999_999), "obj 999999 0 R  /FakeNode248");
    }

    #[test]
    fn stream_range_excerpt_is_bounded_and_escaped() {
        let mut model = LargeStreamModel {
            offset: 0,
            selection_offset: 0,
            selection_len: 16 * 1024,
            ..LargeStreamModel::default()
        };
        model.sync_hex_window();
        let escaped = model.escaped_range_selection();
        assert!(escaped.truncated);
        assert!(escaped.text.len() < COPY_LIMIT_BYTES * 2);
        assert!(escaped.text.contains("00000000"));
    }

    #[test]
    fn stream_hex_window_uses_generated_bytes() {
        let model = LargeStreamModel::default();
        let window = model.hex_window();
        assert!(window.starts_with("00000000  11 30 4f 6e"));
        assert!(window.lines().count() > 1);
    }

    #[test]
    fn selected_hex_text_is_copy_authority() {
        let mut model = LargeStreamModel {
            selection_offset: 1024,
            selection_len: 16 * 1024,
            ..LargeStreamModel::default()
        };
        model.selected_hex_text = Some("00000000  11 30 4f 6e".to_string());
        let escaped = model.escaped_copy_text();
        assert!(!escaped.truncated);
        assert_eq!(escaped.text, "00000000  11 30 4f 6e");
        assert_eq!(model.copy_source_label(), "selected hex text");
    }

    #[test]
    fn read_only_hex_window_resets_to_canonical_dump() {
        let mut model = LargeStreamModel::default();
        let canonical = model.hex_text.clone();
        model.hex_text.push_str("mutated");
        model.reset_hex_window();
        assert_eq!(model.hex_text, canonical);
    }

    #[test]
    fn reference_navigation_uses_back_forward_history() {
        let mut app = GuiShellApp::new();
        assert_eq!(app.selected_row, 0);

        app.follow_reference(42);
        assert_eq!(app.selected_row, 42);
        assert_eq!(app.back_stack, vec![0]);
        assert!(app.forward_stack.is_empty());

        app.go_back();
        assert_eq!(app.selected_row, 0);
        assert_eq!(app.forward_stack, vec![42]);
    }

    #[test]
    fn smoke_exit_option_is_stored_for_native_launch_tests() {
        let app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: Some(Duration::from_millis(250)),
        });
        assert_eq!(app.smoke_exit_after, Some(Duration::from_millis(250)));
    }

    #[test]
    fn theme_defines_named_font_stacks_and_severity_colors() {
        let fonts = pdbg_fonts();
        assert!(fonts
            .families
            .contains_key(&FontFamily::Name("pdbg-sans".into())));
        assert!(fonts
            .families
            .contains_key(&FontFamily::Name("pdbg-mono".into())));

        let style = pdbg_style();
        assert_eq!(style.visuals.panel_fill, PdbgTheme::PANEL);
        assert_eq!(
            PdbgTheme::severity_fg(&DiagnosticSeverity::Warning),
            PdbgTheme::WARN_FG
        );
        assert_eq!(
            PdbgTheme::severity_fg(&DiagnosticSeverity::Error),
            PdbgTheme::ERROR_FG
        );
    }
}
