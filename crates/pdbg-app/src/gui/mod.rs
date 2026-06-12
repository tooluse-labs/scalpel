use crate::AppState;
use eframe::egui::{
    self, Color32, FontDefinitions, FontFamily, FontId, RichText, ScrollArea, TextEdit, TextStyle,
};
use pdbg_core::{
    build_markdown_report, diagnostics_payload_to_json_string, escape_pdf_text,
    search_objects_with_cancel, search_text_with_cache, CancelToken, ChildContainer, ChildPage,
    ChildRange, DiagnosticCode, DiagnosticFilter, DiagnosticSeverity, DiagnosticSummary,
    DocumentDiagnostics, DocumentSession, EgressFormat, EscapedText, ImagePreview,
    MarkdownReportInput, NodeId, NodePathSegment, ObjectDetail, ObjectId, ObjectKind,
    ObjectSearchField, ObjectSearchHit, ObjectSearchRequest, ObjectSearchResult, ObjectSummary,
    ObjectValue, OpenDocument, PageRect, RenderRequest, RenderResult, RenderResultCache,
    ShimDocument, StreamChunk, StreamChunkCache, StreamMode, StreamSaveOutcome, StreamSummary,
    StreamViewMode, TextPage, TextPageCache, TextRequest, TextSearchHit, TextSearchRequest,
    TextSearchResult, TextSpan, VisualElement, VisualElementKind, VisualPage, VisualRequest,
    XrefEntryInfo, XrefEntryKind, XrefTableSlice,
};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

mod app;
mod draw;
mod files;
mod jobs;
mod labels;
mod layout;
mod preview;
mod stream;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
mod theme;
mod tree;

// app.rs mostly holds GuiShellApp impl blocks; selected free helpers are shared
// with drawing code and tests.
#[cfg(test)]
use app::*;
use app::{xobject_subtype, ResolvedXObject};
use files::*;
use jobs::*;
use labels::*;
use layout::*;
use preview::*;
use stream::*;
use theme::*;
use tree::*;

