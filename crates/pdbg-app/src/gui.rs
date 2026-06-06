use crate::AppState;
use eframe::egui::{
    self, Color32, FontDefinitions, FontFamily, FontId, RichText, ScrollArea, TextEdit, TextStyle,
};
use pdbg_core::{
    build_markdown_report, diagnostics_payload_to_json_string, escape_pdf_text,
    search_objects_with_cancel, search_text_with_cache, CancelToken, ChildContainer, ChildPage,
    ChildRange, DiagnosticCode, DiagnosticFilter, DiagnosticSeverity, DiagnosticSummary,
    DocumentDiagnostics, EgressFormat, EscapedText, MarkdownReportInput, NodeId, NodePathSegment,
    ObjectDetail, ObjectId, ObjectKind, ObjectSearchField, ObjectSearchHit, ObjectSearchRequest,
    ObjectSearchResult, ObjectSummary, ObjectValue, RenderRequest, RenderResult, RenderResultCache,
    ShimDocument, StreamChunk, StreamChunkCache, StreamMode, StreamSummary, StreamViewMode,
    TextPageCache, TextSearchHit, TextSearchRequest, TextSearchResult,
};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

const VIRTUAL_TREE_ROWS: usize = 1_000_000;
const STREAM_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const HEX_WINDOW_BYTES: usize = 512;
const COPY_LIMIT_BYTES: usize = 4096;
const DEFAULT_RENDER_ZOOM: f32 = 2.0;
const OBJECT_SEARCH_CHILD_PAGE_SIZE: usize = 64;
const OBJECT_SEARCH_MAX_CHILD_PAGES: usize = 2;
const OBJECT_SEARCH_MAX_DEPTH: usize = 4;
const OBJECT_SEARCH_MAX_NODES: usize = 768;
const OBJECT_SEARCH_MAX_RESULTS: usize = 100;
const TEXT_SEARCH_CACHE_MAX_PAGES: usize = 16;
const TEXT_SEARCH_CACHE_MAX_BYTES: usize = 8 * 1024 * 1024;
const RENDER_CACHE_MAX_ITEMS: usize = 32;
const RENDER_CACHE_MAX_BYTES: usize = 256 * 1024 * 1024;
const DECODED_STREAM_CACHE_MAX_ITEMS: usize = 64;
const DECODED_STREAM_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;
const TEXT_SEARCH_MAX_PAGES: usize = 64;
const TEXT_SEARCH_MAX_RESULTS: usize = 100;
const TEXT_SEARCH_MAX_CHARS_PER_PAGE: usize = 512 * 1024;
const TEXT_SEARCH_MAX_BLOCKS_PER_PAGE: usize = 50_000;
const MARKDOWN_REPORT_LIMIT_BYTES: usize = 64 * 1024;
const REPORT_DIAGNOSTIC_LIMIT: usize = 128;
const REPORT_SEARCH_HIT_LIMIT: usize = 64;
const RECENT_PDF_MAX_ITEMS: usize = 10;
const PATH_DISPLAY_MAX_BYTES: usize = 4096;
const APP_TITLE: &str = "pdbg Preview";

#[derive(Clone, Debug, Default)]
pub struct GuiRunOptions {
    pub smoke_exit_after: Option<Duration>,
    pub pdf_path: Option<String>,
    pub recent_files_path: Option<PathBuf>,
    pub start_empty_when_no_pdf: bool,
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

struct RealStreamJob {
    key: RealStreamKey,
    cancel: Arc<CancelToken>,
    receiver: mpsc::Receiver<RealStreamJobOutput>,
}

impl Drop for RealStreamJob {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

struct RealStreamJobOutput {
    key: RealStreamKey,
    result: Result<StreamChunk, String>,
}

struct RealTextSearchJob {
    query: String,
    cancel: Arc<CancelToken>,
    receiver: mpsc::Receiver<RealTextSearchJobOutput>,
}

impl Drop for RealTextSearchJob {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

struct RealTextSearchJobOutput {
    query: String,
    result: Result<(TextSearchResult, TextPageCache), String>,
}

struct RealObjectSearchJob {
    query: String,
    cancel: Arc<CancelToken>,
    receiver: mpsc::Receiver<RealObjectSearchJobOutput>,
}

impl Drop for RealObjectSearchJob {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

struct RealObjectSearchJobOutput {
    query: String,
    result: Result<ObjectSearchResult, String>,
}

struct OpenPdfJob {
    path: String,
    receiver: mpsc::Receiver<OpenPdfJobOutput>,
}

struct OpenPdfJobOutput {
    path: String,
    result: Result<OpenPdfJobResult, String>,
}

enum OpenPdfJobResult {
    Opened(Box<OpenedPdfModel>),
    NeedsPassword,
}

struct OpenedPdfModel {
    state: AppState,
    tree: TreeModel,
    real_detail: Option<ObjectDetail>,
    real_detail_error: Option<String>,
    real_pages: Option<ChildPage<ObjectSummary>>,
    real_pages_error: Option<String>,
    status_log: Vec<String>,
}

pub fn run_gui_with_options(options: GuiRunOptions) -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_TITLE)
            .with_inner_size([1440.0, 900.0]),
        ..Default::default()
    };

    eframe::run_native(
        APP_TITLE,
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
    empty_workspace: bool,
    launched_at: Instant,
    smoke_exit_after: Option<Duration>,
    recent_files_path: PathBuf,
    recent_pdf_paths: Vec<String>,
    open_pdf_dialog_open: bool,
    open_pdf_path_input: String,
    open_pdf_password_input: String,
    open_pdf_error: Option<String>,
    open_pdf_job: Option<OpenPdfJob>,
    tree: TreeModel,
    stream: LargeStreamModel,
    real_stream_mode: StreamMode,
    real_stream_view_mode: StreamViewMode,
    real_stream_offset: u64,
    real_stream_limit: usize,
    real_stream_key: Option<RealStreamKey>,
    real_stream_job: Option<RealStreamJob>,
    real_stream_chunk: Option<StreamChunk>,
    real_stream_error: Option<String>,
    decoded_stream_cache: StreamChunkCache<RealStreamKey>,
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
    render_cache: RenderResultCache<RealRenderKey>,
    object_search_query: String,
    object_search_result: Option<ObjectSearchResult>,
    object_search_error: Option<String>,
    object_search_job: Option<RealObjectSearchJob>,
    text_search_query: String,
    text_search_result: Option<TextSearchResult>,
    text_search_error: Option<String>,
    text_search_job: Option<RealTextSearchJob>,
    text_search_cache: TextPageCache,
    selected_text_hit: Option<TextSearchHit>,
    diagnostic_min_severity: Option<DiagnosticSeverity>,
    diagnostic_code_filter: String,
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
        let start_empty = options.start_empty_when_no_pdf && options.pdf_path.is_none();
        let state = if start_empty {
            Err("No PDF open".to_string())
        } else {
            open_app_state(options.pdf_path.as_deref(), None)
        };
        let tree = TreeModel::from_state(&state, options.pdf_path.is_some());
        let (real_detail, real_detail_error) = load_initial_real_detail(&state, &tree);
        let (real_pages, real_pages_error) = load_initial_real_pages(&state, &tree);
        let recent_files_path = options
            .recent_files_path
            .clone()
            .unwrap_or_else(default_recent_files_path);
        let mut recent_pdf_paths = load_recent_pdf_paths_from(&recent_files_path);
        let render_page_index = 0;
        let render_zoom = DEFAULT_RENDER_ZOOM;
        let render_rotation_degrees = 0;
        let mut status_log = if start_empty {
            empty_status_log()
        } else {
            initial_status_log(&state, &tree, options.pdf_path.as_deref())
        };
        if let Some(pages) = &real_pages {
            status_log.push(format!(
                "loaded page list {}",
                child_page_detail(pages.total, pages.items.len())
            ));
        } else if let Some(err) = &real_pages_error {
            status_log.push(format!("page list load failed: {err}"));
        }
        if state.is_ok() {
            if let Some(path) = options.pdf_path.as_deref() {
                if record_recent_pdf_path(&mut recent_pdf_paths, path) {
                    if let Err(err) =
                        save_recent_pdf_paths_to(&recent_files_path, &recent_pdf_paths)
                    {
                        status_log.push(format!("recent file save failed: {err}"));
                    }
                }
            }
        }
        let smoke_exit_after = options.smoke_exit_after;
        let mut app = Self {
            state,
            empty_workspace: start_empty,
            launched_at: Instant::now(),
            smoke_exit_after,
            recent_files_path,
            recent_pdf_paths,
            open_pdf_dialog_open: false,
            open_pdf_path_input: options.pdf_path.unwrap_or_default(),
            open_pdf_password_input: String::new(),
            open_pdf_error: None,
            open_pdf_job: None,
            tree,
            stream: LargeStreamModel::default(),
            real_stream_mode: StreamMode::Raw,
            real_stream_view_mode: StreamViewMode::Hex,
            real_stream_offset: 0,
            real_stream_limit: HEX_WINDOW_BYTES,
            real_stream_key: None,
            real_stream_job: None,
            real_stream_chunk: None,
            real_stream_error: None,
            decoded_stream_cache: StreamChunkCache::new(
                DECODED_STREAM_CACHE_MAX_ITEMS,
                DECODED_STREAM_CACHE_MAX_BYTES,
            ),
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
            render_cache: RenderResultCache::new(RENDER_CACHE_MAX_ITEMS, RENDER_CACHE_MAX_BYTES),
            object_search_query: String::new(),
            object_search_result: None,
            object_search_error: None,
            object_search_job: None,
            text_search_query: String::new(),
            text_search_result: None,
            text_search_error: None,
            text_search_job: None,
            text_search_cache: TextPageCache::new(
                TEXT_SEARCH_CACHE_MAX_PAGES,
                TEXT_SEARCH_CACHE_MAX_BYTES,
            ),
            selected_text_hit: None,
            diagnostic_min_severity: None,
            diagnostic_code_filter: String::new(),
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

    fn follow_object_search_hit(&mut self, hit: &ObjectSearchHit) {
        if self.tree.is_real() {
            if let Some(doc) = self
                .state
                .as_ref()
                .ok()
                .and_then(|state| state.panels.summary.as_ref())
                .map(|summary| summary.doc.clone())
            {
                if let Some(row) = self.tree.ensure_real_search_hit_row(doc, hit) {
                    self.follow_reference(row);
                    self.status_log.push(format!(
                        "opened object search hit {}",
                        object_search_hit_summary(hit)
                    ));
                    return;
                }
            }
        } else if let Some(row) = virtual_search_hit_row(hit, self.tree.row_count()) {
            self.follow_reference(row);
            self.status_log.push(format!(
                "opened object search hit {}",
                object_search_hit_summary(hit)
            ));
            return;
        }

        self.status_log.push(format!(
            "object search hit is not navigable: {}",
            object_search_hit_summary(hit)
        ));
    }

    fn run_object_search(&mut self) {
        let query = self.object_search_query.trim().to_string();
        if query.is_empty() {
            self.cancel_object_search_job();
            self.object_search_result = None;
            self.object_search_error = None;
            return;
        }

        if let Some(job) = self.object_search_job.take() {
            job.cancel.cancel();
        }

        let Ok(state) = self.state.as_ref() else {
            self.object_search_result = None;
            self.object_search_error = Some("document is not open".to_string());
            return;
        };
        let cancel = match CancelToken::new() {
            Ok(cancel) => Arc::new(cancel),
            Err(err) => {
                self.object_search_result = None;
                self.object_search_error = Some(err.message);
                return;
            }
        };
        let request = ObjectSearchRequest {
            query: query.clone(),
            root: None,
            child_page_size: OBJECT_SEARCH_CHILD_PAGE_SIZE,
            max_child_pages_per_node: OBJECT_SEARCH_MAX_CHILD_PAGES,
            max_depth: OBJECT_SEARCH_MAX_DEPTH,
            max_nodes: OBJECT_SEARCH_MAX_NODES,
            max_results: OBJECT_SEARCH_MAX_RESULTS,
            inspect_details: false,
        };
        let session = state.session.clone();
        let worker_cancel = Arc::clone(&cancel);
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = session
                .run_task(|document| {
                    search_objects_with_cancel(document, &request, worker_cancel.as_ref())
                })
                .map_err(|err| err.message);
            let _ = sender.send(RealObjectSearchJobOutput { query, result });
        });

        self.object_search_result = None;
        self.object_search_error = None;
        self.object_search_job = Some(RealObjectSearchJob {
            query: self.object_search_query.trim().to_string(),
            cancel,
            receiver,
        });
        self.status_log.push(format!(
            "queued object search {:?}",
            self.object_search_query.trim()
        ));
    }

