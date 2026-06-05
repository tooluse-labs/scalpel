use crate::AppState;
use eframe::egui::{
    self, Color32, FontDefinitions, FontFamily, FontId, RichText, ScrollArea, TextEdit, TextStyle,
};
use pdbg_core::{
    escape_pdf_text, CancelToken, ChildContainer, ChildPage, ChildRange, DiagnosticSeverity,
    EgressFormat, EscapedText, NodeId, NodePathSegment, ObjectDetail, ObjectId, ObjectKind,
    ObjectSummary, ObjectValue, RenderRequest, RenderResult, ShimDocument, StreamChunk, StreamMode,
    StreamSummary, StreamViewMode,
};
use std::collections::BTreeMap;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

const VIRTUAL_TREE_ROWS: usize = 1_000_000;
const STREAM_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const HEX_WINDOW_BYTES: usize = 512;
const COPY_LIMIT_BYTES: usize = 4096;
const DEFAULT_RENDER_ZOOM: f32 = 2.0;

#[derive(Clone, Debug, Default)]
pub struct GuiRunOptions {
    pub smoke_exit_after: Option<Duration>,
    pub pdf_path: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RealRenderKey {
    page_index: usize,
    zoom_bits: u32,
    rotation_degrees: i32,
}

impl RealRenderKey {
    fn new(page_index: usize, zoom: f32, rotation_degrees: i32) -> Self {
        Self {
            page_index,
            zoom_bits: zoom.to_bits(),
            rotation_degrees,
        }
    }

    fn zoom(self) -> f32 {
        f32::from_bits(self.zoom_bits)
    }

    fn request(self) -> RenderRequest {
        let mut request = RenderRequest::page(self.page_index);
        request.zoom = self.zoom();
        request.rotation_degrees = self.rotation_degrees;
        request
    }
}

struct RealRenderJob {
    key: RealRenderKey,
    cancel: Arc<CancelToken>,
    receiver: mpsc::Receiver<RealRenderJobOutput>,
}

impl Drop for RealRenderJob {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

struct RealRenderJobOutput {
    key: RealRenderKey,
    result: Result<RenderResult, String>,
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
    fonts.font_data.insert(
        "InterVariable".to_string(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/InterVariable.ttf")).into(),
    );
    fonts.font_data.insert(
        "JetBrainsMono-Regular".to_string(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"))
            .into(),
    );
    fonts.families.insert(
        FontFamily::Name("pdbg-sans".into()),
        vec![
            "InterVariable".to_string(),
            "Ubuntu-Light".to_string(),
            "NotoEmoji-Regular".to_string(),
            "emoji-icon-font".to_string(),
        ],
    );
    fonts.families.insert(
        FontFamily::Name("pdbg-mono".into()),
        vec![
            "JetBrainsMono-Regular".to_string(),
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
    style.text_styles.insert(
        TextStyle::Name("panel-title".into()),
        FontId::new(13.0, FontFamily::Name("pdbg-sans".into())),
    );
    style.spacing.item_spacing = egui::vec2(8.0, 5.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.interact_size = egui::vec2(28.0, 22.0);
    style.spacing.window_margin = egui::Margin::same(8);

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
    const PANEL_ALT: Color32 = Color32::from_rgb(242, 246, 249);
    const CANVAS: Color32 = Color32::from_rgb(233, 237, 242);
    const PAGE: Color32 = Color32::from_rgb(255, 253, 248);
    const CODE_BG: Color32 = Color32::from_rgb(245, 247, 250);
    const CHIP_BG: Color32 = Color32::from_rgb(238, 243, 247);
    const SELECTED_BG: Color32 = Color32::from_rgb(232, 245, 246);
    const TOP_BAR: Color32 = Color32::from_rgb(28, 37, 48);
    const TOP_BAR_TEXT: Color32 = Color32::from_rgb(236, 241, 246);
    const TOP_BAR_MUTED: Color32 = Color32::from_rgb(170, 183, 196);
    const LOG_BG: Color32 = Color32::from_rgb(17, 24, 39);
    const LOG_TEXT: Color32 = Color32::from_rgb(218, 226, 235);
    const LOG_MUTED: Color32 = Color32::from_rgb(139, 152, 168);
    const TEXT: Color32 = Color32::from_rgb(31, 41, 51);
    const MUTED: Color32 = Color32::from_rgb(104, 116, 131);
    const BORDER: Color32 = Color32::from_rgb(207, 215, 225);
    const STRONG_BORDER: Color32 = Color32::from_rgb(179, 190, 203);
    const ACCENT: Color32 = Color32::from_rgb(8, 127, 140);
    const OPERATOR: Color32 = Color32::from_rgb(215, 100, 53);
    const SAFE: Color32 = Color32::from_rgb(22, 132, 92);
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

fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(PdbgTheme::PANEL)
        .stroke(egui::Stroke::new(1.0, PdbgTheme::BORDER))
        .inner_margin(egui::Margin::symmetric(10, 10))
}

fn section_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(PdbgTheme::SURFACE)
        .stroke(egui::Stroke::new(1.0, PdbgTheme::BORDER))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(10, 8))
}

fn section_header(ui: &mut egui::Ui, title: &str, detail: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title)
                .strong()
                .size(13.0)
                .color(PdbgTheme::TEXT),
        );
        if let Some(detail) = detail {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(detail).small().color(PdbgTheme::MUTED));
            });
        }
    });
    ui.add_space(4.0);
}

fn top_bar_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).size(12.0).color(if enabled {
            PdbgTheme::TOP_BAR_TEXT
        } else {
            PdbgTheme::TOP_BAR_MUTED
        }))
        .fill(Color32::from_rgb(42, 54, 68)),
    )
}

fn top_bar_chip(ui: &mut egui::Ui, label: &str, bg: Color32, fg: Color32) {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(11.0).strong().color(fg));
        });
}

fn option_text(value: Option<&str>) -> &str {
    value.unwrap_or("-")
}

pub struct GuiShellApp {
    state: Result<AppState, String>,
    launched_at: Instant,
    smoke_exit_after: Option<Duration>,
    tree: TreeModel,
    stream: LargeStreamModel,
    real_stream_mode: StreamMode,
    real_stream_view_mode: StreamViewMode,
    real_stream_offset: u64,
    real_stream_limit: usize,
    real_stream_key: Option<RealStreamKey>,
    real_stream_chunk: Option<StreamChunk>,
    real_stream_error: Option<String>,
    selected_row: usize,
    back_stack: Vec<usize>,
    forward_stack: Vec<usize>,
    selected_tab: InspectorTab,
    real_detail: Option<ObjectDetail>,
    real_detail_error: Option<String>,
    real_pages: Option<ChildPage<ObjectSummary>>,
    real_pages_error: Option<String>,
    render_page_index: usize,
    render_zoom: f32,
    render_rotation_degrees: i32,
    real_render_key: Option<RealRenderKey>,
    real_render_job: Option<RealRenderJob>,
    real_render: Option<RenderResult>,
    real_render_error: Option<String>,
    real_render_texture: Option<egui::TextureHandle>,
    copied_excerpt: Option<EscapedText>,
    status_log: Vec<String>,
}

impl Default for GuiShellApp {
    fn default() -> Self {
        Self::new()
    }
}

impl GuiShellApp {
    pub fn new() -> Self {
        Self::new_with_options(GuiRunOptions::default())
    }

    pub fn new_with_options(options: GuiRunOptions) -> Self {
        let state = open_app_state(options.pdf_path.as_deref());
        let tree = TreeModel::from_state(&state, options.pdf_path.is_some());
        let (real_detail, real_detail_error) = load_initial_real_detail(&state, &tree);
        let (real_pages, real_pages_error) = load_initial_real_pages(&state, &tree);
        let render_page_index = 0;
        let render_zoom = DEFAULT_RENDER_ZOOM;
        let render_rotation_degrees = 0;
        let mut status_log = initial_status_log(&state, &tree, options.pdf_path.as_deref());
        if let Some(pages) = &real_pages {
            status_log.push(format!(
                "loaded page list {}",
                child_page_detail(pages.total, pages.items.len())
            ));
        } else if let Some(err) = &real_pages_error {
            status_log.push(format!("page list load failed: {err}"));
        }
        let smoke_exit_after = options.smoke_exit_after;
        let mut app = Self {
            state,
            launched_at: Instant::now(),
            smoke_exit_after,
            tree,
            stream: LargeStreamModel::default(),
            real_stream_mode: StreamMode::Raw,
            real_stream_view_mode: StreamViewMode::Hex,
            real_stream_offset: 0,
            real_stream_limit: HEX_WINDOW_BYTES,
            real_stream_key: None,
            real_stream_chunk: None,
            real_stream_error: None,
            selected_row: 0,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            selected_tab: InspectorTab::Object,
            real_detail,
            real_detail_error,
            real_pages,
            real_pages_error,
            render_page_index,
            render_zoom,
            render_rotation_degrees,
            real_render_key: None,
            real_render_job: None,
            real_render: None,
            real_render_error: None,
            real_render_texture: None,
            copied_excerpt: None,
            status_log,
        };
        app.refresh_real_render();
        app
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
        self.refresh_real_detail_for_selection();
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
        self.refresh_real_detail_for_selection();
        self.status_log.push(format!(
            "resolved reference to {}",
            self.tree.row_label(row)
        ));
    }

    fn go_back(&mut self) {
        if let Some(row) = self.back_stack.pop() {
            self.forward_stack.push(self.selected_row);
            self.selected_row = row;
            self.refresh_real_detail_for_selection();
            self.status_log
                .push(format!("back to {}", self.tree.row_label(row)));
        }
    }