const VIRTUAL_TREE_ROWS: usize = 1_000_000;
const STREAM_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const HEX_WINDOW_BYTES: usize = 512;
const REAL_STREAM_DEFAULT_VIEW_LIMIT_BYTES: usize = 64 * 1024;
const REAL_STREAM_MAX_VIEW_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const REAL_STREAM_MAX_LOADED_WINDOWS: usize = 5;
const COPY_LIMIT_BYTES: usize = 4096;
const DEFAULT_RENDER_ZOOM: f32 = 1.0;
const RENDER_ZOOM_LEVELS: [f32; 6] = [0.5, 1.0, 1.5, 2.0, 3.0, 4.0];
const DEFAULT_RENDER_MAX_DIMENSION: u32 = 4096;
const DEFAULT_RENDER_MAX_OUTPUT_BYTES: u64 = 128 * 1024 * 1024;
const RENDER_DIMENSION_LIMIT_ERROR: &str = "render output exceeds configured dimensions";
const PREVIEW_CONTROL_GROUP_HEIGHT: f32 = 42.0;
const PREVIEW_ZOOM_CONTROL_WIDTH: f32 = 228.0;
const PREVIEW_PAGER_CONTROL_WIDTH: f32 = 174.0;
const PREVIEW_CONTROL_GAP: f32 = 14.0;
const PREVIEW_PAGE_SCROLL_THRESHOLD: f32 = 120.0;
const PREVIEW_PAGE_SCROLL_MAX_STEPS: i32 = 5;
const TOP_BAR_HEIGHT: f32 = 38.0;
const TOP_BAR_BUTTON_HEIGHT: f32 = 28.0;
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
const VISUAL_CLICK_MAX_ELEMENTS_PER_PAGE: usize = 200_000;
const VISUAL_CLICK_CACHE_MAX_PAGES: usize = 8;
const VISUAL_CLICK_CACHE_MAX_ELEMENTS: usize = 400_000;
const MARKDOWN_REPORT_LIMIT_BYTES: usize = 64 * 1024;
const REPORT_DIAGNOSTIC_LIMIT: usize = 128;
const REPORT_SEARCH_HIT_LIMIT: usize = 64;
const RECENT_PDF_MAX_ITEMS: usize = 10;
const XREF_PAGE_SIZE: usize = 256;
const HEX_VIEW_WINDOW_BYTES: usize = 4096;
const HEX_VIEW_BYTES_PER_ROW: usize = 16;
const IMAGE_PREVIEW_MAX_DIMENSION: u32 = 1024;
const IMAGE_PREVIEW_MAX_HEIGHT: f32 = 280.0;
const STREAM_EXPORT_MAX_BYTES: u64 = 512 * 1024 * 1024;
const PATH_DISPLAY_MAX_BYTES: usize = 4096;
const TEXT_CLICK_BBOX_TOLERANCE_PT: f32 = 3.0;
const VISUAL_CLICK_BBOX_TOLERANCE_PT: f32 = 5.0;
const APP_TITLE: &str = "Scalpel";
const APP_GITHUB_URL: &str = "https://github.com/tooluse-labs/xreflab";
const LEFT_PANEL_MIN_WIDTH: f32 = 220.0;
const LEFT_PANEL_DEFAULT_WIDTH: f32 = 320.0;
const LEFT_PANEL_MAX_WIDTH: f32 = 520.0;
const COMPACT_LEFT_PANEL_MIN_WIDTH: f32 = 200.0;
const COMPACT_LEFT_PANEL_DEFAULT_WIDTH: f32 = 280.0;
const RIGHT_PANEL_MIN_WIDTH: f32 = 280.0;
const RIGHT_PANEL_DEFAULT_WIDTH: f32 = 440.0;
const RIGHT_PANEL_MAX_WIDTH: f32 = 680.0;
const COMPACT_RIGHT_PANEL_MIN_WIDTH: f32 = 260.0;
const COMPACT_RIGHT_PANEL_DEFAULT_WIDTH: f32 = 340.0;
const COMPACT_LAYOUT_WIDTH: f32 = 1180.0;
const PAGE_PREVIEW_MIN_WIDTH: f32 = 360.0;
const PAGE_PREVIEW_MIN_HEIGHT: f32 = 320.0;
const WORKSPACE_SPLITTER_WIDTH: f32 = 6.0;
const WORKSPACE_MIN_CENTER_WIDTH: f32 = 360.0;
const DENSE_ROW_FONT_SIZE: f32 = 11.0;
const STREAM_VIEW_FONT_SIZE: f32 = 10.0;
const STREAM_VIEW_MIN_HEIGHT: f32 = 240.0;
const STREAM_VIEW_AUTO_LOAD_EDGE_PX: f32 = 48.0;
const NICE_STREAM_INDENT_WIDTH: f32 = 12.0;
const CJK_FONT_NAME: &str = "pdbg-cjk";

const CJK_FONT_CANDIDATES: &[&str] = &[
    "C:\\Windows\\Fonts\\Deng.ttf",
    "C:\\Windows\\Fonts\\simhei.ttf",
    "C:\\Windows\\Fonts\\simsunb.ttf",
    "C:\\Windows\\Fonts\\simkai.ttf",
    "C:\\Windows\\Fonts\\msyh.ttc",
    "C:\\Windows\\Fonts\\msyhbd.ttc",
    "/System/Library/Fonts/PingFang.ttc",
    "/System/Library/Fonts/STHeiti Light.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
];

#[derive(Clone, Debug, Default)]
pub struct GuiRunOptions {
    pub smoke_exit_after: Option<Duration>,
    pub pdf_path: Option<String>,
    pub recent_files_path: Option<PathBuf>,
    pub start_empty_when_no_pdf: bool,
    pub render_max_dimension: Option<u32>,
}

fn app_icon() -> Option<Arc<egui::IconData>> {
    let icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icons/scalpel-mark.png"))
            .ok()?;
    Some(Arc::new(icon))
}