    fn cancel_object_search_job(&mut self) {
        if let Some(job) = self.object_search_job.take() {
            job.cancel.cancel();
            self.object_search_error = Some("object search cancelled".to_string());
            self.status_log
                .push(format!("cancelled object search {:?}", job.query));
        }
    }

    fn poll_object_search_job(&mut self) {
        let Some(polled) =
            self.object_search_job
                .as_ref()
                .and_then(|job| match job.receiver.try_recv() {
                    Ok(output) => Some(Ok(output)),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => Some(Err(job.query.clone())),
                })
        else {
            return;
        };

        self.object_search_job = None;
        match polled {
            Ok(output) => match output.result {
                Ok(result) => {
                    self.status_log.push(format!(
                        "object search {:?}: {} hits across {} nodes{}",
                        output.query,
                        result.hits.len(),
                        result.searched_nodes,
                        if result.truncated { " (truncated)" } else { "" }
                    ));
                    self.object_search_result = Some(result);
                    self.object_search_error = None;
                }
                Err(err) => {
                    self.object_search_result = None;
                    self.object_search_error = Some(err.clone());
                    self.status_log
                        .push(format!("object search {:?} failed: {err}", output.query));
                }
            },
            Err(query) => {
                self.object_search_result = None;
                self.object_search_error = Some("object search worker disconnected".to_string());
                self.status_log
                    .push(format!("object search {:?} worker disconnected", query));
            }
        }
    }

    fn start_text_search(&mut self) {
        let query = self.text_search_query.trim().to_string();
        if query.is_empty() {
            self.cancel_text_search_job();
            self.text_search_result = None;
            self.text_search_error = None;
            self.selected_text_hit = None;
            return;
        }

        if let Some(job) = self.text_search_job.take() {
            job.cancel.cancel();
        }

        let page_count = self.page_count();
        if page_count == 0 {
            self.text_search_result = None;
            self.text_search_error = Some("document has no pages".to_string());
            return;
        }

        let Ok(state) = self.state.as_ref() else {
            self.text_search_result = None;
            self.text_search_error = Some("document is not open".to_string());
            return;
        };
        let cancel = match CancelToken::new() {
            Ok(cancel) => Arc::new(cancel),
            Err(err) => {
                self.text_search_result = None;
                self.text_search_error = Some(err.message);
                return;
            }
        };

        let request = TextSearchRequest {
            query: query.clone(),
            max_pages: TEXT_SEARCH_MAX_PAGES,
            max_results: TEXT_SEARCH_MAX_RESULTS,
            max_chars_per_page: TEXT_SEARCH_MAX_CHARS_PER_PAGE,
            max_blocks_per_page: TEXT_SEARCH_MAX_BLOCKS_PER_PAGE,
            ..TextSearchRequest::new(query.clone())
        };
        let session = state.session.clone();
        let worker_cancel = Arc::clone(&cancel);
        let mut cache = self.text_search_cache.clone();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = session
                .run_task(|document| {
                    search_text_with_cache(page_count, &mut cache, &request, |text_request| {
                        document
                            .extract_text_with_cancel_token(text_request, worker_cancel.as_ref())
                    })
                })
                .map(|result| (result, cache))
                .map_err(|err| err.message);
            let _ = sender.send(RealTextSearchJobOutput { query, result });
        });