    fn go_forward(&mut self) {
        if let Some(row) = self.forward_stack.pop() {
            self.back_stack.push(self.selected_row);
            self.selected_row = row;
            self.refresh_real_detail_for_selection();
            self.status_log
                .push(format!("forward to {}", self.tree.row_label(row)));
        }
    }

    fn follow_real_reference(&mut self, object: ObjectId) {
        let Some(summary) = self
            .state
            .as_ref()
            .ok()
            .and_then(|state| state.panels.summary.as_ref())
        else {
            return;
        };
        let row = self
            .tree
            .ensure_real_object_row(summary.doc.clone(), object);
        self.follow_reference(row);
    }

    fn expand_selected_real_row(&mut self) -> usize {
        let Some(detail) = self.real_detail.clone() else {
            return 0;
        };
        let inserted = match &mut self.tree {
            TreeModel::Real(tree) => tree.expand_row_from_detail(self.selected_row, &detail),
            TreeModel::Virtual(_) => 0,
        };
        if inserted > 0 {
            self.status_log.push(format!(
                "expanded {} bounded children under {}",
                inserted,
                self.tree.row_label(self.selected_row)
            ));
        }
        inserted
    }

    fn refresh_real_detail_for_selection(&mut self) {
        self.clear_real_stream_chunk();
        let TreeModel::Real(tree) = &self.tree else {
            return;
        };
        let Some(summary) = tree.summary(self.selected_row) else {
            self.real_detail = None;
            self.real_detail_error = None;
            return;
        };
        match self.state.as_ref() {
            Ok(state) => match load_object_detail(state, &summary.id) {
                Ok(detail) => {
                    if let TreeModel::Real(tree) = &mut self.tree {
                        tree.update_row_from_detail(self.selected_row, &detail);
                    }
                    self.real_detail = Some(detail);
                    self.real_detail_error = None;
                }
                Err(err) => {
                    self.real_detail = None;
                    self.real_detail_error = Some(err);
                }
            },
            Err(err) => {
                self.real_detail = None;
                self.real_detail_error = Some(err.clone());
            }
        }
    }

    fn clear_real_stream_chunk(&mut self) {
        self.real_stream_key = None;
        self.real_stream_chunk = None;
        self.real_stream_error = None;
    }

    fn refresh_real_stream_chunk(&mut self, object: ObjectId) {
        let key = RealStreamKey {
            object,
            mode: self.real_stream_mode,
            offset: self.real_stream_offset,
            limit: self.real_stream_limit,
        };
        let result = match self.state.as_ref() {
            Ok(state) => load_stream_chunk(state, object, key.mode, key.offset, key.limit),
            Err(err) => Err(err.clone()),
        };
        self.real_stream_key = Some(key);
        match result {
            Ok(chunk) => {
                self.status_log.push(format!(
                    "loaded {} stream chunk {} {} R @ {} ({} bytes{})",
                    stream_mode_label(chunk.mode),
                    object.num,
                    object.gen,
                    chunk.offset,
                    chunk.bytes.len(),
                    if chunk.truncated { ", truncated" } else { "" }
                ));
                self.real_stream_chunk = Some(chunk);
                self.real_stream_error = None;
            }
            Err(err) => {
                self.real_stream_chunk = None;
                self.real_stream_error = Some(err);
            }
        }
    }

    fn page_count(&self) -> usize {
        self.state
            .as_ref()
            .ok()
            .and_then(|state| state.panels.summary.as_ref())
            .map(|summary| summary.page_count)
            .unwrap_or(0)
    }

    fn current_render_key(&self) -> Option<RealRenderKey> {
        if !self.tree.is_real() || self.page_count() == 0 {
            return None;
        }
        Some(RealRenderKey::new(
            self.render_page_index,
            self.render_zoom,
            self.render_rotation_degrees,
        ))
    }

    fn set_render_page(&mut self, page_index: usize) {
        let page_count = self.page_count();
        if page_count == 0 {
            return;
        }
        let page_index = page_index.min(page_count - 1);
        if self.render_page_index == page_index {
            return;
        }
        self.render_page_index = page_index;
        self.refresh_real_render();
    }

    fn refresh_real_render(&mut self) {
        let Some(key) = self.current_render_key() else {
            return;
        };
        if self.real_render_key == Some(key)
            && (self.real_render.is_some() || self.real_render_job.is_some())
        {
            return;
        }
        if let Some(job) = self.real_render_job.take() {
            job.cancel.cancel();
        }
        self.real_render_texture = None;
        self.real_render = None;
        self.real_render_error = None;
        self.real_render_key = Some(key);

        let Ok(state) = self.state.as_ref() else {
            self.real_render_error = Some("document is not open".to_string());
            return;
        };
        let cancel = match CancelToken::new() {
            Ok(cancel) => Arc::new(cancel),
            Err(err) => {
                self.real_render_error = Some(err.message);
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        let session = state.session.clone();
        let worker_cancel = Arc::clone(&cancel);
        thread::spawn(move || {
            let request = key.request();
            let result = session
                .run_task(|document| {
                    document.render_page_with_cancel_token(&request, worker_cancel.as_ref())
                })
                .map_err(|err| err.message);
            let _ = sender.send(RealRenderJobOutput { key, result });
        });
        self.real_render_job = Some(RealRenderJob {
            key,
            cancel,
            receiver,
        });
        self.status_log.push(format!(
            "queued page {} @ {:.0}% rot {} render",
            key.page_index + 1,
            key.zoom() * 100.0,
            key.rotation_degrees
        ));
    }

    fn poll_real_render_job(&mut self) {
        let Some(polled) =
            self.real_render_job
                .as_ref()
                .and_then(|job| match job.receiver.try_recv() {
                    Ok(output) => Some(Ok(output)),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => Some(Err(job.key)),
                })
        else {
            return;
        };

        self.real_render_job = None;
        match polled {
            Ok(output) => {
                if self.current_render_key() != Some(output.key) {
                    self.status_log.push(format!(
                        "discarded stale page {} render",
                        output.key.page_index + 1
                    ));
                    return;
                }
                match output.result {
                    Ok(render) => {
                        self.real_render_texture = None;
                        self.real_render_error = None;
                        self.status_log.push(format!(
                            "rendered page {} @ {:.0}% rot {} -> {}x{}",
                            render.page_index + 1,
                            output.key.zoom() * 100.0,
                            output.key.rotation_degrees,
                            render.width,
                            render.height
                        ));
                        self.real_render = Some(render);
                    }
                    Err(err) => {
                        self.real_render = None;
                        self.real_render_error = Some(err.clone());
                        self.status_log.push(format!(
                            "page {} render failed: {err}",
                            output.key.page_index + 1
                        ));
                    }
                }
            }
            Err(key) => {
                if self.current_render_key() == Some(key) {
                    self.real_render = None;
                    self.real_render_error = Some("render worker disconnected".to_string());
                }
            }
        }
    }

    fn document_chips(&self) -> (String, String, String) {
        if let Ok(state) = &self.state {
            if let Some(summary) = &state.panels.summary {
                return (
                    file_chip_label(&summary.file_path),
                    format!("pages {}", summary.page_count),
                    format!("xref {}", summary.xref_size),
                );
            }
        }
        (
            "fake.pdf".to_string(),
            "pages 1".to_string(),
            "xref 3".to_string(),
        )
    }

    fn breadcrumb_label(&self) -> String {
        if let Some(detail) = &self.real_detail {
            return node_breadcrumb(&detail.id);
        }
        match &self.tree {
            TreeModel::Real(tree) => tree
                .summary(self.selected_row)
                .map(|summary| node_breadcrumb(&summary.id))
                .unwrap_or_else(|| "Document".to_string()),
            TreeModel::Virtual(_) => "Document/FakeNode".to_string(),
        }
    }

    fn real_diagnostics(&self) -> Vec<pdbg_core::DiagnosticSummary> {
        let mut diagnostics = Vec::new();
        if let Ok(state) = &self.state {
            if let Some(summary) = &state.panels.summary {
                diagnostics.extend(summary.diagnostics.clone());
            }
        }
        if let Some(detail) = &self.real_detail {
            diagnostics.extend(detail.diagnostics.clone());
        }
        diagnostics
    }
}

fn open_app_state(pdf_path: Option<&str>) -> Result<AppState, String> {
    if let Some(path) = pdf_path {
        #[cfg(feature = "real-mupdf")]
        {
            return AppState::new_real_path(path).map_err(|err| err.message);
        }
        #[cfg(not(feature = "real-mupdf"))]
        {
            let _ = path;
            return Err(
                "`--pdf` requires building pdbg-app with `--features real-mupdf`".to_string(),
            );
        }
    }
    AppState::new_headless().map_err(|err| err.message)
}

fn load_initial_real_detail(
    state: &Result<AppState, String>,
    tree: &TreeModel,
) -> (Option<ObjectDetail>, Option<String>) {
    let TreeModel::Real(tree) = tree else {
        return (None, None);
    };
    let Some(summary) = tree.summary(0) else {
        return (None, None);
    };
    match state {
        Ok(state) => match load_object_detail(state, &summary.id) {
            Ok(detail) => (Some(detail), None),
            Err(err) => (None, Some(err)),
        },
        Err(err) => (None, Some(err.clone())),
    }
}

fn load_initial_real_pages(
    state: &Result<AppState, String>,
    tree: &TreeModel,
) -> (Option<ChildPage<ObjectSummary>>, Option<String>) {
    let TreeModel::Real(tree) = tree else {
        return (None, None);
    };
    let Some(page_root) = tree.page_root_summary() else {
        return (None, None);
    };

    match state {
        Ok(state) => match state.session.run_task(|document| {
            document.children(
                &page_root.id,
                ChildRange {
                    offset: 0,
                    limit: 64,
                },
                ChildContainer::Array,
            )
        }) {
            Ok(pages) => (Some(pages), None),
            Err(err) => (None, Some(err.message)),
        },
        Err(err) => (None, Some(err.clone())),
    }
}

fn load_object_detail(state: &AppState, id: &NodeId) -> Result<ObjectDetail, String> {
    state
        .session
        .run_task(|document| {
            document.object_detail(
                id,
                ChildRange {
                    offset: 0,
                    limit: 32,
                },
            )
        })
        .map_err(|err| err.message)
}

fn load_stream_chunk(
    state: &AppState,
    object: ObjectId,
    mode: StreamMode,
    offset: u64,
    limit: usize,
) -> Result<StreamChunk, String> {
    state
        .session
        .run_task(|document| document.stream_load(object, mode, offset, limit))
        .map_err(|err| err.message)
}

fn initial_status_log(
    state: &Result<AppState, String>,
    tree: &TreeModel,
    pdf_path: Option<&str>,
) -> Vec<String> {
    match (state, tree, pdf_path) {
        (Ok(_), TreeModel::Real(tree), Some(path)) => vec![
            format!("real MuPDF opened {}", file_chip_label(path)),
            format!("loaded bounded root page: {}", tree.row_count_label()),
            "real stream bytes available as bounded raw/decoded chunks".to_string(),
        ],
        (Err(err), _, Some(path)) => vec![
            format!("failed to open {}", file_chip_label(path)),
            err.clone(),
        ],
        _ => vec![
            "fake shim opened fake.pdf".to_string(),
            "virtual object tree uses generated rows".to_string(),
            "large stream pane uses generated bytes".to_string(),
        ],
    }
}

fn tree_text_format(color: Color32) -> egui::TextFormat {
    egui::TextFormat {
        font_id: FontId::new(11.0, FontFamily::Name("pdbg-mono".into())),
        color,
        ..Default::default()
    }
}

fn file_chip_label(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn kind_badge_text(kind: &ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Null => "null",
        ObjectKind::Bool => "bool",
        ObjectKind::Int => "int",
        ObjectKind::Real => "real",
        ObjectKind::Name => "name",
        ObjectKind::String => "str",
        ObjectKind::Array => "[]",
        ObjectKind::Dict => "<>",
        ObjectKind::IndirectRef => "ref",
        ObjectKind::Stream => "stm",
        ObjectKind::Page => "page",
        ObjectKind::XrefEntry => "xref",
        ObjectKind::Trailer => "trl",
        ObjectKind::Metadata => "meta",
        ObjectKind::Unknown => "?",
    }
}

fn object_kind_label(kind: &ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Null => "null",
        ObjectKind::Bool => "bool",
        ObjectKind::Int => "int",
        ObjectKind::Real => "real",
        ObjectKind::Name => "name",
        ObjectKind::String => "string",
        ObjectKind::Array => "array",
        ObjectKind::Dict => "dictionary",
        ObjectKind::IndirectRef => "indirect_ref",
        ObjectKind::Stream => "stream",
        ObjectKind::Page => "page",
        ObjectKind::XrefEntry => "xref_entry",
        ObjectKind::Trailer => "trailer",
        ObjectKind::Metadata => "metadata",
        ObjectKind::Unknown => "unknown",
    }
}

fn type_badge(ui: &mut egui::Ui, kind: &ObjectKind) {
    egui::Frame::new()
        .fill(PdbgTheme::CHIP_BG)
        .stroke(egui::Stroke::new(1.0, PdbgTheme::BORDER))
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(5, 2))
        .show(ui, |ui| {
            ui.label(
                RichText::new(kind_badge_text(kind))
                    .monospace()
                    .color(PdbgTheme::MUTED),
            );
        });
}