pub fn run_gui_with_options(options: GuiRunOptions) -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_TITLE)
        .with_inner_size([1440.0, 900.0]);
    if let Some(icon) = app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        APP_TITLE,
        native_options,
        Box::new(move |cc| {
            // The app constructor loads persisted UI settings (theme choice),
            // so it must run before the style is applied.
            let app = GuiShellApp::new_with_options(options);
            configure_egui(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}

pub struct GuiShellApp {
    state: Result<AppState, String>,
    empty_workspace: bool,
    launched_at: Instant,
    smoke_exit_after: Option<Duration>,
    recent_files_path: PathBuf,
    recent_pdf_paths: Vec<String>,
    ui_settings_path: PathBuf,
    open_pdf_dialog_open: bool,
    open_pdf_path_input: String,
    open_pdf_password_input: String,
    open_pdf_error: Option<String>,
    open_pdf_job: Option<OpenPdfJob>,
    about_dialog_open: bool,
    about_logo_texture: Option<egui::TextureHandle>,
    left_panel_width: Option<f32>,
    right_panel_width: Option<f32>,
    tree: TreeModel,
    stream: LargeStreamModel,
    real_stream_preset: RealStreamPreset,
    real_stream_mode: StreamMode,
    real_stream_view_mode: StreamViewMode,
    real_stream_offset: u64,
    real_stream_limit: usize,
    real_stream_key: Option<RealStreamKey>,
    real_stream_job: Option<RealStreamJob>,
    real_stream_chunk: Option<StreamChunk>,
    real_stream_windows: VecDeque<RealStreamLoadedWindow>,
    real_stream_collapsed_blocks: HashSet<String>,
    real_stream_selected_block: Option<String>,
    scroll_selected_nice_stream_row: bool,
    real_stream_error: Option<String>,
    decoded_stream_cache: StreamChunkCache<RealStreamKey>,
    selected_row: usize,
    scroll_selected_tree_row: bool,
    back_stack: Vec<usize>,
    forward_stack: Vec<usize>,
    selected_tab: InspectorTab,
    real_detail: Option<ObjectDetail>,
    real_detail_error: Option<String>,
    real_pages: Option<ChildPage<ObjectSummary>>,
    real_pages_error: Option<String>,
    render_page_index: usize,
    preview_page_scroll_accumulator: egui::Vec2,
    render_zoom: f32,
    render_rotation_degrees: i32,
    render_max_dimension: u32,
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
    visual_page_cache: VisualPageCache,
    selected_text_hit: Option<TextSearchHit>,
    selected_visual_hit: Option<PreviewVisualHit>,
    pending_preview_stream_selection: Option<PendingPreviewStreamSelection>,
    preview_click: Option<PagePreviewClick>,
    xref_slice: Option<XrefTableSlice>,
    xref_error: Option<String>,
    xref_offset: usize,
    hex_mode: StreamMode,
    hex_offset: u64,
    hex_jump_input: String,
    hex_key: Option<RealStreamKey>,
    hex_job: Option<RealStreamJob>,
    hex_chunk: Option<StreamChunk>,
    hex_error: Option<String>,
    image_preview_job: Option<ImagePreviewJob>,
    image_preview_result: Option<(ObjectId, ImagePreview)>,
    image_preview_error: Option<(ObjectId, String)>,
    image_preview_texture: Option<(ObjectId, egui::TextureHandle)>,
    stream_export_job: Option<StreamExportJob>,
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

impl eframe::App for GuiShellApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_ui_settings();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_dropped_pdf_files(&ctx);
        self.poll_open_pdf_job();
        self.poll_real_stream_job();
        self.poll_hex_job();
        self.poll_image_preview_job();
        self.poll_stream_export_job();
        self.poll_real_render_job();
        self.poll_object_search_job();
        self.poll_text_search_job();
        self.handle_page_keyboard_shortcuts(&ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
        if self.open_pdf_job.is_some()
            || self.real_stream_job.is_some()
            || self.hex_job.is_some()
            || self.image_preview_job.is_some()
            || self.stream_export_job.is_some()
            || self.real_render_job.is_some()
            || self.object_search_job.is_some()
            || self.text_search_job.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        egui::Panel::top("top_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme().top_bar)
                    .inner_margin(egui::Margin::symmetric(12, 0)),
            )
            .show_inside(ui, |ui| {
                ui.set_min_height(TOP_BAR_HEIGHT);
                self.draw_top_bar(ui);
            });

        egui::Panel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme().surface)
                    .inner_margin(egui::Margin::symmetric(12, 5)),
            )
            .show_inside(ui, |ui| self.draw_status_bar(ui));

        self.draw_workspace(ui, &ctx);

        self.draw_open_pdf_dialog(&ctx);
        self.draw_about_dialog(&ctx);

        if self
            .smoke_exit_after
            .is_some_and(|duration| self.launched_at.elapsed() >= duration)
        {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