        self.text_search_result = None;
        self.text_search_error = None;
        self.selected_text_hit = None;
        self.text_search_job = Some(RealTextSearchJob {
            query: self.text_search_query.trim().to_string(),
            cancel,
            receiver,
        });
        self.status_log.push(format!(
            "queued text search {:?} across up to {} pages",
            self.text_search_query.trim(),
            page_count.min(TEXT_SEARCH_MAX_PAGES)
        ));
    }

    fn cancel_text_search_job(&mut self) {
        if let Some(job) = self.text_search_job.take() {
            job.cancel.cancel();
            self.text_search_error = Some("text search cancelled".to_string());
            self.status_log
                .push(format!("cancelled text search {:?}", job.query));
        }
    }

    fn poll_text_search_job(&mut self) {
        let Some(polled) =
            self.text_search_job
                .as_ref()
                .and_then(|job| match job.receiver.try_recv() {
                    Ok(output) => Some(Ok(output)),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => Some(Err(job.query.clone())),
                })
        else {
            return;
        };

        self.text_search_job = None;
        match polled {
            Ok(output) => match output.result {
                Ok((result, cache)) => {
                    self.status_log.push(format!(
                        "text search {:?}: {} hits across {} pages{}",
                        output.query,
                        result.hits.len(),
                        result.searched_pages,
                        if result.truncated { " (truncated)" } else { "" }
                    ));
                    self.text_search_cache = cache;
                    self.text_search_result = Some(result);
                    self.text_search_error = None;
                }
                Err(err) => {
                    self.text_search_result = None;
                    self.text_search_error = Some(err.clone());
                    self.status_log
                        .push(format!("text search {:?} failed: {err}", output.query));
                }
            },
            Err(query) => {
                self.text_search_result = None;
                self.text_search_error = Some("text search worker disconnected".to_string());
                self.status_log
                    .push(format!("text search {:?} worker disconnected", query));
            }
        }
    }

    fn follow_text_search_hit(&mut self, hit: &TextSearchHit) {
        self.selected_text_hit = Some(hit.clone());
        self.set_render_page(hit.page_index);
        self.status_log.push(format!(
            "opened text search hit page {} span {}",
            hit.page_index + 1,
            hit.span_index
        ));
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
        if let Some(job) = self.real_stream_job.take() {
            job.cancel.cancel();
        }
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
        if self.real_stream_key == Some(key)
            && (self.real_stream_chunk.is_some() || self.real_stream_job.is_some())
        {
            return;
        }
        if let Some(job) = self.real_stream_job.take() {
            job.cancel.cancel();
        }
        self.real_stream_key = Some(key);
        self.real_stream_chunk = None;
        self.real_stream_error = None;

        if key.mode == StreamMode::Decoded {
            if let Some(chunk) = self.decoded_stream_cache.get(&key) {
                self.real_stream_chunk = Some(chunk);
                self.status_log.push(format!(
                    "reused cached decoded stream chunk {} {} R @ {}",
                    key.object.num, key.object.gen, key.offset
                ));
                return;
            }
        }

        let Ok(state) = self.state.as_ref() else {
            self.real_stream_error = Some("document is not open".to_string());
            return;
        };
        let cancel = match CancelToken::new() {
            Ok(cancel) => Arc::new(cancel),
            Err(err) => {
                self.real_stream_error = Some(err.message);
                return;
            }
        };
        let session = state.session.clone();
        let worker_cancel = Arc::clone(&cancel);
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = session
                .run_task(|document| {
                    document.stream_load_with_cancel_token(
                        key.object,
                        key.mode,
                        key.offset,
                        key.limit,
                        worker_cancel.as_ref(),
                    )
                })
                .map_err(|err| err.message);
            let _ = sender.send(RealStreamJobOutput { key, result });
        });
        self.real_stream_job = Some(RealStreamJob {
            key,
            cancel,
            receiver,
        });
        self.status_log.push(format!(
            "queued {} stream chunk {} {} R @ {}",
            stream_mode_label(key.mode),
            object.num,
            object.gen,
            key.offset
        ));
    }

    fn cancel_real_stream_job(&mut self) {
        if let Some(job) = self.real_stream_job.take() {
            job.cancel.cancel();
            self.real_stream_chunk = None;
            self.real_stream_error = Some("stream chunk load cancelled".to_string());
            self.status_log.push(format!(
                "cancelled {} stream chunk {} {} R @ {}",
                stream_mode_label(job.key.mode),
                job.key.object.num,
                job.key.object.gen,
                job.key.offset
            ));
        }
    }

    fn poll_real_stream_job(&mut self) {
        let Some(polled) =
            self.real_stream_job
                .as_ref()
                .and_then(|job| match job.receiver.try_recv() {
                    Ok(output) => Some(Ok(output)),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => Some(Err(job.key)),
                })
        else {
            return;
        };

        self.real_stream_job = None;
        match polled {
            Ok(output) => {
                if self.real_stream_key != Some(output.key) {
                    self.status_log.push(format!(
                        "discarded stale {} stream chunk {} {} R @ {}",
                        stream_mode_label(output.key.mode),
                        output.key.object.num,
                        output.key.object.gen,
                        output.key.offset
                    ));
                    return;
                }
                match output.result {
                    Ok(chunk) => {
                        self.status_log.push(format!(
                            "loaded {} stream chunk {} {} R @ {} ({} bytes{})",
                            stream_mode_label(chunk.mode),
                            output.key.object.num,
                            output.key.object.gen,
                            chunk.offset,
                            chunk.bytes.len(),
                            if chunk.truncated { ", truncated" } else { "" }
                        ));
                        if output.key.mode == StreamMode::Decoded {
                            self.decoded_stream_cache.insert(output.key, chunk.clone());
                        }
                        self.real_stream_chunk = Some(chunk);
                        self.real_stream_error = None;
                    }
                    Err(err) => {
                        self.real_stream_chunk = None;
                        self.real_stream_error = Some(err.clone());
                        self.status_log.push(format!(
                            "{} stream chunk {} {} R failed: {err}",
                            stream_mode_label(output.key.mode),
                            output.key.object.num,
                            output.key.object.gen
                        ));
                    }
                }
            }
            Err(key) => {
                if self.real_stream_key == Some(key) {
                    self.real_stream_chunk = None;
                    self.real_stream_error = Some("stream worker disconnected".to_string());
                }
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

        if let Some(render) = self.render_cache.get(&key) {
            self.real_render_texture = None;
            self.real_render = Some(render);
            self.status_log.push(format!(
                "reused cached page {} @ {:.0}% rot {} render",
                key.page_index + 1,
                key.zoom() * 100.0,
                key.rotation_degrees
            ));
            return;
        }

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

    fn cancel_real_render_job(&mut self) {
        if let Some(job) = self.real_render_job.take() {
            job.cancel.cancel();
            self.real_render = None;
            self.real_render_texture = None;
            self.real_render_error = Some("page render cancelled".to_string());
            self.status_log.push(format!(
                "cancelled page {} @ {:.0}% rot {} render",
                job.key.page_index + 1,
                job.key.zoom() * 100.0,
                job.key.rotation_degrees
            ));
        }
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
                        self.render_cache.insert(output.key, render.clone());
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
        if self.empty_workspace {
            return (
                "No PDF".to_string(),
                "pages -".to_string(),
                "xref -".to_string(),
            );
        }
        if let Ok(state) = &self.state {
            if let Some(summary) = &state.panels.summary {
                return (
                    display_file_chip_label(&summary.file_path),
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

    fn window_title(&self) -> String {
        if let Some(job) = &self.open_pdf_job {
            return format!(
                "Opening {} - {APP_TITLE}",
                display_file_chip_label(&job.path)
            );
        }
        if self.empty_workspace {
            return APP_TITLE.to_string();
        }
        if let Ok(state) = &self.state {
            if let Some(summary) = &state.panels.summary {
                return format!(
                    "{} - {APP_TITLE}",
                    display_file_chip_label(&summary.file_path)
                );
            }
        }
        format!("fake.pdf - {APP_TITLE}")
    }

    fn breadcrumb_label(&self) -> String {
        if self.empty_workspace {
            return "No document".to_string();
        }
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

    fn diagnostics_filter(&self) -> DiagnosticFilter {
        DiagnosticFilter {
            min_severity: self.diagnostic_min_severity.clone(),
            code_query: Some(self.diagnostic_code_filter.clone()),
        }
    }

    fn diagnostics_model(&self) -> DocumentDiagnostics {
        DocumentDiagnostics::new(self.collected_diagnostics())
    }

    fn filtered_diagnostics(&self) -> Vec<DiagnosticSummary> {
        self.diagnostics_model()
            .filtered(&self.diagnostics_filter())
    }

    fn collected_diagnostics(&self) -> Vec<DiagnosticSummary> {
        let mut diagnostics = Vec::new();
        if let Ok(state) = &self.state {
            if let Some(summary) = &state.panels.summary {
                diagnostics.extend(summary.diagnostics.clone());
            }
        }
        if let Some(detail) = &self.real_detail {
            diagnostics.extend(detail.diagnostics.clone());
        }
        if let Some(chunk) = &self.real_stream_chunk {
            diagnostics.extend(chunk.decode_diagnostics.clone());
        }
        if let Some(render) = &self.real_render {
            diagnostics.extend(render.diagnostics.clone());
        }
        if let Some(result) = &self.text_search_result {
            diagnostics.extend(result.page_errors.iter().map(|error| DiagnosticSummary {
                severity: DiagnosticSeverity::Warning,
                code: DiagnosticCode::Unknown,
                message: format!(
                    "text extraction failed on page {}: {}",
                    error.page_index + 1,
                    error.message
                ),
                node: None,
                page_index: Some(error.page_index),
                object: None,
            }));
        }
        diagnostics
    }

    fn copy_diagnostics_json(&mut self, ctx: &egui::Context) {
        let diagnostics = self.filtered_diagnostics();
        let json = diagnostics_payload_to_json_string(&diagnostics);
        ctx.copy_text(json);
        self.status_log.push(format!(
            "copied diagnostics JSON with {} filtered diagnostics",
            diagnostics.len()
        ));
    }

    fn copy_markdown_report(&mut self, ctx: &egui::Context) {
        let diagnostics = self.filtered_diagnostics();
        let report = build_markdown_report(&MarkdownReportInput {
            document: self
                .state
                .as_ref()
                .ok()
                .and_then(|state| state.panels.summary.as_ref()),
            selected_object: self.real_detail.as_ref(),
            diagnostics: &diagnostics,
            object_search: self.object_search_result.as_ref(),
            text_search: self.text_search_result.as_ref(),
            max_diagnostics: REPORT_DIAGNOSTIC_LIMIT,
            max_object_hits: REPORT_SEARCH_HIT_LIMIT,
            max_text_hits: REPORT_SEARCH_HIT_LIMIT,
            max_bytes: MARKDOWN_REPORT_LIMIT_BYTES,
        });
        ctx.copy_text(report.text.clone());
        self.status_log.push(format!(
            "copied Markdown diagnostic report{}",
            if report.truncated { " (truncated)" } else { "" }
        ));
        self.copied_excerpt = Some(report);
    }
}

fn open_app_state(pdf_path: Option<&str>, password: Option<&str>) -> Result<AppState, String> {
    if let Some(path) = pdf_path {
        #[cfg(feature = "real-mupdf")]
        {
            return AppState::new_real_path_with_password(path, password)
                .map_err(|err| err.message);
        }
        #[cfg(not(feature = "real-mupdf"))]
        {
            let _ = password;
            let _ = path;
            return Err(
                "`--pdf` requires building pdbg-app with `--features real-mupdf`".to_string(),
            );
        }
    }
    let _ = password;
    AppState::new_headless().map_err(|err| err.message)
}

fn choose_pdf_file() -> Option<String> {
    rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .pick_file()
        .map(|path| path.to_string_lossy().into_owned())
}

fn default_recent_files_path() -> PathBuf {
    if let Some(path) = std::env::var_os("PDBG_RECENT_FILES_PATH").filter(|path| !path.is_empty()) {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        return PathBuf::from(path).join("pdbg").join("recent-files.txt");
    }
    if let Some(path) = std::env::var_os("HOME").filter(|path| !path.is_empty()) {
        return PathBuf::from(path)
            .join(".config")
            .join("pdbg")
            .join("recent-files.txt");
    }
    std::env::temp_dir().join("pdbg").join("recent-files.txt")
}

fn load_recent_pdf_paths_from(path: &Path) -> Vec<String> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut seen = HashSet::new();
    let mut paths = Vec::new();
    for line in contents.lines() {
        let entry = line.trim();
        if !is_recent_path_entry_safe(entry) {
            continue;
        }
        if seen.insert(entry.to_string()) {
            paths.push(entry.to_string());
        }
        if paths.len() >= RECENT_PDF_MAX_ITEMS {
            break;
        }
    }
    paths
}

fn save_recent_pdf_paths_to(path: &Path, paths: &[String]) -> io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }

    let mut out = String::new();
    for path in paths
        .iter()
        .filter(|path| is_recent_path_entry_safe(path))
        .take(RECENT_PDF_MAX_ITEMS)
    {
        out.push_str(path);
        out.push('\n');
    }

    let tmp_path = unique_recent_tmp_path(path);
    fs::write(&tmp_path, out)?;
    match fs::rename(&tmp_path, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(&tmp_path);
            Err(err)
        }
    }
}

fn unique_recent_tmp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("recent-files");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let tmp_name = format!("{file_name}.{}.{}.tmp", std::process::id(), nonce);
    path.with_file_name(tmp_name)
}

fn record_recent_pdf_path(paths: &mut Vec<String>, path: &str) -> bool {
    let Some(path) = normalize_recent_pdf_path(path) else {
        return false;
    };
    let before = paths.clone();
    paths.retain(|entry| entry != &path);
    paths.insert(0, path);
    paths.truncate(RECENT_PDF_MAX_ITEMS);
    *paths != before
}

fn normalize_recent_pdf_path(path: &str) -> Option<String> {
    let path = path.trim();
    if !is_recent_path_entry_safe(path) {
        return None;
    }
    let path = Path::new(path);
    let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = normalized.to_string_lossy().into_owned();
    is_recent_path_entry_safe(&text).then_some(text)
}

fn is_recent_path_entry_safe(path: &str) -> bool {
    !path.trim().is_empty() && !path.contains('\n') && !path.contains('\r')
}

fn is_pdf_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

fn load_initial_real_detail(
    state: &Result<AppState, String>,
    tree: &TreeModel,
) -> (Option<ObjectDetail>, Option<String>) {
    if !tree.is_real() {
        return (None, None);
    }
    match state {
        Ok(state) => load_initial_real_detail_from_state(state, tree),
        Err(err) => (None, Some(err.clone())),
    }
}

fn load_initial_real_detail_from_state(
    state: &AppState,
    tree: &TreeModel,
) -> (Option<ObjectDetail>, Option<String>) {
    let TreeModel::Real(tree) = tree else {
        return (None, None);
    };
    let Some(summary) = tree.summary(0) else {
        return (None, None);
    };
    match load_object_detail(state, &summary.id) {
        Ok(detail) => (Some(detail), None),
        Err(err) => (None, Some(err)),
    }
}

fn load_initial_real_pages(
    state: &Result<AppState, String>,
    tree: &TreeModel,
) -> (Option<ChildPage<ObjectSummary>>, Option<String>) {
    if !tree.is_real() {
        return (None, None);
    }
    match state {
        Ok(state) => load_initial_real_pages_from_state(state, tree),
        Err(err) => (None, Some(err.clone())),
    }
}

fn load_initial_real_pages_from_state(
    state: &AppState,
    tree: &TreeModel,
) -> (Option<ChildPage<ObjectSummary>>, Option<String>) {
    let TreeModel::Real(tree) = tree else {
        return (None, None);
    };
    let Some(page_root) = tree.page_root_summary() else {
        return (None, None);
    };

    match state.session.run_task(|document| {
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

fn initial_status_log(
    state: &Result<AppState, String>,
    tree: &TreeModel,
    pdf_path: Option<&str>,
) -> Vec<String> {
    match (state, tree, pdf_path) {
        (Ok(_), TreeModel::Real(tree), Some(path)) => vec![
            format!("real MuPDF opened {}", display_file_chip_label(path)),
            format!("loaded bounded root page: {}", tree.row_count_label()),
            "real stream bytes available as bounded raw/decoded chunks".to_string(),
        ],
        (Err(err), _, Some(path)) => vec![
            format!("failed to open {}", display_file_chip_label(path)),
            err.clone(),
        ],
        _ => vec![
            "fake shim opened fake.pdf".to_string(),
            "virtual object tree uses generated rows".to_string(),
            "large stream pane uses generated bytes".to_string(),
        ],
    }
}

fn open_pdf_worker_result(
    path: String,
    password: Option<String>,
) -> Result<OpenPdfJobResult, String> {
    let state = open_app_state(Some(&path), password.as_deref())?;
    if state
        .panels
        .summary
        .as_ref()
        .is_some_and(|summary| summary.needs_password)
    {
        return Ok(OpenPdfJobResult::NeedsPassword);
    }
    Ok(OpenPdfJobResult::Opened(Box::new(build_opened_pdf_model(
        state, &path,
    ))))
}

fn build_opened_pdf_model(state: AppState, path: &str) -> OpenedPdfModel {
    let tree = TreeModel::from_app_state(&state, true);
    let (real_detail, real_detail_error) = load_initial_real_detail_from_state(&state, &tree);
    let (real_pages, real_pages_error) = load_initial_real_pages_from_state(&state, &tree);
    let mut status_log = opened_pdf_status_log(&tree, path);
    if let Some(pages) = &real_pages {
        status_log.push(format!(
            "loaded page list {}",
            child_page_detail(pages.total, pages.items.len())
        ));
    } else if let Some(err) = &real_pages_error {
        status_log.push(format!("page list load failed: {err}"));
    }

    OpenedPdfModel {
        state,
        tree,
        real_detail,
        real_detail_error,
        real_pages,
        real_pages_error,
        status_log,
    }
}

fn opened_pdf_status_log(tree: &TreeModel, path: &str) -> Vec<String> {
    match tree {
        TreeModel::Real(tree) => vec![
            format!("real MuPDF opened {}", display_file_chip_label(path)),
            format!("loaded bounded root page: {}", tree.row_count_label()),
            "real stream bytes available as bounded raw/decoded chunks".to_string(),
        ],
        TreeModel::Virtual(_) => vec![
            format!("opened {}", display_file_chip_label(path)),
            "virtual object tree uses generated rows".to_string(),
            "large stream pane uses generated bytes".to_string(),
        ],
    }
}

fn empty_status_log() -> Vec<String> {
    vec![
        "No PDF open".to_string(),
        "Open PDF or choose a recent file".to_string(),
    ]
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

fn display_file_chip_label(path: &str) -> String {
    escape_pdf_text(
        &file_chip_label(path),
        EgressFormat::PlainText,
        PATH_DISPLAY_MAX_BYTES,
    )
    .text
}

fn display_path_hover(path: &str) -> String {
    escape_pdf_text(path, EgressFormat::PlainText, PATH_DISPLAY_MAX_BYTES).text
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

fn object_search_status_label(
    result: Option<&ObjectSearchResult>,
    error: Option<&str>,
    running: bool,
) -> String {
    if running {
        return "searching".to_string();
    }
    if error.is_some() {
        return "failed".to_string();
    }
    match result {
        Some(result) => format!(
            "{} hits / {} nodes{}",
            result.hits.len(),
            result.searched_nodes,
            if result.truncated { " / truncated" } else { "" }
        ),
        None => "bounded lazy search".to_string(),
    }
}

fn object_search_field_label(field: ObjectSearchField) -> &'static str {
    match field {
        ObjectSearchField::ObjectNumber => "object",
        ObjectSearchField::DictionaryKey => "key",
        ObjectSearchField::NameObject => "name",
        ObjectSearchField::ScalarPreview => "scalar",
        ObjectSearchField::Label => "label",
    }
}

fn object_search_hit_summary(hit: &ObjectSearchHit) -> String {
    let target = hit
        .object
        .map(object_ref_text)
        .unwrap_or_else(|| hit.label.clone());
    let excerpt = hit.excerpt.trim();
    if excerpt.is_empty() {
        format!(
            "{target}  {}  {}",
            object_search_field_label(hit.matched_field),
            hit.label
        )
    } else {
        format!(
            "{target}  {}  {excerpt}",
            object_search_field_label(hit.matched_field)
        )
    }
}

fn node_label_for_hit(hit: &ObjectSearchHit) -> String {
    hit.node
        .as_ref()
        .map(node_breadcrumb)
        .unwrap_or_else(|| hit.label.clone())
}

fn virtual_search_hit_row(hit: &ObjectSearchHit, row_count: usize) -> Option<usize> {
    let row = usize::try_from(hit.object?.num).ok()?;
    (row < row_count).then_some(row)
}

fn object_ref_text(object: ObjectId) -> String {
    format!("{} {} R", object.num, object.gen)
}

fn text_search_status_label(
    result: Option<&TextSearchResult>,
    error: Option<&str>,
    running: bool,
    cached_pages: usize,
    cached_bytes: usize,
) -> String {
    if running {
        return "running".to_string();
    }
    if error.is_some() {
        return format!("failed / {cached_pages} cached");
    }
    match result {
        Some(result) => format!(
            "{} hits / {} pages{} / {} cached / {} KiB",
            result.hits.len(),
            result.searched_pages,
            if result.truncated { " / truncated" } else { "" },
            cached_pages,
            cached_bytes / 1024
        ),
        None => format!("{cached_pages} cached / {} KiB", cached_bytes / 1024),
    }
}

fn text_search_hit_summary(hit: &TextSearchHit) -> String {
    let excerpt = escape_pdf_text(&hit.excerpt, EgressFormat::PlainText, COPY_LIMIT_BYTES)
        .text
        .chars()
        .map(|ch| {
            if matches!(ch, '\n' | '\r' | '\t') {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();
    format!("p{} s{}  {}", hit.page_index + 1, hit.span_index, excerpt)
}

fn text_search_hit_hover(hit: &TextSearchHit) -> String {
    let trust = if hit.untrusted {
        "untrusted"
    } else {
        "trusted"
    };
    match &hit.bbox {
        Some(bbox) => format!(
            "page {} span {} {trust} bbox {:.1},{:.1} {:.1}x{:.1}",
            hit.page_index + 1,
            hit.span_index,
            bbox.x,
            bbox.y,
            bbox.width,
            bbox.height
        ),
        None => format!(
            "page {} span {} {trust}",
            hit.page_index + 1,
            hit.span_index
        ),
    }
}

fn text_hits_same_position(left: &TextSearchHit, right: &TextSearchHit) -> bool {
    left.page_index == right.page_index
        && left.span_index == right.span_index
        && left.excerpt == right.excerpt
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
        self.handle_dropped_pdf_files(&ctx);
        self.poll_open_pdf_job();
        self.poll_real_stream_job();
        self.poll_real_render_job();
        self.poll_object_search_job();
        self.poll_text_search_job();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
        if self.open_pdf_job.is_some()
            || self.real_stream_job.is_some()
            || self.real_render_job.is_some()
            || self.object_search_job.is_some()
            || self.text_search_job.is_some()
        {
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

        self.draw_open_pdf_dialog(&ctx);

        if self
            .smoke_exit_after
            .is_some_and(|duration| self.launched_at.elapsed() >= duration)
        {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl GuiShellApp {
    fn draw_recent_file_list(
        &self,
        ui: &mut egui::Ui,
        id_salt: &'static str,
        max_height: f32,
    ) -> Option<String> {
        if self.recent_pdf_paths.is_empty() {
            ui.label(RichText::new("No recent files").color(PdbgTheme::MUTED));
            return None;
        }

        let mut path_to_open = None;
        ScrollArea::vertical()
            .id_salt(id_salt)
            .max_height(max_height)
            .show(ui, |ui| {
                for path in self.recent_pdf_paths.clone() {
                    if ui
                        .selectable_label(false, display_file_chip_label(&path))
                        .on_hover_text(display_path_hover(&path))
                        .clicked()
                    {
                        path_to_open = Some(path);
                    }
                }
            });
        path_to_open
    }

    fn handle_dropped_pdf_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        let Some(path) = dropped
            .into_iter()
            .filter_map(|file| file.path)
            .find(|path| is_pdf_path(path))
        else {
            return;
        };
        self.open_pdf_from_path(path.to_string_lossy().into_owned());
    }

    fn draw_open_pdf_dialog(&mut self, ctx: &egui::Context) {
        if !self.open_pdf_dialog_open {
            return;
        }

        let mut window_open = self.open_pdf_dialog_open;
        let mut path_to_open = None;
        egui::Window::new("Open PDF")
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .open(&mut window_open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let opening = self.open_pdf_job.is_some();
                    let response = ui.add_enabled(
                        !opening,
                        TextEdit::singleline(&mut self.open_pdf_path_input)
                            .desired_width(380.0)
                            .hint_text("/path/to/file.pdf"),
                    );
                    if ui
                        .add_enabled(!opening, egui::Button::new("Choose..."))
                        .clicked()
                    {
                        if let Some(path) = choose_pdf_file() {
                            self.open_pdf_path_input = path;
                        }
                    }
                    if !opening
                        && response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        path_to_open = Some(self.open_pdf_path_input.clone());
                    }
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let opening = self.open_pdf_job.is_some();
                    ui.label(RichText::new("Password").color(PdbgTheme::MUTED));
                    let response = ui.add_enabled(
                        !opening,
                        TextEdit::singleline(&mut self.open_pdf_password_input)
                            .desired_width(380.0)
                            .password(true),
                    );
                    if !opening
                        && response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        path_to_open = Some(self.open_pdf_path_input.clone());
                    }
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if self.open_pdf_job.is_some() {
                        ui.label(RichText::new("Opening PDF...").color(PdbgTheme::MUTED));
                        if ui.button("Cancel open").clicked() {
                            self.cancel_open_pdf_job();
                            self.open_pdf_dialog_open = false;
                            self.open_pdf_password_input.clear();
                        }
                    } else {
                        let can_open = !self.open_pdf_path_input.trim().is_empty();
                        if ui
                            .add_enabled(can_open, egui::Button::new("Open"))
                            .clicked()
                        {
                            path_to_open = Some(self.open_pdf_path_input.clone());
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.cancel_open_pdf_job();
                        self.open_pdf_dialog_open = false;
                        self.open_pdf_password_input.clear();
                    }
                });

                if let Some(err) = &self.open_pdf_error {
                    ui.add_space(8.0);
                    ui.colored_label(PdbgTheme::ERROR_FG, err);
                }

                if !self.recent_pdf_paths.is_empty() {
                    ui.add_space(12.0);
                    section_header(ui, "Recent Files", None);
                    if let Some(path) = self.draw_recent_file_list(ui, "recent_pdf_paths", 180.0) {
                        path_to_open = Some(path);
                    }
                }
            });

        if !window_open && self.open_pdf_job.is_some() {
            self.cancel_open_pdf_job();
            self.open_pdf_password_input.clear();
        }
        self.open_pdf_dialog_open = window_open && self.open_pdf_dialog_open;
        if let Some(path) = path_to_open {
            self.open_pdf_from_path(path);
        }
    }

    fn open_pdf_from_path(&mut self, path: String) {
        let path = path.trim().to_string();
        if path.is_empty() {
            self.open_pdf_error = Some("PDF path is empty".to_string());
            return;
        }

        let password_owned = self.open_pdf_password_input.trim().to_string();
        let password = (!password_owned.is_empty()).then_some(password_owned);
        if let Some(job) = self.open_pdf_job.take() {
            self.status_log.push(format!(
                "discarded pending open {}",
                display_file_chip_label(&job.path)
            ));
        }

        let worker_path = path.clone();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = open_pdf_worker_result(worker_path.clone(), password);
            let _ = sender.send(OpenPdfJobOutput {
                path: worker_path,
                result,
            });
        });

        self.open_pdf_path_input = path.clone();
        self.open_pdf_error = None;
        self.open_pdf_job = Some(OpenPdfJob {
            path: path.clone(),
            receiver,
        });
        self.status_log
            .push(format!("opening {}", display_file_chip_label(&path)));
    }

    fn cancel_open_pdf_job(&mut self) {
        if let Some(job) = self.open_pdf_job.take() {
            self.open_pdf_error = Some("open cancelled".to_string());
            self.status_log.push(format!(
                "discarded pending open {}",
                display_file_chip_label(&job.path)
            ));
        }
    }

    fn poll_open_pdf_job(&mut self) {
        let Some(polled) =
            self.open_pdf_job
                .as_ref()
                .and_then(|job| match job.receiver.try_recv() {
                    Ok(output) => Some(Ok(output)),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => Some(Err(job.path.clone())),
                })
        else {
            return;
        };

        self.open_pdf_job = None;
        match polled {
            Ok(output) => match output.result {
                Ok(OpenPdfJobResult::Opened(model)) => {
                    self.apply_opened_pdf_model(*model);
                    self.open_pdf_path_input = output.path.clone();
                    self.open_pdf_password_input.clear();
                    self.open_pdf_dialog_open = false;
                    self.open_pdf_error = None;
                    self.record_recent_pdf_path(&output.path);
                }
                Ok(OpenPdfJobResult::NeedsPassword) => {
                    self.open_pdf_path_input = output.path;
                    self.open_pdf_dialog_open = true;
                    self.open_pdf_error = Some("Password required".to_string());
                    self.status_log
                        .push("document requires a password before inspection".to_string());
                }
                Err(err) => {
                    self.open_pdf_error = Some(err.clone());
                    self.status_log.push(format!(
                        "failed to open {}: {err}",
                        display_file_chip_label(&output.path)
                    ));
                }
            },
            Err(path) => {
                self.open_pdf_error = Some("open worker disconnected".to_string());
                self.status_log.push(format!(
                    "open {} worker disconnected",
                    display_file_chip_label(&path)
                ));
            }
        }
    }

    fn apply_opened_pdf_model(&mut self, model: OpenedPdfModel) {
        self.cancel_inflight_document_jobs();

        self.state = Ok(model.state);
        self.empty_workspace = false;
        self.tree = model.tree;
        self.stream = LargeStreamModel::default();
        self.real_stream_mode = StreamMode::Raw;
        self.real_stream_view_mode = StreamViewMode::Hex;
        self.real_stream_offset = 0;
        self.real_stream_limit = HEX_WINDOW_BYTES;
        self.real_stream_key = None;
        self.real_stream_chunk = None;
        self.real_stream_error = None;
        self.decoded_stream_cache = StreamChunkCache::new(
            DECODED_STREAM_CACHE_MAX_ITEMS,
            DECODED_STREAM_CACHE_MAX_BYTES,
        );
        self.selected_row = 0;
        self.back_stack.clear();
        self.forward_stack.clear();
        self.selected_tab = InspectorTab::Object;
        self.real_detail = model.real_detail;
        self.real_detail_error = model.real_detail_error;
        self.real_pages = model.real_pages;
        self.real_pages_error = model.real_pages_error;
        self.render_page_index = 0;
        self.render_zoom = DEFAULT_RENDER_ZOOM;
        self.render_rotation_degrees = 0;
        self.real_render_key = None;
        self.real_render = None;
        self.real_render_error = None;
        self.real_render_texture = None;
        self.render_cache = RenderResultCache::new(RENDER_CACHE_MAX_ITEMS, RENDER_CACHE_MAX_BYTES);
        self.object_search_query.clear();
        self.object_search_result = None;
        self.object_search_error = None;
        self.text_search_query.clear();
        self.text_search_result = None;
        self.text_search_error = None;
        self.text_search_cache =
            TextPageCache::new(TEXT_SEARCH_CACHE_MAX_PAGES, TEXT_SEARCH_CACHE_MAX_BYTES);
        self.selected_text_hit = None;
        self.copied_excerpt = None;
        self.status_log = model.status_log;
        self.refresh_real_render();
    }

    fn cancel_inflight_document_jobs(&mut self) {
        self.open_pdf_job = None;
        if let Some(job) = self.real_stream_job.take() {
            job.cancel.cancel();
        }
        if let Some(job) = self.real_render_job.take() {
            job.cancel.cancel();
        }
        if let Some(job) = self.object_search_job.take() {
            job.cancel.cancel();
        }
        if let Some(job) = self.text_search_job.take() {
            job.cancel.cancel();
        }
    }

    fn record_recent_pdf_path(&mut self, path: &str) {
        if !record_recent_pdf_path(&mut self.recent_pdf_paths, path) {
            return;
        }
        if let Err(err) = save_recent_pdf_paths_to(&self.recent_files_path, &self.recent_pdf_paths)
        {
            self.status_log
                .push(format!("recent file save failed: {err}"));
        }
    }

    fn draw_top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("pdbg")
                    .strong()
                    .size(15.0)
                    .color(PdbgTheme::TOP_BAR_TEXT),
            );
            ui.add_space(8.0);
            if top_bar_button(ui, "Open PDF...", true).clicked() {
                self.open_pdf_dialog_open = true;
                self.open_pdf_error = None;
            }
            if !self.recent_pdf_paths.is_empty() {
                let mut recent_to_open = None;
                ui.menu_button(
                    RichText::new("Recent")
                        .size(12.0)
                        .color(PdbgTheme::TOP_BAR_TEXT),
                    |ui| {
                        for path in self.recent_pdf_paths.clone() {
                            if ui
                                .button(display_file_chip_label(&path))
                                .on_hover_text(display_path_hover(&path))
                                .clicked()
                            {
                                recent_to_open = Some(path);
                                ui.close();
                            }
                        }
                    },
                );
                if let Some(path) = recent_to_open {
                    self.open_pdf_from_path(path);
                }
            }
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
        if self.empty_workspace {
            self.draw_empty_tree_panel(ui);
            return;
        }

        self.draw_object_search(ui);
        ui.add_space(8.0);
        self.draw_text_search(ui);
        ui.add_space(8.0);
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

    fn draw_empty_tree_panel(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "Open PDF", Some("No document"));
        section_frame().show(ui, |ui| {
            if ui.button("Open PDF...").clicked() {
                self.open_pdf_dialog_open = true;
                self.open_pdf_error = None;
            }
            ui.add_space(8.0);
            section_header(ui, "Recent Files", None);
            if let Some(path) = self.draw_recent_file_list(ui, "empty_tree_recent_paths", 220.0) {
                self.open_pdf_from_path(path);
            }
        });
    }

    fn draw_object_search(&mut self, ui: &mut egui::Ui) {
        let status = object_search_status_label(
            self.object_search_result.as_ref(),
            self.object_search_error.as_deref(),
            self.object_search_job.is_some(),
        );
        section_header(ui, "Object Search", Some(&status));

        let mut run_search = false;
        let mut cancel_search = false;
        let mut clear_search = false;
        section_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                let response = ui.add_enabled(
                    self.object_search_job.is_none(),
                    TextEdit::singleline(&mut self.object_search_query)
                        .desired_width(f32::INFINITY)
                        .hint_text("object, key, name, scalar"),
                );
                run_search |=
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if self.object_search_job.is_some() {
                    cancel_search |= ui.button("Cancel").clicked();
                } else {
                    run_search |= ui.button("Search").clicked();
                }
                clear_search |= ui.button("Clear").clicked();
            });
        });

        if clear_search {
            self.cancel_object_search_job();
            self.object_search_query.clear();
            self.object_search_result = None;
            self.object_search_error = None;
        } else if cancel_search {
            self.cancel_object_search_job();
        } else if run_search {
            self.run_object_search();
        }

        if let Some(err) = &self.object_search_error {
            ui.add_space(6.0);
            ui.colored_label(PdbgTheme::ERROR_FG, err);
        }

        let mut clicked_hit = None;
        if let Some(result) = &self.object_search_result {
            if !result.hits.is_empty() {
                ui.add_space(6.0);
                ScrollArea::vertical()
                    .id_salt("object_search_results")
                    .max_height(170.0)
                    .show(ui, |ui| {
                        for hit in &result.hits {
                            let label = object_search_hit_summary(hit);
                            if ui
                                .selectable_label(
                                    false,
                                    RichText::new(label).monospace().size(11.0),
                                )
                                .on_hover_text(node_label_for_hit(hit))
                                .clicked()
                            {
                                clicked_hit = Some(hit.clone());
                            }
                        }
                    });
            } else if self.object_search_error.is_none() {
                ui.add_space(6.0);
                ui.label(RichText::new("No matches").small().color(PdbgTheme::MUTED));
            }
        }
        if let Some(hit) = clicked_hit {
            self.follow_object_search_hit(&hit);
        }
    }

    fn draw_text_search(&mut self, ui: &mut egui::Ui) {
        let status = text_search_status_label(
            self.text_search_result.as_ref(),
            self.text_search_error.as_deref(),
            self.text_search_job.is_some(),
            self.text_search_cache.len(),
            self.text_search_cache.current_bytes(),
        );
        section_header(ui, "Text Search", Some(&status));

        let mut run_search = false;
        let mut cancel_search = false;
        let mut clear_search = false;
        section_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                let response = ui.add_enabled(
                    self.text_search_job.is_none(),
                    TextEdit::singleline(&mut self.text_search_query)
                        .desired_width(f32::INFINITY)
                        .hint_text("page text"),
                );
                run_search |=
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if self.text_search_job.is_some() {
                    cancel_search |= ui.button("Cancel").clicked();
                } else {
                    run_search |= ui.button("Search").clicked();
                }
                clear_search |= ui.button("Clear").clicked();
            });
        });

        if clear_search {
            self.cancel_text_search_job();
            self.text_search_query.clear();
            self.text_search_result = None;
            self.text_search_error = None;
            self.selected_text_hit = None;
        } else if cancel_search {
            self.cancel_text_search_job();
        } else if run_search {
            self.start_text_search();
        }

        if let Some(err) = &self.text_search_error {
            ui.add_space(6.0);
            ui.colored_label(PdbgTheme::ERROR_FG, err);
        }

        let mut clicked_hit = None;
        if let Some(result) = &self.text_search_result {
            if !result.page_errors.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "{} pages had extraction errors",
                        result.page_errors.len()
                    ))
                    .small()
                    .color(PdbgTheme::WARN_FG),
                );
            }
            if !result.hits.is_empty() {
                ui.add_space(6.0);
                ScrollArea::vertical()
                    .id_salt("text_search_results")
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for hit in &result.hits {
                            let selected = self
                                .selected_text_hit
                                .as_ref()
                                .is_some_and(|selected| text_hits_same_position(selected, hit));
                            if ui
                                .selectable_label(
                                    selected,
                                    RichText::new(text_search_hit_summary(hit))
                                        .monospace()
                                        .size(11.0),
                                )
                                .on_hover_text(text_search_hit_hover(hit))
                                .clicked()
                            {
                                clicked_hit = Some(hit.clone());
                            }
                        }
                    });
            } else if self.text_search_error.is_none() && self.text_search_job.is_none() {
                ui.add_space(6.0);
                ui.label(RichText::new("No matches").small().color(PdbgTheme::MUTED));
            }
        }
        if let Some(hit) = clicked_hit {
            self.follow_text_search_hit(&hit);
        }
    }

    fn draw_page_preview(&mut self, ui: &mut egui::Ui) {
        if self.empty_workspace {
            self.draw_empty_page_preview(ui);
            return;
        }

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

    fn draw_empty_page_preview(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "Page Preview", Some("No document"));
        let available = ui.available_size();
        let desired = egui::vec2(available.x.max(320.0), available.y.max(360.0));
        section_frame().fill(PdbgTheme::CANVAS).show(ui, |ui| {
            ui.set_min_size(desired);
            ui.vertical_centered(|ui| {
                ui.add_space((desired.y * 0.28).min(180.0));
                ui.label(
                    RichText::new("No PDF open")
                        .strong()
                        .size(16.0)
                        .color(PdbgTheme::TEXT),
                );
                ui.add_space(8.0);
                if ui.button("Open PDF...").clicked() {
                    self.open_pdf_dialog_open = true;
                    self.open_pdf_error = None;
                }
                ui.add_space(12.0);
                ui.label(RichText::new("Drop PDF here").color(PdbgTheme::MUTED));
                if !self.recent_pdf_paths.is_empty() {
                    ui.add_space(16.0);
                    section_header(ui, "Recent Files", None);
                    if let Some(path) =
                        self.draw_recent_file_list(ui, "empty_preview_recent_paths", 140.0)
                    {
                        self.open_pdf_from_path(path);
                    }
                }
            });
        });
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
            if let Some(hit) = self
                .selected_text_hit
                .as_ref()
                .filter(|hit| hit.page_index == render.page_index)
            {
                ui.label(
                    RichText::new(text_search_hit_summary(hit))
                        .monospace()
                        .color(PdbgTheme::ACCENT),
                )
                .on_hover_text(text_search_hit_hover(hit));
            }
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
            let mut page_number = self.render_page_index + 1;
            if ui
                .add(
                    egui::DragValue::new(&mut page_number)
                        .range(1..=page_count)
                        .speed(1),
                )
                .changed()
            {
                self.render_page_index = page_number.saturating_sub(1).min(page_count - 1);
                rerender = true;
            }
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
            if self.real_render_job.is_some() {
                ui.separator();
                if ui.button("Cancel render").clicked() {
                    self.cancel_real_render_job();
                }
            } else {
                ui.separator();
                if ui.button("Render").clicked() {
                    self.real_render_key = None;
                    self.refresh_real_render();
                }
            }
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
            let selected_page = self.render_page_index;
            let loaded_pages = pages.items.len();
            let visible_pages = loaded_pages.min(12);
            let first_visible_page = if selected_page < loaded_pages {
                selected_page
                    .saturating_add(1)
                    .saturating_sub(visible_pages)
            } else {
                0
            };
            for (index, page) in pages
                .items
                .iter()
                .enumerate()
                .skip(first_visible_page)
                .take(visible_pages)
            {
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
            if let Some(total) = pages.total.filter(|total| *total > visible_pages) {
                ui.label(
                    RichText::new(format!("+{} more, use page control", total - visible_pages))
                        .small()
                        .color(PdbgTheme::MUTED),
                );
            }
        });
        if let Some(page_index) = clicked_page {
            self.set_render_page(page_index);
        }
        ui.add_space(6.0);
    }

    fn draw_inspector(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.empty_workspace {
            self.draw_empty_inspector(ui);
            return;
        }

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
            InspectorTab::Diagnostics => self.draw_diagnostics_panel(ui, ctx),
        }
    }

    fn draw_empty_inspector(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "Inspector", Some("No document"));
        section_frame().show(ui, |ui| {
            ui.label(RichText::new("Waiting for a PDF").color(PdbgTheme::MUTED));
            ui.add_space(8.0);
            if ui.button("Open PDF...").clicked() {
                self.open_pdf_dialog_open = true;
                self.open_pdf_error = None;
            }
        });
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
            if self.real_stream_job.is_some() {
                if ui.button("Cancel load").clicked() {
                    self.cancel_real_stream_job();
                }
            } else if ui.button("Load chunk").clicked() || self.real_stream_key.is_none() {
                self.refresh_real_stream_chunk(stream.object);
            }
        });

        if self.real_stream_job.is_some() {
            ui.add_space(8.0);
            ui.label(RichText::new("Loading stream chunk...").color(PdbgTheme::MUTED));
            return;
        }

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

    fn draw_diagnostics_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let model = self.diagnostics_model();
        let all_count = model.all().len();
        let diagnostics = model.filtered(&self.diagnostics_filter());

        section_frame().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Severity").small().color(PdbgTheme::MUTED));
                egui::ComboBox::from_id_salt("diagnostic_min_severity")
                    .selected_text(match &self.diagnostic_min_severity {
                        Some(severity) => severity.as_public_str(),
                        None => "all",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.diagnostic_min_severity, None, "all");
                        ui.selectable_value(
                            &mut self.diagnostic_min_severity,
                            Some(DiagnosticSeverity::Info),
                            "info+",
                        );
                        ui.selectable_value(
                            &mut self.diagnostic_min_severity,
                            Some(DiagnosticSeverity::Warning),
                            "warning+",
                        );
                        ui.selectable_value(
                            &mut self.diagnostic_min_severity,
                            Some(DiagnosticSeverity::Error),
                            "error",
                        );
                    });
                ui.label(RichText::new("Code").small().color(PdbgTheme::MUTED));
                ui.add(
                    TextEdit::singleline(&mut self.diagnostic_code_filter)
                        .desired_width(150.0)
                        .hint_text("code"),
                );
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("{} shown / {} total", diagnostics.len(), all_count))
                        .small()
                        .color(PdbgTheme::MUTED),
                );
                if ui.button("Copy JSON").clicked() {
                    self.copy_diagnostics_json(ctx);
                }
                if ui.button("Copy Markdown").clicked() {
                    self.copy_markdown_report(ctx);
                }
            });
        });

        ui.add_space(8.0);
        if diagnostics.is_empty() {
            ui.label(RichText::new("No diagnostics").color(PdbgTheme::MUTED));
        } else {
            for diagnostic in diagnostics {
                draw_diagnostic_card(ui, &diagnostic);
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
        match state {
            Ok(state) => Self::from_app_state(state, prefer_real),
            Err(_) => Self::Virtual(VirtualObjectTree::new(VIRTUAL_TREE_ROWS)),
        }
    }

    fn from_app_state(state: &AppState, prefer_real: bool) -> Self {
        if prefer_real {
            if let Some(tree) = &state.panels.tree {
                return Self::Real(RealObjectTree::from_child_page(tree));
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

    fn ensure_real_search_hit_row(
        &mut self,
        doc: pdbg_core::DocumentId,
        hit: &ObjectSearchHit,
    ) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.ensure_search_hit_row(doc, hit),
            Self::Virtual(_) => None,
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

    fn row_for_node(&self, node: &NodeId) -> Option<usize> {
        self.rows.iter().position(|row| row.summary.id == *node)
    }

    fn ensure_search_hit_row(
        &mut self,
        doc: pdbg_core::DocumentId,
        hit: &ObjectSearchHit,
    ) -> Option<usize> {
        if let Some(node) = &hit.node {
            if let Some(index) = self.row_for_node(node) {
                return Some(index);
            }
            if let Some(object) = hit.object {
                if let Some(index) = self
                    .rows
                    .iter()
                    .position(|row| row.summary.object == Some(object))
                {
                    return Some(index);
                }
            }
            self.rows.push(RealTreeRow {
                summary: ObjectSummary {
                    id: node.clone(),
                    kind: ObjectKind::Unknown,
                    label: hit.label.clone(),
                    preview: hit.excerpt.clone(),
                    object: hit.object,
                    has_children: true,
                    has_stream: false,
                    child_count: None,
                    byte_size_hint: None,
                    diagnostics: Vec::new(),
                },
                depth: hit.depth.min(8),
                expanded: false,
            });
            return Some(self.rows.len() - 1);
        }

        hit.object.map(|object| self.ensure_object_row(doc, object))
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

    #[cfg(feature = "real-mupdf")]
    fn wait_for_real_stream(app: &mut GuiShellApp) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.real_stream_job.is_some() && Instant::now() < deadline {
            app.poll_real_stream_job();
            if app.real_stream_job.is_some() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        app.poll_real_stream_job();
        assert!(
            app.real_stream_job.is_none(),
            "real stream job did not finish before test timeout"
        );
    }

    fn wait_for_open_pdf(app: &mut GuiShellApp) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.open_pdf_job.is_some() && Instant::now() < deadline {
            app.poll_open_pdf_job();
            if app.open_pdf_job.is_some() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        app.poll_open_pdf_job();
        assert!(
            app.open_pdf_job.is_none(),
            "open PDF job did not finish before test timeout"
        );
    }

    #[cfg(not(feature = "real-mupdf"))]
    fn wait_for_text_search(app: &mut GuiShellApp) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.text_search_job.is_some() && Instant::now() < deadline {
            app.poll_text_search_job();
            if app.text_search_job.is_some() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        app.poll_text_search_job();
        assert!(
            app.text_search_job.is_none(),
            "text search job did not finish before test timeout"
        );
    }

    #[cfg(not(feature = "real-mupdf"))]
    fn wait_for_object_search(app: &mut GuiShellApp) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.object_search_job.is_some() && Instant::now() < deadline {
            app.poll_object_search_job();
            if app.object_search_job.is_some() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        app.poll_object_search_job();
        assert!(
            app.object_search_job.is_none(),
            "object search job did not finish before test timeout"
        );
    }

    #[test]
    fn virtual_tree_does_not_materialize_rows() {
        let tree = VirtualObjectTree::new(1_000_000);
        assert_eq!(tree.row_count(), 1_000_001);
        assert_eq!(tree.row_label(0), "root / catalog");
        assert_eq!(tree.row_label(999_999), "obj 999999 0 R  /FakeNode248");
    }

    #[cfg(not(feature = "real-mupdf"))]
    #[test]
    fn gui_object_search_navigates_headless_fake_hit() {
        let mut app = GuiShellApp::new();
        app.object_search_query = "2 0 R".to_string();

        app.run_object_search();
        assert!(app.object_search_job.is_some());
        wait_for_object_search(&mut app);

        let result = app.object_search_result.as_ref().unwrap();
        assert!(app.object_search_error.is_none());
        assert!(result.searched_nodes > 0);
        let hit = result
            .hits
            .iter()
            .find(|hit| {
                hit.matched_field == ObjectSearchField::ObjectNumber
                    && hit.object == Some(ObjectId { num: 2, gen: 0 })
            })
            .cloned()
            .unwrap();

        app.follow_object_search_hit(&hit);

        assert_eq!(app.selected_row, 2);
        assert_eq!(app.back_stack, vec![0]);
        assert!(app.forward_stack.is_empty());
        assert!(app
            .status_log
            .iter()
            .any(|line| line.starts_with("opened object search hit 2 0 R")));
    }

    #[test]
    fn real_tree_search_hit_row_preserves_search_node() {
        let doc = pdbg_core::DocumentId(7);
        let node = NodeId::DictEntry {
            doc: doc.clone(),
            parent: Box::new(NodeId::DocumentRoot { doc: doc.clone() }),
            key: "Needs".to_string(),
        };
        let hit = ObjectSearchHit {
            label: "Needs".to_string(),
            matched_field: ObjectSearchField::DictionaryKey,
            excerpt: "Needs".to_string(),
            object: None,
            node: Some(node.clone()),
            depth: 2,
        };
        let mut tree = RealObjectTree::from_child_page(&pdbg_core::ChildPage {
            total: Some(0),
            items: Vec::new(),
        });

        let row = tree.ensure_search_hit_row(doc, &hit).unwrap();

        assert_eq!(tree.rows[row].summary.id, node);
        assert_eq!(tree.rows[row].summary.preview, "Needs");
        assert_eq!(tree.rows[row].depth, 2);
    }

    #[test]
    fn text_search_hit_summary_egresses_display_controls() {
        let hit = TextSearchHit {
            page_index: 0,
            span_index: 2,
            excerpt: "A\0B\u{202e}C\nD".to_string(),
            bbox: None,
            untrusted: true,
        };

        let summary = text_search_hit_summary(&hit);

        assert!(summary.contains(&format!("A{}B{}C D", '\u{fffd}', '\u{fffd}')));
        assert!(!summary.contains('\0'));
        assert!(!summary.contains('\u{202e}'));
    }

    #[cfg(not(feature = "real-mupdf"))]
    #[test]
    fn gui_text_search_runs_async_caches_and_selects_hit() {
        let mut app = GuiShellApp::new();
        app.text_search_query = "A".to_string();

        app.start_text_search();
        assert!(app.text_search_job.is_some());
        wait_for_text_search(&mut app);

        let result = app.text_search_result.as_ref().unwrap();
        assert_eq!(result.searched_pages, 1);
        assert_eq!(result.cache_hits, 0);
        assert!(result.hits.iter().any(|hit| hit.untrusted));
        assert_eq!(app.text_search_cache.len(), 1);

        app.start_text_search();
        wait_for_text_search(&mut app);
        assert_eq!(app.text_search_result.as_ref().unwrap().cache_hits, 1);

        let hit = app.text_search_result.as_ref().unwrap().hits[0].clone();
        app.follow_text_search_hit(&hit);

        assert_eq!(app.selected_text_hit.as_ref().unwrap().page_index, 0);
        assert_eq!(app.render_page_index, 0);
        assert!(app
            .status_log
            .iter()
            .any(|line| line.starts_with("opened text search hit page 1")));
    }

    #[test]
    fn diagnostics_model_includes_text_page_errors_and_filters_codes() {
        let mut app = GuiShellApp::new();
        app.text_search_result = Some(TextSearchResult {
            hits: Vec::new(),
            searched_pages: 1,
            cache_hits: 0,
            page_errors: vec![pdbg_core::TextSearchPageError {
                page_index: 2,
                message: "limit".to_string(),
            }],
            truncated: false,
        });
        app.diagnostic_min_severity = Some(DiagnosticSeverity::Warning);
        app.diagnostic_code_filter = "unknown".to_string();

        let diagnostics = app.filtered_diagnostics();

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::Unknown
                && diagnostic.page_index == Some(2)
                && diagnostic.message.contains("text extraction failed")
        }));
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

    #[test]
    fn recent_pdf_paths_are_deduped_bounded_and_persisted() {
        let recent_path = temp_recent_file_path("round-trip");
        let dir = recent_path.parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        let tmp_path = unique_recent_tmp_path(&recent_path);
        assert_eq!(tmp_path.parent(), recent_path.parent());
        assert_ne!(tmp_path, recent_path.with_extension("tmp"));

        let first = dir.join("first.pdf");
        let second = dir.join("second.pdf");
        std::fs::write(&first, b"%PDF-1.7\n").unwrap();
        std::fs::write(&second, b"%PDF-1.7\n").unwrap();

        let mut recent = Vec::new();
        assert!(record_recent_pdf_path(
            &mut recent,
            &first.to_string_lossy()
        ));
        assert!(record_recent_pdf_path(
            &mut recent,
            &second.to_string_lossy()
        ));
        assert!(record_recent_pdf_path(
            &mut recent,
            &first.to_string_lossy()
        ));
        assert_eq!(recent.len(), 2);
        assert_eq!(
            recent[0],
            first.canonicalize().unwrap().to_string_lossy().to_string()
        );
        assert!(!record_recent_pdf_path(&mut recent, "bad\npath.pdf"));

        for index in 0..(RECENT_PDF_MAX_ITEMS + 4) {
            let path = dir.join(format!("extra-{index}.pdf"));
            std::fs::write(&path, b"%PDF-1.7\n").unwrap();
            record_recent_pdf_path(&mut recent, &path.to_string_lossy());
        }
        assert_eq!(recent.len(), RECENT_PDF_MAX_ITEMS);

        save_recent_pdf_paths_to(&recent_path, &recent).unwrap();
        let loaded = load_recent_pdf_paths_from(&recent_path);
        assert_eq!(loaded, recent);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn displayed_file_paths_neutralize_controls() {
        let path = format!("/tmp/report{}fdp.pdf", '\u{202e}');
        let label = display_file_chip_label(&path);
        let hover = display_path_hover(&path);

        assert!(!label.contains('\u{202e}'));
        assert!(!hover.contains('\u{202e}'));
        assert!(label.contains('\u{fffd}'));
        assert!(hover.contains('\u{fffd}'));
        assert_eq!(normalize_recent_pdf_path(&path), Some(path));
    }

    #[test]
    fn gui_empty_workspace_starts_without_fake_document() {
        let app = GuiShellApp::new_with_options(GuiRunOptions {
            start_empty_when_no_pdf: true,
            ..GuiRunOptions::default()
        });

        assert!(app.empty_workspace);
        assert!(app.state.is_err());
        assert_eq!(app.page_count(), 0);
        assert!(app.real_render_job.is_none());
        assert_eq!(app.document_chips().0, "No PDF");
        assert_eq!(app.window_title(), APP_TITLE);
        assert_eq!(app.breadcrumb_label(), "No document");
        assert!(app.status_log.iter().any(|line| line == "No PDF open"));
    }

    #[test]
    fn gui_window_title_reflects_document_and_pending_open() {
        let mut app = GuiShellApp::new();
        assert_eq!(app.window_title(), format!("fake.pdf - {APP_TITLE}"));

        app.open_pdf_from_path("fixtures/synthetic/minimal.pdf".to_string());
        assert!(app.open_pdf_job.is_some());
        assert_eq!(
            app.window_title(),
            format!("Opening minimal.pdf - {APP_TITLE}")
        );
        app.cancel_open_pdf_job();
    }

    #[cfg(not(feature = "real-mupdf"))]
    #[test]
    fn gui_open_pdf_without_real_mupdf_keeps_current_document() {
        let recent_path = temp_recent_file_path("fake-open");
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            recent_files_path: Some(recent_path),
            start_empty_when_no_pdf: false,
            ..GuiRunOptions::default()
        });
        let initial_row = app.tree.row_label(0);

        app.open_pdf_from_path("fixtures/synthetic/minimal.pdf".to_string());
        assert!(app.open_pdf_job.is_some());
        wait_for_open_pdf(&mut app);

        assert_eq!(app.tree.row_label(0), initial_row);
        assert!(app
            .open_pdf_error
            .as_deref()
            .is_some_and(|err| err.contains("requires building pdbg-app")));
        assert!(app.recent_pdf_paths.is_empty());
    }

    #[test]
    fn gui_open_pdf_cancel_discards_pending_job() {
        let recent_path = temp_recent_file_path("cancel-open");
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            recent_files_path: Some(recent_path),
            start_empty_when_no_pdf: false,
            ..GuiRunOptions::default()
        });

        app.open_pdf_from_path("fixtures/synthetic/minimal.pdf".to_string());
        assert!(app.open_pdf_job.is_some());
        app.cancel_open_pdf_job();

        assert!(app.open_pdf_job.is_none());
        assert_eq!(app.open_pdf_error.as_deref(), Some("open cancelled"));
        assert!(app
            .status_log
            .iter()
            .any(|line| line.starts_with("discarded pending open ")));
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_gui_open_pdf_action_replaces_document_and_records_recent() {
        let recent_path = temp_recent_file_path("real-open");
        let path = write_temp_pdf("gui-open-action", &synthetic_two_page_pdf());
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: None,
            recent_files_path: Some(recent_path.clone()),
            start_empty_when_no_pdf: false,
        });
        assert!(!app.tree.is_real());

        app.open_pdf_from_path(path.to_string_lossy().to_string());
        assert!(app.open_pdf_job.is_some());
        wait_for_open_pdf(&mut app);
        wait_for_real_render(&mut app);

        assert!(app.tree.is_real());
        assert_eq!(app.page_count(), 2);
        assert!(app.open_pdf_error.is_none());
        assert!(!app.open_pdf_dialog_open);
        let canonical = path.canonicalize().unwrap().to_string_lossy().to_string();
        assert_eq!(
            app.window_title(),
            format!("{} - {APP_TITLE}", display_file_chip_label(&canonical))
        );
        assert_eq!(app.recent_pdf_paths.first(), Some(&canonical));
        assert_eq!(
            load_recent_pdf_paths_from(&recent_path).first(),
            Some(&canonical)
        );

        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(recent_path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_gui_open_pdf_prompts_for_password_and_retries() {
        let recent_path = temp_recent_file_path("real-password-open");
        let path = encrypted_minimal_pdf_path("gui-password-open");
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: None,
            recent_files_path: Some(recent_path.clone()),
            start_empty_when_no_pdf: false,
        });
        let initial_row = app.tree.row_label(0);

        app.open_pdf_from_path(path.to_string_lossy().to_string());
        assert!(app.open_pdf_job.is_some());
        wait_for_open_pdf(&mut app);

        assert_eq!(app.tree.row_label(0), initial_row);
        assert_eq!(app.open_pdf_error.as_deref(), Some("Password required"));
        assert!(app.open_pdf_dialog_open);
        assert!(app.recent_pdf_paths.is_empty());

        app.open_pdf_password_input = "user".to_string();
        app.open_pdf_from_path(path.to_string_lossy().to_string());
        assert!(app.open_pdf_job.is_some());
        wait_for_open_pdf(&mut app);
        wait_for_real_render(&mut app);

        assert!(app.tree.is_real());
        assert!(app.open_pdf_error.is_none());
        assert!(!app.open_pdf_dialog_open);
        assert!(app.open_pdf_password_input.is_empty());
        let canonical = path.canonicalize().unwrap().to_string_lossy().to_string();
        assert_eq!(app.recent_pdf_paths.first(), Some(&canonical));

        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(recent_path);
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
            recent_files_path: None,
            start_empty_when_no_pdf: false,
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
            recent_files_path: None,
            start_empty_when_no_pdf: false,
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
            recent_files_path: None,
            start_empty_when_no_pdf: false,
        });
        wait_for_real_render(&mut app);

        app.follow_real_reference(ObjectId { num: 4, gen: 0 });
        assert!(app
            .real_detail
            .as_ref()
            .is_some_and(|detail| detail.stream.is_some()));
        app.real_stream_limit = 16;
        app.refresh_real_stream_chunk(ObjectId { num: 4, gen: 0 });
        assert!(app.real_stream_job.is_some());
        wait_for_real_stream(&mut app);

        let chunk = app.real_stream_chunk.as_ref().unwrap();
        assert_eq!(chunk.mode, StreamMode::Raw);
        assert_eq!(chunk.offset, 0);
        assert!(chunk.bytes.starts_with(b"BT /F1"));
        assert!(chunk.truncated);
        assert!(app
            .status_log
            .iter()
            .any(|line| line.contains("queued raw stream chunk 4 0 R")));
        assert!(app
            .status_log
            .iter()
            .any(|line| line.contains("loaded raw stream chunk 4 0 R")));

        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_gui_decoded_stream_cache_reuses_loaded_chunk() {
        let path = write_temp_pdf("gui-stream-cache", &synthetic_large_xref_pdf(16));
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: Some(path.to_string_lossy().to_string()),
            recent_files_path: None,
            start_empty_when_no_pdf: false,
        });
        wait_for_real_render(&mut app);

        let object = ObjectId { num: 4, gen: 0 };
        app.follow_real_reference(object);
        app.real_stream_mode = StreamMode::Decoded;
        app.real_stream_limit = 16;
        app.refresh_real_stream_chunk(object);
        assert!(app.real_stream_job.is_some());
        wait_for_real_stream(&mut app);
        assert_eq!(
            app.real_stream_chunk.as_ref().unwrap().mode,
            StreamMode::Decoded
        );
        assert_eq!(app.decoded_stream_cache.len(), 1);

        app.clear_real_stream_chunk();
        app.refresh_real_stream_chunk(object);

        assert!(app.real_stream_job.is_none());
        assert_eq!(
            app.real_stream_chunk.as_ref().unwrap().mode,
            StreamMode::Decoded
        );
        assert!(app
            .status_log
            .iter()
            .any(|line| { line.contains("reused cached decoded stream chunk 4 0 R @ 0") }));

        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_gui_stream_job_can_be_cancelled_from_ui_state() {
        let path = write_temp_pdf("gui-stream-cancel", &synthetic_large_xref_pdf(16));
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: Some(path.to_string_lossy().to_string()),
            recent_files_path: None,
            start_empty_when_no_pdf: false,
        });
        wait_for_real_render(&mut app);

        app.follow_real_reference(ObjectId { num: 4, gen: 0 });
        app.real_stream_limit = 16;
        app.refresh_real_stream_chunk(ObjectId { num: 4, gen: 0 });
        assert!(app.real_stream_job.is_some());

        app.cancel_real_stream_job();
        assert!(app.real_stream_job.is_none());
        assert!(app.real_stream_chunk.is_none());
        assert_eq!(
            app.real_stream_error.as_deref(),
            Some("stream chunk load cancelled")
        );
        assert!(app
            .status_log
            .iter()
            .any(|line| line.contains("cancelled raw stream chunk 4 0 R")));

        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_gui_page_controls_refresh_render_parameters() {
        let path = write_temp_pdf("gui-pages", &synthetic_two_page_pdf());
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: Some(path.to_string_lossy().to_string()),
            recent_files_path: None,
            start_empty_when_no_pdf: false,
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
    fn real_gui_render_cache_reuses_previous_page() {
        let path = write_temp_pdf("gui-render-cache", &synthetic_two_page_pdf());
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: Some(path.to_string_lossy().to_string()),
            recent_files_path: None,
            start_empty_when_no_pdf: false,
        });
        wait_for_real_render(&mut app);

        app.set_render_page(1);
        wait_for_real_render(&mut app);
        assert_eq!(app.render_cache.len(), 2);

        app.set_render_page(0);

        assert!(app.real_render_job.is_none());
        let render = app.real_render.as_ref().unwrap();
        assert_eq!(render.page_index, 0);
        assert_eq!((render.width, render.height), (400, 200));
        assert!(app
            .status_log
            .iter()
            .any(|line| line.starts_with("reused cached page 1 @ 200%")));

        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_gui_render_job_replacement_keeps_latest_page() {
        let path = write_temp_pdf("gui-render-replace", &synthetic_two_page_pdf());
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: Some(path.to_string_lossy().to_string()),
            recent_files_path: None,
            start_empty_when_no_pdf: false,
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

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_gui_render_job_can_be_cancelled_from_ui_state() {
        let path = write_temp_pdf("gui-render-cancel", &synthetic_two_page_pdf());
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: Some(path.to_string_lossy().to_string()),
            recent_files_path: None,
            start_empty_when_no_pdf: false,
        });
        assert!(app.real_render_job.is_some());

        app.cancel_real_render_job();
        assert!(app.real_render_job.is_none());
        assert!(app.real_render.is_none());
        assert_eq!(
            app.real_render_error.as_deref(),
            Some("page render cancelled")
        );
        assert!(app
            .status_log
            .iter()
            .any(|line| line.starts_with("cancelled page 1")));

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
            recent_files_path: None,
            start_empty_when_no_pdf: false,
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

    fn temp_recent_file_path(prefix: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "pdbg-app-{}-{}-{}",
                prefix,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
            .join("recent-files.txt")
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
    fn mutool_path() -> PathBuf {
        if let Some(path) = std::env::var_os("PDBG_MUTOOL_PATH") {
            return PathBuf::from(path);
        }
        let source_dir = std::env::var_os("PDBG_MUPDF_SOURCE_DIR")
            .expect("real encrypted GUI test requires PDBG_MUPDF_SOURCE_DIR or PDBG_MUTOOL_PATH");
        let path = PathBuf::from(source_dir)
            .join("build")
            .join("release")
            .join("mutool");
        assert!(
            path.is_file(),
            "real encrypted GUI test requires mutool at {}; build it with `make build=release build/release/mutool` or set PDBG_MUTOOL_PATH",
            path.display()
        );
        path
    }

    #[cfg(feature = "real-mupdf")]
    fn encrypted_minimal_pdf_path(prefix: &str) -> PathBuf {
        let input = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/minimal.pdf"
        );
        let output = std::env::temp_dir().join(format!(
            "pdbg-app-{}-{}-{}.pdf",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let status = std::process::Command::new(mutool_path())
            .args([
                "clean", "-E", "aes-128", "-O", "owner", "-U", "user", "-P", "0", input,
            ])
            .arg(&output)
            .status()
            .expect("failed to run mutool");
        assert!(
            status.success(),
            "mutool failed to create encrypted GUI fixture"
        );
        output
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