fn summary_inline_text(summary: &ObjectSummary) -> String {
    let mut out = String::new();
    if let Some(object) = summary.object {
        out.push_str(&format!("[{} {} R] ", object.num, object.gen));
    }
    let preview = summary.preview.trim();
    if preview.is_empty() {
        out.push_str(&summary.label);
    } else {
        out.push_str(preview);
    }
    if summary.has_stream {
        out.push_str(" stream");
    }
    out
}

fn object_value_preview(value: &ObjectValue, fallback: &str) -> String {
    match value {
        ObjectValue::Null => "null".to_string(),
        ObjectValue::Bool(value) => value.to_string(),
        ObjectValue::Int(value) => value.to_string(),
        ObjectValue::Real(value) => value.to_string(),
        ObjectValue::Name(value) => format!("/{value}"),
        ObjectValue::StringBytes {
            bytes,
            decoded_text,
            ..
        } => decoded_text
            .clone()
            .unwrap_or_else(|| format!("{} bytes", bytes.len())),
        ObjectValue::IndirectRef(object) => format!("{} {} R", object.num, object.gen),
        ObjectValue::Container => fallback.to_string(),
        ObjectValue::Unknown => fallback.to_string(),
    }
}

fn detail_reference_targets(detail: &ObjectDetail) -> Vec<ObjectId> {
    let mut out = Vec::new();
    push_value_reference(&mut out, &detail.value);
    if let Some(entries) = &detail.dictionary_entries {
        for entry in &entries.items {
            if let Some(object) = entry.value.object {
                push_unique_object(&mut out, object);
            }
        }
    }
    if let Some(entries) = &detail.array_entries {
        for entry in &entries.items {
            if let Some(object) = entry.object {
                push_unique_object(&mut out, object);
            }
        }
    }
    out
}

fn push_value_reference(out: &mut Vec<ObjectId>, value: &ObjectValue) {
    if let ObjectValue::IndirectRef(object) = value {
        push_unique_object(out, *object);
    }
}

fn push_unique_object(out: &mut Vec<ObjectId>, object: ObjectId) {
    if !out.contains(&object) {
        out.push(object);
    }
}

fn child_page_detail(total: Option<usize>, loaded: usize) -> String {
    match total {
        Some(total) => format!("{loaded} loaded / {total} total"),
        None => format!("{loaded} loaded"),
    }
}

fn draw_stream_summary_grid(ui: &mut egui::Ui, stream: &StreamSummary) {
    egui::Grid::new("real_stream_summary_grid")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            ui.label("object");
            ui.monospace(format!("{} {} R", stream.object.num, stream.object.gen));
            ui.end_row();
            ui.label("filters");
            ui.monospace(if stream.filters.is_empty() {
                "-".to_string()
            } else {
                stream.filters.join(", ")
            });
            ui.end_row();
            ui.label("raw size");
            ui.monospace(optional_u64(stream.raw_size_hint));
            ui.end_row();
            ui.label("decoded size");
            ui.monospace(optional_u64(stream.decoded_size_hint));
            ui.end_row();
            ui.label("can decode");
            ui.monospace(stream.can_decode.to_string());
            ui.end_row();
            ui.label("image preview");
            ui.monospace(stream.image_preview_available.to_string());
            ui.end_row();
        });
}

fn optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn render_result_color_image(render: &RenderResult) -> Option<egui::ColorImage> {
    let width = render.width as usize;
    let height = render.height as usize;
    if width == 0 || height == 0 || render.stride < width.checked_mul(4)? {
        return None;
    }
    let required = render.stride.checked_mul(height)?;
    if render.pixels_rgba.len() < required {
        return None;
    }

    let row_len = width * 4;
    let mut compact = Vec::with_capacity(row_len * height);
    for row in 0..height {
        let start = row * render.stride;
        compact.extend_from_slice(&render.pixels_rgba[start..start + row_len]);
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [width, height],
        &compact,
    ))
}

fn stream_mode_label(mode: StreamMode) -> &'static str {
    match mode {
        StreamMode::Raw => "raw",
        StreamMode::Decoded => "decoded",
    }
}

fn stream_view_mode_label(mode: StreamViewMode) -> &'static str {
    match mode {
        StreamViewMode::Hex => "Hex",
        StreamViewMode::Text => "Text",
        StreamViewMode::Bytes => "Bytes",
    }
}

fn stream_chunk_display_text(chunk: &StreamChunk, view_mode: StreamViewMode) -> String {
    match view_mode {
        StreamViewMode::Hex => {
            if chunk.bytes.is_empty() {
                "<empty chunk>".to_string()
            } else {
                hex_dump_bytes(chunk.offset, &chunk.bytes)
            }
        }
        StreamViewMode::Text => String::from_utf8_lossy(&chunk.bytes).into_owned(),
        StreamViewMode::Bytes => chunk
            .bytes
            .iter()
            .map(|byte| byte.to_string())
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn hex_dump_bytes(base_offset: u64, bytes: &[u8]) -> String {
    let mut out = String::new();
    for (line_index, chunk) in bytes.chunks(16).enumerate() {
        let line_offset = base_offset.saturating_add((line_index as u64).saturating_mul(16));
        out.push_str(&format!("{line_offset:08x}  "));
        for byte in chunk {
            out.push_str(&format!("{byte:02x} "));
        }
        for _ in chunk.len()..16 {
            out.push_str("   ");
        }
        out.push(' ');
        for byte in chunk {
            out.push(if byte.is_ascii_graphic() || *byte == b' ' {
                *byte as char
            } else {
                '.'
            });
        }
        out.push('\n');
    }
    out
}

fn draw_diagnostic_card(ui: &mut egui::Ui, diagnostic: &pdbg_core::DiagnosticSummary) {
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

fn node_breadcrumb(id: &NodeId) -> String {
    id.to_serialized()
        .segments
        .iter()
        .map(segment_label)
        .collect::<Vec<_>>()
        .join("/")
}

fn segment_label(segment: &NodePathSegment) -> String {
    match segment {
        NodePathSegment::DocumentRoot => "Root".to_string(),
        NodePathSegment::Trailer => "Trailer".to_string(),
        NodePathSegment::Catalog => "Catalog".to_string(),
        NodePathSegment::XrefRoot => "Xref".to_string(),
        NodePathSegment::XrefObject(object) => format!("{} {} R", object.num, object.gen),
        NodePathSegment::PageRoot => "Pages".to_string(),
        NodePathSegment::Page { index } => format!("Page[{index}]"),
        NodePathSegment::DictKey(key) => key.clone(),
        NodePathSegment::ArrayIndex(index) => format!("[{index}]"),
        NodePathSegment::IndirectRef(object) => format!("{} {} R", object.num, object.gen),
        NodePathSegment::Stream { object, decoded } => {
            let mode = if *decoded { "decoded" } else { "raw" };
            format!("Stream({} {} R,{mode})", object.num, object.gen)
        }
        NodePathSegment::ResourceGroup { page_index, group } => {
            format!("Page[{page_index}]/{}", group.as_public_str())
        }
    }
}

impl eframe::App for GuiShellApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_real_render_job();
        if self.real_render_job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        egui::Panel::top("top_bar")
            .frame(
                egui::Frame::new()
                    .fill(PdbgTheme::TOP_BAR)
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show_inside(ui, |ui| self.draw_top_bar(ui, &ctx));

        egui::Panel::bottom("log_panel")
            .resizable(true)
            .default_size(150.0)
            .frame(
                egui::Frame::new()
                    .fill(PdbgTheme::LOG_BG)
                    .inner_margin(egui::Margin::symmetric(10, 8)),
            )
            .show_inside(ui, |ui| self.draw_log(ui));

        egui::Panel::left("document_tree")
            .resizable(true)
            .default_size(320.0)
            .size_range(220.0..=520.0)
            .frame(panel_frame())
            .show_inside(ui, |ui| self.draw_tree(ui));

        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(440.0)
            .size_range(320.0..=680.0)
            .frame(panel_frame())
            .show_inside(ui, |ui| self.draw_inspector(ui, &ctx));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(PdbgTheme::CANVAS)
                    .inner_margin(egui::Margin::symmetric(10, 10)),
            )
            .show_inside(ui, |ui| self.draw_page_preview(ui));

        if self
            .smoke_exit_after
            .is_some_and(|duration| self.launched_at.elapsed() >= duration)
        {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl GuiShellApp {
    fn draw_top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("pdbg")
                    .strong()
                    .size(15.0)
                    .color(PdbgTheme::TOP_BAR_TEXT),
            );
            ui.add_space(8.0);
            if top_bar_button(ui, "Back", !self.back_stack.is_empty()).clicked() {
                self.go_back();
            }
            if top_bar_button(ui, "Forward", !self.forward_stack.is_empty()).clicked() {
                self.go_forward();
            }
            ui.add_space(8.0);
            let (file, pages, xref) = self.document_chips();
            top_bar_chip(ui, &file, PdbgTheme::PANEL_ALT, PdbgTheme::TEXT);
            top_bar_chip(ui, &pages, PdbgTheme::PANEL_ALT, PdbgTheme::TEXT);
            top_bar_chip(ui, &xref, PdbgTheme::PANEL_ALT, PdbgTheme::TEXT);
            top_bar_chip(ui, "SAFE MODE", PdbgTheme::SAFE, Color32::WHITE);
            ui.add_space(8.0);
            ui.label(
                RichText::new(self.breadcrumb_label())
                    .monospace()
                    .color(PdbgTheme::TOP_BAR_MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("scale {:.2}x", ctx.pixels_per_point()))
                        .monospace()
                        .color(PdbgTheme::TOP_BAR_MUTED),
                );
            });
        });
    }

    fn draw_tree(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "Document Tree", Some(&self.tree.row_count_label()));

        let row_height = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
        ScrollArea::vertical().show_rows(ui, row_height, self.tree.row_count(), |ui, range| {
            for row in range {
                let selected = row == self.selected_row;
                let job = self.tree.row_layout_job(row, selected);
                if ui.selectable_label(selected, job).clicked() {
                    self.select_row_from_tree(row);
                }
            }
        });
    }

    fn draw_page_preview(&mut self, ui: &mut egui::Ui) {
        let preview_detail = if self.tree.is_real() {
            "real MuPDF render"
        } else {
            "fake renderer surface"
        };
        section_header(ui, "Page Preview", Some(preview_detail));
        if self.draw_real_page_preview(ui) {
            return;
        }

        let available = ui.available_size();
        let desired = egui::vec2(available.x.max(320.0), available.y.max(360.0));
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, PdbgTheme::CANVAS);

        let max_page = rect.shrink2(egui::vec2(44.0, 34.0));
        let page_ratio = 0.72;
        let mut page_h = max_page.height();
        let mut page_w = page_h * page_ratio;
        if page_w > max_page.width() {
            page_w = max_page.width();
            page_h = page_w / page_ratio;
        }
        let page_rect = egui::Rect::from_center_size(max_page.center(), egui::vec2(page_w, page_h));
        let shadow_rect = page_rect.translate(egui::vec2(0.0, 3.0));
        painter.rect_filled(shadow_rect, 3.0, Color32::from_black_alpha(18));
        painter.rect_filled(page_rect, 3.0, PdbgTheme::PAGE);
        painter.rect_stroke(
            page_rect,
            3.0,
            egui::Stroke::new(1.0, PdbgTheme::STRONG_BORDER),
            egui::StrokeKind::Outside,
        );

        let content = page_rect.shrink(34.0);
        for index in 0..9 {
            let y = content.top() + index as f32 * 24.0;
            painter.line_segment(
                [
                    egui::pos2(content.left(), y),
                    egui::pos2(content.right() - (index % 3) as f32 * 54.0, y),
                ],
                egui::Stroke::new(2.5, Color32::from_rgb(88, 105, 120)),
            );
        }

        let highlight = egui::Rect::from_min_size(
            egui::pos2(content.left() + 36.0, content.top() + 190.0),
            egui::vec2(220.0, 86.0),
        );
        painter.rect_filled(
            highlight,
            2.0,
            Color32::from_rgba_premultiplied(215, 100, 53, 18),
        );
        painter.rect_stroke(
            highlight,
            2.0,
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

    fn draw_real_page_preview(&mut self, ui: &mut egui::Ui) -> bool {
        self.draw_real_render_controls(ui);
        self.draw_real_page_list(ui);
        if self.real_render_job.is_some() {
            let available = ui.available_size();
            let desired = egui::vec2(available.x.max(320.0), available.y.max(320.0));
            let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, PdbgTheme::CANVAS);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Rendering page...",
                egui::FontId::proportional(13.0),
                PdbgTheme::MUTED,
            );
            return true;
        }
        if let Some(err) = &self.real_render_error {
            ui.colored_label(PdbgTheme::ERROR_FG, err);
            return true;
        }
        let Some(render) = &self.real_render else {
            return false;
        };
        if self.real_render_texture.is_none() {
            let Some(image) = render_result_color_image(render) else {
                ui.colored_label(PdbgTheme::ERROR_FG, "render output has invalid RGBA layout");
                return true;
            };
            self.real_render_texture = Some(ui.ctx().load_texture(
                "real-page-preview",
                image,
                egui::TextureOptions::LINEAR,
            ));
        }
        let Some(texture) = &self.real_render_texture else {
            return false;
        };

        let available = ui.available_size();
        let texture_size = texture.size_vec2();
        let scale = (available.x / texture_size.x)
            .min(available.y / texture_size.y)
            .max(0.1);
        let display_size = texture_size * scale;
        ui.vertical_centered(|ui| {
            ui.add(
                egui::Image::new((texture.id(), display_size))
                    .bg_fill(PdbgTheme::PAGE)
                    .corner_radius(3),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "page {} / {}x{} / stride {}",
                    render.page_index + 1,
                    render.width,
                    render.height,
                    render.stride
                ))
                .monospace()
                .color(PdbgTheme::MUTED),
            );
        });
        true
    }

    fn draw_real_render_controls(&mut self, ui: &mut egui::Ui) {
        let page_count = self.page_count();
        if page_count == 0 {
            return;
        }

        let mut rerender = false;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.render_page_index > 0, egui::Button::new("Prev"))
                .clicked()
            {
                self.render_page_index -= 1;
                rerender = true;
            }
            ui.label(
                RichText::new(format!(
                    "Page {} / {page_count}",
                    self.render_page_index + 1
                ))
                .small()
                .color(PdbgTheme::MUTED),
            );
            if ui
                .add_enabled(
                    self.render_page_index + 1 < page_count,
                    egui::Button::new("Next"),
                )
                .clicked()
            {
                self.render_page_index += 1;
                rerender = true;
            }

            ui.separator();
            egui::ComboBox::from_id_salt("render_zoom")
                .selected_text(format!("{:.0}%", self.render_zoom * 100.0))
                .show_ui(ui, |ui| {
                    for zoom in [0.5_f32, 1.0, 1.5, 2.0, 3.0, 4.0] {
                        if ui
                            .selectable_value(
                                &mut self.render_zoom,
                                zoom,
                                format!("{:.0}%", zoom * 100.0),
                            )
                            .changed()
                        {
                            rerender = true;
                        }
                    }
                });
            egui::ComboBox::from_id_salt("render_rotation")
                .selected_text(format!("{} deg", self.render_rotation_degrees))
                .show_ui(ui, |ui| {
                    for rotation in [0, 90, 180, 270] {
                        if ui
                            .selectable_value(
                                &mut self.render_rotation_degrees,
                                rotation,
                                format!("{rotation} deg"),
                            )
                            .changed()
                        {
                            rerender = true;
                        }
                    }
                });
        });
        if rerender {
            self.refresh_real_render();
        }
        ui.add_space(6.0);
    }

    fn draw_real_page_list(&mut self, ui: &mut egui::Ui) {
        if self.real_pages.is_none() && self.real_pages_error.is_none() {
            return;
        }

        let mut clicked_page = None;

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Pages").small().color(PdbgTheme::MUTED));
            if let Some(err) = &self.real_pages_error {
                ui.colored_label(PdbgTheme::ERROR_FG, err);
                return;
            }

            let Some(pages) = &self.real_pages else {
                return;
            };
            let selected_page = self
                .real_render
                .as_ref()
                .map(|render| render.page_index)
                .unwrap_or(0);
            for (index, page) in pages.items.iter().enumerate().take(12) {
                let selected = index == selected_page;
                if ui
                    .add(
                        egui::Button::new(RichText::new(&page.label).monospace().size(11.0).color(
                            if selected {
                                PdbgTheme::ACCENT
                            } else {
                                PdbgTheme::TEXT
                            },
                        ))
                        .fill(if selected {
                            PdbgTheme::SELECTED_BG
                        } else {
                            PdbgTheme::CHIP_BG
                        })
                        .stroke(egui::Stroke::new(
                            if selected { 1.5 } else { 1.0 },
                            if selected {
                                PdbgTheme::ACCENT
                            } else {
                                PdbgTheme::BORDER
                            },
                        )),
                    )
                    .clicked()
                {
                    clicked_page = Some(index);
                }
            }
            ui.label(
                RichText::new(child_page_detail(pages.total, pages.items.len()))
                    .small()
                    .color(PdbgTheme::MUTED),
            );
        });
        if let Some(page_index) = clicked_page {
            self.set_render_page(page_index);
        }
        ui.add_space(6.0);
    }

    fn draw_inspector(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        section_header(ui, "Inspector", Some(&self.selected_object_label()));
        ui.horizontal(|ui| {
            self.tab_button(ui, InspectorTab::Object, "Object");
            self.tab_button(ui, InspectorTab::Stream, "Stream");
            self.tab_button(ui, InspectorTab::Diagnostics, "Diagnostics");
        });
        ui.add_space(6.0);

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
        if self.tree.is_real() {
            self.draw_real_object_panel(ui);
            return;
        }

        section_frame().show(ui, |ui| {
            ui.label(
                RichText::new(self.selected_object_label())
                    .monospace()
                    .strong()
                    .color(PdbgTheme::TEXT),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("Indirect references")
                    .small()
                    .color(PdbgTheme::MUTED),
            );
            ui.horizontal_wrapped(|ui| {
                for target in self.tree.reference_targets(self.selected_row) {
                    if ui
                        .link(RichText::new(format!("{target} 0 R")).monospace())
                        .on_hover_text("Resolve reference and push navigation history")
                        .clicked()
                    {
                        self.follow_reference(target);
                    }
                }
            });
        });
        ui.add_space(8.0);
        if let Ok(state) = &self.state {
            if let Some(preview) = &state.panels.detail_preview {
                section_frame().show(ui, |ui| {
                    ui.label(RichText::new("Preview").small().color(PdbgTheme::MUTED));
                    ui.add_space(3.0);
                    ui.monospace(preview);
                });
            }
            if let Some(summary) = &state.panels.summary {
                ui.add_space(8.0);
                section_frame().show(ui, |ui| {
                    ui.label(
                        RichText::new("Document summary")
                            .small()
                            .color(PdbgTheme::MUTED),
                    );
                    ui.add_space(3.0);
                    egui::Grid::new("document_summary_grid")
                        .num_columns(2)
                        .spacing([12.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("hash");
                            ui.monospace(option_text(summary.file_hash.as_deref()));
                            ui.end_row();
                            ui.label("version");
                            ui.monospace(option_text(summary.pdf_version.as_deref()));
                            ui.end_row();
                            ui.label("permissions");
                            ui.monospace(format!(
                                "print={} copy={} modify={}",
                                summary.permissions.print,
                                summary.permissions.copy,
                                summary.permissions.modify
                            ));
                            ui.end_row();
                        });
                });
            }
        } else if let Err(err) = &self.state {
            ui.colored_label(PdbgTheme::ERROR_FG, err);
        }
    }

    fn draw_real_object_panel(&mut self, ui: &mut egui::Ui) {
        if let Some(err) = &self.real_detail_error {
            ui.colored_label(PdbgTheme::ERROR_FG, err);
            return;
        }

        let Some(detail) = self.real_detail.clone() else {
            ui.label(RichText::new("No object selected").color(PdbgTheme::MUTED));
            return;
        };

        section_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                type_badge(ui, &detail.kind);
                ui.label(
                    RichText::new(&detail.label)
                        .monospace()
                        .strong()
                        .color(PdbgTheme::TEXT),
                );
                if let Some(object) = detail.object {
                    ui.label(
                        RichText::new(format!("[{} {} R]", object.num, object.gen))
                            .monospace()
                            .color(PdbgTheme::ACCENT),
                    );
                }
            });
            ui.add_space(6.0);
            egui::Grid::new("real_object_summary_grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label("kind");
                    ui.monospace(object_kind_label(&detail.kind));
                    ui.end_row();
                    ui.label("value");
                    ui.monospace(object_value_preview(&detail.value, &detail.preview));
                    ui.end_row();
                    ui.label("path");
                    ui.monospace(node_breadcrumb(&detail.id));
                    ui.end_row();
                });
        });

        if detail.dictionary_entries.is_some() || detail.array_entries.is_some() {
            ui.add_space(8.0);
            if ui
                .button("Expand bounded children")
                .on_hover_text("Append only the loaded child page to the tree")
                .clicked()
            {
                self.expand_selected_real_row();
            }
        }

        let references = detail_reference_targets(&detail);
        if !references.is_empty() {
            ui.add_space(8.0);
            section_frame().show(ui, |ui| {
                ui.label(
                    RichText::new("Indirect references")
                        .small()
                        .color(PdbgTheme::MUTED),
                );
                ui.add_space(3.0);
                ui.horizontal_wrapped(|ui| {
                    for object in references {
                        if ui
                            .link(
                                RichText::new(format!("{} {} R", object.num, object.gen))
                                    .monospace(),
                            )
                            .on_hover_text("Resolve reference and push navigation history")
                            .clicked()
                        {
                            self.follow_real_reference(object);
                        }
                    }
                });
            });
        }

        if let Some(entries) = &detail.dictionary_entries {
            ui.add_space(8.0);
            section_frame().show(ui, |ui| {
                section_header(
                    ui,
                    "Dictionary",
                    Some(&child_page_detail(entries.total, entries.items.len())),
                );
                egui::Grid::new("real_dictionary_grid")
                    .num_columns(3)
                    .spacing([12.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for entry in &entries.items {
                            ui.monospace(format!("/{}", entry.key));
                            type_badge(ui, &entry.value.kind);
                            ui.monospace(summary_inline_text(&entry.value));
                            ui.end_row();
                        }
                    });
            });
        }

        if let Some(entries) = &detail.array_entries {
            ui.add_space(8.0);
            section_frame().show(ui, |ui| {
                section_header(
                    ui,
                    "Array",
                    Some(&child_page_detail(entries.total, entries.items.len())),
                );
                egui::Grid::new("real_array_grid")
                    .num_columns(3)
                    .spacing([12.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for (index, entry) in entries.items.iter().enumerate() {
                            ui.monospace(format!("[{index}]"));
                            type_badge(ui, &entry.kind);
                            ui.monospace(summary_inline_text(entry));
                            ui.end_row();
                        }
                    });
            });
        }
    }

    fn draw_stream_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.tree.is_real() {
            self.draw_real_stream_panel(ui, ctx);
            return;
        }

        section_frame().show(ui, |ui| {
            egui::Grid::new("stream_controls_grid")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Offset");
                    let offset_response = ui.add(
                        egui::DragValue::new(&mut self.stream.offset)
                            .range(0..=STREAM_TOTAL_BYTES.saturating_sub(HEX_WINDOW_BYTES))
                            .speed(64),
                    );
                    if offset_response.changed() {
                        self.stream.sync_hex_window();
                    }
                    ui.end_row();

                    ui.label("Fallback range");
                    ui.horizontal(|ui| {
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
                    ui.end_row();
                });
            ui.label(
                RichText::new("Read-only hex window. Drag-select visible text before copying.")
                    .small()
                    .color(PdbgTheme::MUTED),
            );
        });
        ui.add_space(8.0);

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
            ui.add_space(8.0);
            ui.label(
                RichText::new(if escaped.truncated {
                    "Last copied excerpt (truncated)"
                } else {
                    "Last copied excerpt"
                })
                .small()
                .color(PdbgTheme::MUTED),
            );
            let mut copied = escaped.text.clone();
            ui.add(
                TextEdit::multiline(&mut copied)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(5)
                    .interactive(false),
            );
        }
    }

    fn draw_real_stream_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(detail) = &self.real_detail else {
            ui.label(RichText::new("No object selected").color(PdbgTheme::MUTED));
            return;
        };
        let Some(stream) = detail.stream.clone() else {
            ui.label(RichText::new("Selected object has no stream").color(PdbgTheme::MUTED));
            return;
        };

        section_frame().show(ui, |ui| {
            section_header(ui, "Stream Summary", Some("real bounded bytes"));
            draw_stream_summary_grid(ui, &stream);
        });

        ui.add_space(8.0);
        let mut request_changed = false;
        section_frame().show(ui, |ui| {
            section_header(ui, "Stream Bytes", Some("raw / decoded chunk"));
            ui.horizontal(|ui| {
                ui.label(RichText::new("Decode").small().color(PdbgTheme::MUTED));
                request_changed |= ui
                    .selectable_value(&mut self.real_stream_mode, StreamMode::Raw, "Raw")
                    .changed();
                let decoded_enabled = stream.can_decode;
                ui.add_enabled_ui(decoded_enabled, |ui| {
                    request_changed |= ui
                        .selectable_value(
                            &mut self.real_stream_mode,
                            StreamMode::Decoded,
                            "Decoded",
                        )
                        .changed();
                });
                if !decoded_enabled && self.real_stream_mode == StreamMode::Decoded {
                    self.real_stream_mode = StreamMode::Raw;
                    request_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("View").small().color(PdbgTheme::MUTED));
                ui.selectable_value(&mut self.real_stream_view_mode, StreamViewMode::Hex, "Hex");
                ui.selectable_value(
                    &mut self.real_stream_view_mode,
                    StreamViewMode::Text,
                    "Text",
                );
                ui.selectable_value(
                    &mut self.real_stream_view_mode,
                    StreamViewMode::Bytes,
                    "Bytes",
                );
            });
            ui.add_space(4.0);
            egui::Grid::new("real_stream_controls_grid")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Offset");
                    request_changed |= ui
                        .add(egui::DragValue::new(&mut self.real_stream_offset).speed(64))
                        .changed();
                    ui.end_row();

                    ui.label("Limit");
                    request_changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.real_stream_limit)
                                .range(1..=32 * 1024)
                                .speed(64),
                        )
                        .changed();
                    ui.end_row();
                });
            if request_changed {
                self.clear_real_stream_chunk();
            }
            if ui.button("Load chunk").clicked() || self.real_stream_key.is_none() {
                self.refresh_real_stream_chunk(stream.object);
            }
        });

        if let Some(err) = &self.real_stream_error {
            ui.add_space(8.0);
            ui.colored_label(PdbgTheme::ERROR_FG, err);
            return;
        }

        let Some(chunk) = self.real_stream_chunk.clone() else {
            return;
        };
        ui.add_space(8.0);
        let total = chunk
            .total_size
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string());
        section_frame().show(ui, |ui| {
            section_header(
                ui,
                stream_view_mode_label(self.real_stream_view_mode),
                Some(&format!(
                    "{} bytes @ {} / total {}{}",
                    chunk.bytes.len(),
                    chunk.offset,
                    total,
                    if chunk.truncated { " / truncated" } else { "" }
                )),
            );
            let mut visible_text = stream_chunk_display_text(&chunk, self.real_stream_view_mode);
            ui.add(
                TextEdit::multiline(&mut visible_text)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(14)
                    .code_editor()
                    .interactive(false),
            );
            if ui.button("Copy visible chunk").clicked() {
                let escaped =
                    escape_pdf_text(&visible_text, EgressFormat::Markdown, COPY_LIMIT_BYTES);
                ctx.copy_text(escaped.text.clone());
                self.status_log.push(format!(
                    "copied visible {} {} stream chunk{}",
                    stream_view_mode_label(self.real_stream_view_mode).to_ascii_lowercase(),
                    stream_mode_label(chunk.mode),
                    if escaped.truncated {
                        " (truncated)"
                    } else {
                        ""
                    }
                ));
                self.copied_excerpt = Some(escaped);
            }
        });

        if !chunk.decode_diagnostics.is_empty() {
            ui.add_space(8.0);
            for diagnostic in &chunk.decode_diagnostics {
                draw_diagnostic_card(ui, diagnostic);
            }
        }
    }

    fn draw_diagnostics_panel(&mut self, ui: &mut egui::Ui) {
        if self.tree.is_real() {
            let diagnostics = self.real_diagnostics();
            if diagnostics.is_empty() {
                ui.label(RichText::new("No diagnostics").color(PdbgTheme::MUTED));
            } else {
                for diagnostic in diagnostics {
                    draw_diagnostic_card(ui, &diagnostic);
                }
            }
            return;
        }

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
                    draw_diagnostic_card(ui, diagnostic);
                }
            }
        }
    }

    fn draw_log(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Log")
                    .strong()
                    .size(13.0)
                    .color(PdbgTheme::LOG_TEXT),
            );
            if ui
                .add(
                    egui::Button::new(RichText::new("Clear").color(PdbgTheme::LOG_TEXT))
                        .fill(Color32::from_rgb(31, 41, 55)),
                )
                .clicked()
            {
                self.status_log.clear();
            }
        });
        ui.add_space(4.0);
        ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
            for line in &self.status_log {
                ui.label(RichText::new(line).monospace().color(PdbgTheme::LOG_MUTED));
            }
        });
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RealStreamKey {
    object: ObjectId,
    mode: StreamMode,
    offset: u64,
    limit: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InspectorTab {
    Object,
    Stream,
    Diagnostics,
}

#[derive(Clone, Debug)]
enum TreeModel {
    Virtual(VirtualObjectTree),
    Real(RealObjectTree),
}

impl TreeModel {
    fn from_state(state: &Result<AppState, String>, prefer_real: bool) -> Self {
        if prefer_real {
            if let Ok(state) = state {
                if let Some(tree) = &state.panels.tree {
                    return Self::Real(RealObjectTree::from_child_page(tree));
                }
            }
        }
        Self::Virtual(VirtualObjectTree::new(VIRTUAL_TREE_ROWS))
    }

    fn is_real(&self) -> bool {
        matches!(self, Self::Real(_))
    }

    fn row_count(&self) -> usize {
        match self {
            Self::Virtual(tree) => tree.row_count(),
            Self::Real(tree) => tree.row_count(),
        }
    }

    fn row_count_label(&self) -> String {
        match self {
            Self::Virtual(tree) => format!("{} rows", tree.row_count()),
            Self::Real(tree) => tree.row_count_label(),
        }
    }

    fn row_label(&self, row: usize) -> String {
        match self {
            Self::Virtual(tree) => tree.row_label(row),
            Self::Real(tree) => tree.row_label(row),
        }
    }

    fn row_layout_job(&self, row: usize, selected: bool) -> egui::text::LayoutJob {
        match self {
            Self::Virtual(tree) => tree.row_layout_job(row, selected),
            Self::Real(tree) => tree.row_layout_job(row, selected),
        }
    }

    fn reference_targets(&self, row: usize) -> [usize; 3] {
        match self {
            Self::Virtual(tree) => tree.reference_targets(row),
            Self::Real(_) => [row; 3],
        }
    }

    fn ensure_real_object_row(&mut self, doc: pdbg_core::DocumentId, object: ObjectId) -> usize {
        match self {
            Self::Real(tree) => tree.ensure_object_row(doc, object),
            Self::Virtual(_) => 0,
        }
    }
}

#[derive(Clone, Debug)]
struct RealObjectTree {
    rows: Vec<RealTreeRow>,
    total: Option<usize>,
}

#[derive(Clone, Debug)]
struct RealTreeRow {
    summary: ObjectSummary,
    depth: usize,
    expanded: bool,
}

impl RealObjectTree {
    fn from_child_page(page: &pdbg_core::ChildPage<ObjectSummary>) -> Self {
        Self {
            rows: page
                .items
                .iter()
                .cloned()
                .map(|summary| RealTreeRow {
                    summary,
                    depth: 0,
                    expanded: false,
                })
                .collect(),
            total: page.total,
        }
    }

    fn row_count(&self) -> usize {
        self.rows.len().max(1)
    }

    fn row_count_label(&self) -> String {
        match self.total {
            Some(total) => format!("{} loaded / {total} total", self.rows.len()),
            None => format!("{} loaded", self.rows.len()),
        }
    }

    fn summary(&self, row: usize) -> Option<&ObjectSummary> {
        self.rows.get(row).map(|row| &row.summary)
    }

    fn page_root_summary(&self) -> Option<&ObjectSummary> {
        self.rows
            .iter()
            .find(|row| {
                matches!(
                    &row.summary.id,
                    NodeId::DictEntry { key, .. } if key == "Pages"
                ) || matches!(&row.summary.id, NodeId::PageRoot { .. })
            })
            .map(|row| &row.summary)
    }

    fn row_label(&self, row: usize) -> String {
        self.summary(row)
            .map(summary_inline_text)
            .unwrap_or_else(|| "no real rows loaded".to_string())
    }

    fn row_layout_job(&self, row: usize, selected: bool) -> egui::text::LayoutJob {
        let Some(row) = self.rows.get(row) else {
            let mut job = egui::text::LayoutJob::default();
            job.append(
                "no real rows loaded",
                0.0,
                tree_text_format(PdbgTheme::MUTED),
            );
            return job;
        };
        let mut job = egui::text::LayoutJob::default();
        let primary = if selected {
            PdbgTheme::ACCENT
        } else {
            PdbgTheme::TEXT
        };
        let muted = if selected {
            PdbgTheme::ACCENT
        } else {
            PdbgTheme::MUTED
        };
        let accent = if row.summary.diagnostics.is_empty() {
            muted
        } else {
            PdbgTheme::WARN_FG
        };
        let indent = "  ".repeat(row.depth);
        job.append(&indent, 0.0, tree_text_format(muted));
        job.append(
            kind_badge_text(&row.summary.kind),
            0.0,
            tree_text_format(accent),
        );
        job.append(" ", 0.0, tree_text_format(muted));
        job.append(&row.summary.label, 0.0, tree_text_format(primary));
        if let Some(count) = row.summary.child_count {
            job.append(&format!(" ({count})"), 0.0, tree_text_format(muted));
        }
        if let Some(object) = row.summary.object {
            job.append(
                &format!(" [{} {} R]", object.num, object.gen),
                0.0,
                tree_text_format(PdbgTheme::ACCENT),
            );
        }
        let preview = row.summary.preview.trim();
        if !preview.is_empty() && preview != row.summary.label {
            job.append("  ", 0.0, tree_text_format(muted));
            job.append(preview, 0.0, tree_text_format(muted));
        }
        if row.summary.has_stream {
            job.append("  stream", 0.0, tree_text_format(PdbgTheme::OPERATOR));
        }
        job
    }

    fn ensure_object_row(&mut self, doc: pdbg_core::DocumentId, object: ObjectId) -> usize {
        if let Some(index) = self
            .rows
            .iter()
            .position(|row| row.summary.object == Some(object))
        {
            return index;
        }
        let id = NodeId::XrefObject { doc, object };
        self.rows.push(RealTreeRow {
            summary: ObjectSummary {
                id,
                kind: ObjectKind::Unknown,
                label: format!("Object {}", object.num),
                preview: format!("{} {} R", object.num, object.gen),
                object: Some(object),
                has_children: true,
                has_stream: false,
                child_count: None,
                byte_size_hint: None,
                diagnostics: Vec::new(),
            },
            depth: 0,
            expanded: false,
        });
        self.rows.len() - 1
    }

    fn update_row_from_detail(&mut self, row: usize, detail: &ObjectDetail) {
        let Some(row) = self.rows.get_mut(row) else {
            return;
        };
        row.summary.kind = detail.kind.clone();
        row.summary.label = detail.label.clone();
        row.summary.preview = detail.preview.clone();
        row.summary.object = detail.object;
        row.summary.has_stream = detail.stream.is_some();
        row.summary.has_children =
            detail.dictionary_entries.is_some() || detail.array_entries.is_some();
        row.summary.child_count = detail
            .dictionary_entries
            .as_ref()
            .and_then(|entries| entries.total)
            .or_else(|| {
                detail
                    .array_entries
                    .as_ref()
                    .and_then(|entries| entries.total)
            });
        row.summary.diagnostics = detail.diagnostics.clone();
    }

    fn expand_row_from_detail(&mut self, row: usize, detail: &ObjectDetail) -> usize {
        let Some(parent) = self.rows.get_mut(row) else {
            return 0;
        };
        if parent.expanded {
            return 0;
        }
        parent.expanded = true;
        let child_depth = parent.depth + 1;

        let mut children = Vec::new();
        if let Some(entries) = &detail.dictionary_entries {
            children.extend(entries.items.iter().map(|entry| RealTreeRow {
                summary: entry.value.clone(),
                depth: child_depth,
                expanded: false,
            }));
        }
        if let Some(entries) = &detail.array_entries {
            children.extend(entries.items.iter().cloned().map(|summary| RealTreeRow {
                summary,
                depth: child_depth,
                expanded: false,
            }));
        }

        let inserted = children.len();
        self.rows.splice(row + 1..row + 1, children);
        inserted
    }
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

    fn row_layout_job(&self, row: usize, selected: bool) -> egui::text::LayoutJob {
        let label = self.row_label(row);
        // Two-tone row: object id in primary text, the /Name in muted
        // (both accent when selected) so the tree reads as structure,
        // not a flat dump.
        let id_color = if selected {
            PdbgTheme::ACCENT
        } else {
            PdbgTheme::TEXT
        };
        let name_color = if selected {
            PdbgTheme::ACCENT
        } else {
            PdbgTheme::MUTED
        };
        let mut job = egui::text::LayoutJob::default();
        match label.split_once("  ") {
            Some((id_part, name_part)) => {
                job.append(id_part, 0.0, tree_text_format(id_color));
                job.append(&format!("  {name_part}"), 0.0, tree_text_format(name_color));
            }
            None => job.append(&label, 0.0, tree_text_format(id_color)),
        }
        job
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

    #[cfg(feature = "real-mupdf")]
    fn wait_for_real_render(app: &mut GuiShellApp) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.real_render_job.is_some() && Instant::now() < deadline {
            app.poll_real_render_job();
            if app.real_render_job.is_some() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        app.poll_real_render_job();
        assert!(
            app.real_render_job.is_none(),
            "real render job did not finish before test timeout"
        );
    }

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
    fn hex_dump_bytes_formats_offsets_hex_and_ascii() {
        let dump = hex_dump_bytes(16, b"BT /F1\n");
        assert!(dump.starts_with("00000010  42 54 20 2f 46 31 0a"));
        assert!(dump.contains("BT /F1."));
    }

    #[test]
    fn stream_chunk_display_text_supports_hex_text_and_bytes() {
        let chunk = StreamChunk {
            mode: StreamMode::Decoded,
            offset: 32,
            bytes: b"Hi\n".to_vec(),
            total_size: Some(3),
            truncated: false,
            decode_diagnostics: Vec::new(),
        };

        assert!(stream_chunk_display_text(&chunk, StreamViewMode::Hex)
            .starts_with("00000020  48 69 0a"));
        assert_eq!(
            stream_chunk_display_text(&chunk, StreamViewMode::Text),
            "Hi\n"
        );
        assert_eq!(
            stream_chunk_display_text(&chunk, StreamViewMode::Bytes),
            "72 105 10"
        );
    }

    #[test]
    fn render_result_color_image_compacts_padded_stride() {
        let render = RenderResult {
            page_index: 0,
            width: 1,
            height: 2,
            stride: 8,
            pixels_rgba: vec![1, 2, 3, 4, 99, 99, 99, 99, 5, 6, 7, 8, 88, 88, 88, 88],
            duration_ms: 0,
            diagnostics: Vec::new(),
        };

        let image = render_result_color_image(&render).unwrap();
        assert_eq!(image.size, [1, 2]);
        assert_eq!(image.pixels[0], Color32::from_rgba_unmultiplied(1, 2, 3, 4));
        assert_eq!(image.pixels[1], Color32::from_rgba_unmultiplied(5, 6, 7, 8));
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
    fn option_text_hides_rust_debug_wrappers() {
        assert_eq!(option_text(Some("fake-hash")), "fake-hash");
        assert_eq!(option_text(None), "-");
    }

    #[test]
    fn real_tree_row_anatomy_uses_kind_count_ref_preview_and_diagnostics() {
        let summary = ObjectSummary {
            id: NodeId::XrefObject {
                doc: pdbg_core::DocumentId(7),
                object: ObjectId { num: 12, gen: 0 },
            },
            kind: ObjectKind::Dict,
            label: "Info".to_string(),
            preview: "<< /Creator (...) >>".to_string(),
            object: Some(ObjectId { num: 12, gen: 0 }),
            has_children: true,
            has_stream: true,
            child_count: Some(3),
            byte_size_hint: None,
            diagnostics: vec![pdbg_core::DiagnosticSummary {
                severity: DiagnosticSeverity::Warning,
                code: pdbg_core::DiagnosticCode::RepairWarning,
                message: "repaired".to_string(),
                node: None,
                page_index: None,
                object: Some(ObjectId { num: 12, gen: 0 }),
            }],
        };
        let tree = RealObjectTree::from_child_page(&pdbg_core::ChildPage {
            total: Some(1),
            items: vec![summary],
        });

        assert_eq!(tree.row_count_label(), "1 loaded / 1 total");
        assert!(tree.row_label(0).contains("[12 0 R]"));
        let job = tree.row_layout_job(0, false);
        let row_text = job.text;
        assert!(row_text.contains("<> Info (3) [12 0 R]"));
        assert!(row_text.contains("stream"));
    }

    #[test]
    fn node_breadcrumb_formats_public_path_segments() {
        let id = NodeId::DictEntry {
            doc: pdbg_core::DocumentId(1),
            parent: Box::new(NodeId::ArrayEntry {
                doc: pdbg_core::DocumentId(1),
                parent: Box::new(NodeId::PageRoot {
                    doc: pdbg_core::DocumentId(1),
                }),
                index: 0,
            }),
            key: "Contents".to_string(),
        };

        assert_eq!(node_breadcrumb(&id), "Pages/[0]/Contents");
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_gui_model_loads_bounded_tree_and_detail_from_pdf_path() {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/minimal.pdf"
        );
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: Some(fixture.to_string()),
        });
        assert!(app.real_render_job.is_some());
        wait_for_real_render(&mut app);

        assert!(app.state.is_ok());
        assert!(matches!(app.tree, TreeModel::Real(_)));
        assert_eq!(app.tree.row_count(), 4);
        assert_eq!(app.tree.row_count_label(), "4 loaded / 4 total");
        assert!(app.real_detail.is_some());
        let pages = app.real_pages.as_ref().unwrap();
        assert_eq!(pages.total, Some(1));
        assert_eq!(pages.items[0].label, "Page 1");
        assert!(app.real_render.is_some());
        assert!(app.breadcrumb_label().contains("Trailer"));
        assert!(app.status_log[0].contains("real MuPDF opened"));
        assert!(app
            .status_log
            .iter()
            .any(|line| line.starts_with("loaded page list")));
        assert!(app
            .status_log
            .iter()
            .any(|line| line.starts_with("queued page 1")));
        assert!(app
            .status_log
            .iter()
            .any(|line| line.starts_with("rendered page 1")));
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_gui_large_pdf_smoke_keeps_tree_bounded_and_records_timings() {
        let path = write_temp_pdf("gui-large", &synthetic_large_xref_pdf(1_500));
        let open_start = Instant::now();
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: Some(path.to_string_lossy().to_string()),
        });
        wait_for_real_render(&mut app);
        let open_elapsed = open_start.elapsed();

        let xref_size = app
            .state
            .as_ref()
            .unwrap()
            .panels
            .summary
            .as_ref()
            .unwrap()
            .xref_size;
        assert!(xref_size > 1_000);
        assert_eq!(app.tree.row_count(), 4);

        let xref_expand_start = Instant::now();
        app.select_row_from_tree(3);
        let xref_inserted = app.expand_selected_real_row();
        let xref_elapsed = xref_expand_start.elapsed();
        assert_eq!(xref_inserted, 64);
        assert!(app.tree.row_count() < xref_size / 10);

        let pages_expand_start = Instant::now();
        app.select_row_from_tree(2);
        let pages_inserted = app.expand_selected_real_row();
        let pages_elapsed = pages_expand_start.elapsed();
        assert_eq!(pages_inserted, 1);

        let jump_start = Instant::now();
        app.follow_real_reference(ObjectId { num: 1, gen: 0 });
        let jump_elapsed = jump_start.elapsed();
        assert_eq!(
            app.real_detail.as_ref().and_then(|detail| detail.object),
            Some(ObjectId { num: 1, gen: 0 })
        );
        assert!(app.tree.row_count() < xref_size / 10);
        assert!(open_elapsed < Duration::from_secs(5));
        assert!(xref_elapsed < Duration::from_secs(2));
        assert!(pages_elapsed < Duration::from_secs(2));
        assert!(jump_elapsed < Duration::from_secs(2));

        app.status_log.push(format!(
            "large smoke timings: open={}ms xref_expand={}ms pages_expand={}ms jump={}ms",
            open_elapsed.as_millis(),
            xref_elapsed.as_millis(),
            pages_elapsed.as_millis(),
            jump_elapsed.as_millis()
        ));
        assert!(app
            .status_log
            .iter()
            .any(|line| line.starts_with("large smoke timings:")));

        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_gui_stream_panel_loads_bounded_real_bytes() {
        let path = write_temp_pdf("gui-stream", &synthetic_large_xref_pdf(16));
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: Some(path.to_string_lossy().to_string()),
        });
        wait_for_real_render(&mut app);

        app.follow_real_reference(ObjectId { num: 4, gen: 0 });
        assert!(app
            .real_detail
            .as_ref()
            .is_some_and(|detail| detail.stream.is_some()));
        app.real_stream_limit = 16;
        app.refresh_real_stream_chunk(ObjectId { num: 4, gen: 0 });

        let chunk = app.real_stream_chunk.as_ref().unwrap();
        assert_eq!(chunk.mode, StreamMode::Raw);
        assert_eq!(chunk.offset, 0);
        assert!(chunk.bytes.starts_with(b"BT /F1"));
        assert!(chunk.truncated);
        assert!(app
            .status_log
            .iter()
            .any(|line| line.contains("loaded raw stream chunk 4 0 R")));

        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_gui_page_controls_refresh_render_parameters() {
        let path = write_temp_pdf("gui-pages", &synthetic_two_page_pdf());
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: Some(path.to_string_lossy().to_string()),
        });
        wait_for_real_render(&mut app);

        assert_eq!(app.page_count(), 2);
        assert_eq!(app.real_pages.as_ref().unwrap().total, Some(2));
        assert_eq!(app.real_pages.as_ref().unwrap().items[1].label, "Page 2");
        let initial = app.real_render.as_ref().unwrap();
        assert_eq!(initial.page_index, 0);
        assert_eq!((initial.width, initial.height), (400, 200));

        app.render_zoom = 1.0;
        app.refresh_real_render();
        wait_for_real_render(&mut app);
        let zoomed = app.real_render.as_ref().unwrap();
        assert_eq!(zoomed.page_index, 0);
        assert_eq!((zoomed.width, zoomed.height), (200, 100));

        app.set_render_page(1);
        wait_for_real_render(&mut app);
        let second_page = app.real_render.as_ref().unwrap();
        assert_eq!(second_page.page_index, 1);
        assert_eq!((second_page.width, second_page.height), (100, 200));

        app.set_render_page(99);
        assert_eq!(app.render_page_index, 1);
        assert!(app
            .status_log
            .iter()
            .any(|line| line.starts_with("rendered page 2 @ 100%")));

        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_gui_render_job_replacement_keeps_latest_page() {
        let path = write_temp_pdf("gui-render-replace", &synthetic_two_page_pdf());
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: Some(path.to_string_lossy().to_string()),
        });
        assert!(app.real_render_job.is_some());

        app.set_render_page(1);
        wait_for_real_render(&mut app);

        let render = app.real_render.as_ref().unwrap();
        assert_eq!(render.page_index, 1);
        assert_eq!((render.width, render.height), (200, 400));
        assert!(app
            .status_log
            .iter()
            .any(|line| line.starts_with("queued page 2")));

        let _ = std::fs::remove_file(path);
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
            pdf_path: None,
        });
        assert_eq!(app.smoke_exit_after, Some(Duration::from_millis(250)));
    }

    #[test]
    fn fake_shell_keeps_mock_page_preview() {
        let app = GuiShellApp::new();
        assert!(!app.tree.is_real());
        assert!(app.real_pages.is_none());
        assert!(app.real_pages_error.is_none());
        assert!(app.real_render.is_none());
        assert!(app.real_render_error.is_none());
        assert!(!app
            .status_log
            .iter()
            .any(|line| line.starts_with("loaded page list")));
        assert!(!app
            .status_log
            .iter()
            .any(|line| line.starts_with("rendered page preview")));
    }

    #[test]
    fn theme_defines_named_font_stacks_and_severity_colors() {
        let fonts = pdbg_fonts();
        assert!(fonts.font_data.contains_key("InterVariable"));
        assert!(fonts.font_data.contains_key("JetBrainsMono-Regular"));
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

    #[cfg(feature = "real-mupdf")]
    fn write_temp_pdf(prefix: &str, bytes: &[u8]) -> std::path::PathBuf {
        let temp_path = std::env::temp_dir().join(format!(
            "pdbg-app-{}-{}-{}.pdf",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&temp_path, bytes).unwrap();
        temp_path
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_large_xref_pdf(object_count: usize) -> Vec<u8> {
        let object_count = object_count.max(8);
        let mut pdf = String::from("%PDF-1.7\n");
        let mut offsets = vec![0usize; object_count + 1];
        let push_object = |pdf: &mut String, offsets: &mut [usize], number: usize, body: &str| {
            offsets[number] = pdf.len();
            pdf.push_str(&format!("{number} 0 obj\n{body}\nendobj\n"));
        };

        push_object(
            &mut pdf,
            &mut offsets,
            1,
            "<< /Type /Catalog /Pages 2 0 R /Names 5 0 R >>",
        );
        push_object(
            &mut pdf,
            &mut offsets,
            2,
            "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
        );
        push_object(
            &mut pdf,
            &mut offsets,
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>",
        );
        let stream = "BT /F1 12 Tf 24 120 Td (pdbg large smoke) Tj ET";
        push_object(
            &mut pdf,
            &mut offsets,
            4,
            &format!(
                "<< /Length {} >>\nstream\n{}\nendstream",
                stream.len(),
                stream
            ),
        );
        push_object(&mut pdf, &mut offsets, 5, "<< /Dests 6 0 R >>");
        for number in 6..=object_count {
            let prev = if number == 6 { 1 } else { number - 1 };
            push_object(
                &mut pdf,
                &mut offsets,
                number,
                &format!("<< /Index {number} /Prev {prev} 0 R /Name /Node{number} >>"),
            );
        }

        let xref_offset = pdf.len();
        pdf.push_str(&format!("xref\n0 {}\n", object_count + 1));
        pdf.push_str("0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size {} >>\nstartxref\n{xref_offset}\n%%EOF\n",
            object_count + 1
        ));
        pdf.into_bytes()
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_two_page_pdf() -> Vec<u8> {
        let mut pdf = String::from("%PDF-1.7\n");
        let mut offsets = vec![0usize; 7];
        let push_object = |pdf: &mut String, offsets: &mut [usize], number: usize, body: &str| {
            offsets[number] = pdf.len();
            pdf.push_str(&format!("{number} 0 obj\n{body}\nendobj\n"));
        };

        push_object(
            &mut pdf,
            &mut offsets,
            1,
            "<< /Type /Catalog /Pages 2 0 R >>",
        );
        push_object(
            &mut pdf,
            &mut offsets,
            2,
            "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>",
        );
        push_object(
            &mut pdf,
            &mut offsets,
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents 5 0 R >>",
        );
        push_object(
            &mut pdf,
            &mut offsets,
            4,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 200] /Contents 6 0 R >>",
        );
        push_object(
            &mut pdf,
            &mut offsets,
            5,
            "<< /Length 0 >>\nstream\n\nendstream",
        );
        push_object(
            &mut pdf,
            &mut offsets,
            6,
            "<< /Length 0 >>\nstream\n\nendstream",
        );

        let xref_offset = pdf.len();
        pdf.push_str("xref\n0 7\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 7 >>\nstartxref\n{xref_offset}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }
}
