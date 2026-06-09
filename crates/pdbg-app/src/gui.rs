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
    ObjectSearchResult, ObjectSummary, ObjectValue, PageRect, RenderRequest, RenderResult,
    RenderResultCache, ShimDocument, StreamChunk, StreamChunkCache, StreamMode, StreamSummary,
    StreamViewMode, TextPage, TextPageCache, TextRequest, TextSearchHit, TextSearchRequest,
    TextSearchResult, TextSpan, VisualElement, VisualElementKind, VisualPage, VisualRequest,
};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

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
const PREVIEW_CONTROL_ROW_HEIGHT: f32 = 44.0;
const PREVIEW_CONTROL_GROUP_HEIGHT: f32 = 42.0;
const PREVIEW_ZOOM_CONTROL_WIDTH: f32 = 228.0;
const PREVIEW_PAGER_CONTROL_WIDTH: f32 = 174.0;
const PREVIEW_CONTROL_GAP: f32 = 14.0;
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
const PATH_DISPLAY_MAX_BYTES: usize = 4096;
const TEXT_CLICK_BBOX_TOLERANCE_PT: f32 = 3.0;
const VISUAL_CLICK_BBOX_TOLERANCE_PT: f32 = 5.0;
const APP_TITLE: &str = "pdbg Preview";
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
const PAGE_PREVIEW_FOOTER_RESERVED_HEIGHT: f32 = 64.0;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RealRenderKey {
    page_index: usize,
    zoom_bits: u32,
    rotation_degrees: i32,
    max_dimension: u32,
}

impl RealRenderKey {
    fn new(page_index: usize, zoom: f32, rotation_degrees: i32, max_dimension: u32) -> Self {
        Self {
            page_index,
            zoom_bits: zoom.to_bits(),
            rotation_degrees,
            max_dimension,
        }
    }

    fn zoom(self) -> f32 {
        f32::from_bits(self.zoom_bits)
    }

    fn request(self) -> RenderRequest {
        let mut request = RenderRequest::page(self.page_index);
        request.zoom = self.zoom();
        request.rotation_degrees = self.rotation_degrees;
        request.max_width = self.max_dimension;
        request.max_height = self.max_dimension;
        request.max_pixels = render_max_pixels(self.max_dimension);
        request.max_output_bytes = render_max_output_bytes(self.max_dimension);
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
    let has_runtime_cjk_font = add_runtime_cjk_font(&mut fonts);
    let mut sans_fonts = vec![
        "InterVariable".to_string(),
        "Ubuntu-Light".to_string(),
        "NotoEmoji-Regular".to_string(),
        "emoji-icon-font".to_string(),
    ];
    if has_runtime_cjk_font {
        sans_fonts.insert(1, CJK_FONT_NAME.to_string());
    }
    fonts
        .families
        .insert(FontFamily::Name("pdbg-sans".into()), sans_fonts);
    let mut mono_fonts = vec![
        "JetBrainsMono-Regular".to_string(),
        "Hack".to_string(),
        "Ubuntu-Light".to_string(),
        "NotoEmoji-Regular".to_string(),
    ];
    if has_runtime_cjk_font {
        mono_fonts.insert(1, CJK_FONT_NAME.to_string());
        insert_font_fallback(fonts.families.entry(FontFamily::Proportional).or_default());
        insert_font_fallback(fonts.families.entry(FontFamily::Monospace).or_default());
    }
    fonts
        .families
        .insert(FontFamily::Name("pdbg-mono".into()), mono_fonts);
    fonts
}

fn insert_font_fallback(family: &mut Vec<String>) {
    if !family.iter().any(|font| font == CJK_FONT_NAME) {
        family.push(CJK_FONT_NAME.to_string());
    }
}

fn add_runtime_cjk_font(fonts: &mut FontDefinitions) -> bool {
    if let Some(bytes) = CJK_FONT_CANDIDATES
        .iter()
        .map(Path::new)
        .find_map(|path| fs::read(path).ok())
    {
        fonts.font_data.insert(
            CJK_FONT_NAME.to_string(),
            egui::FontData::from_owned(bytes).into(),
        );
        return true;
    }
    false
}

fn page_preview_display_size(
    texture_size: egui::Vec2,
    available_size: egui::Vec2,
    reserved_footer_height: f32,
    visual_zoom: f32,
) -> egui::Vec2 {
    if texture_size.x <= 0.0 || texture_size.y <= 0.0 {
        return egui::Vec2::ZERO;
    }
    let visual_zoom = visual_zoom.max(0.1);
    let base_texture_size = texture_size / visual_zoom;
    let image_available_height = (available_size.y - reserved_footer_height).max(1.0);
    let scale = (available_size.x / base_texture_size.x)
        .min(image_available_height / base_texture_size.y)
        .max(0.1);
    base_texture_size * scale * visual_zoom
}

fn page_preview_leading_space(available_width: f32, display_width: f32) -> f32 {
    ((available_width - display_width) * 0.5).max(0.0)
}

fn previous_render_zoom(current: f32) -> Option<f32> {
    RENDER_ZOOM_LEVELS
        .iter()
        .rev()
        .copied()
        .find(|zoom| *zoom < current - f32::EPSILON)
}

fn next_render_zoom(current: f32) -> Option<f32> {
    RENDER_ZOOM_LEVELS
        .iter()
        .copied()
        .find(|zoom| *zoom > current + f32::EPSILON)
}

fn next_render_rotation(current: i32) -> i32 {
    match current.rem_euclid(360) {
        0 => 90,
        90 => 180,
        180 => 270,
        _ => 0,
    }
}

fn render_max_dimension_or_default(value: Option<u32>) -> u32 {
    value
        .filter(|dimension| *dimension > 0)
        .unwrap_or(DEFAULT_RENDER_MAX_DIMENSION)
}

fn render_max_pixels(max_dimension: u32) -> u64 {
    u64::from(max_dimension).saturating_mul(u64::from(max_dimension))
}

fn render_max_output_bytes(max_dimension: u32) -> u64 {
    render_max_pixels(max_dimension)
        .saturating_mul(4)
        .max(DEFAULT_RENDER_MAX_OUTPUT_BYTES)
}

#[derive(Clone, Copy, Debug)]
enum PreviewControlIcon {
    ZoomIn,
    ZoomOut,
    PreviousPage,
    NextPage,
    RotateRight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PageKeyboardShortcut {
    Previous,
    Next,
    First,
    Last,
}

fn page_keyboard_target_page(
    current_page: usize,
    page_count: usize,
    shortcut: PageKeyboardShortcut,
) -> Option<usize> {
    if page_count == 0 {
        return None;
    }
    let last_page = page_count - 1;
    Some(match shortcut {
        PageKeyboardShortcut::Previous => current_page.saturating_sub(1),
        PageKeyboardShortcut::Next => (current_page + 1).min(last_page),
        PageKeyboardShortcut::First => 0,
        PageKeyboardShortcut::Last => last_page,
    })
}

fn preview_icon_button(
    ui: &mut egui::Ui,
    icon: PreviewControlIcon,
    enabled: bool,
    hover_text: impl Into<String>,
) -> egui::Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("")
            .frame(false)
            .min_size(egui::vec2(36.0, 30.0)),
    );
    if ui.is_rect_visible(response.rect) {
        draw_preview_control_icon(ui, response.rect.shrink(4.0), icon, enabled);
    }
    response.on_hover_text(hover_text.into())
}

fn preview_control_group<R>(
    ui: &mut egui::Ui,
    width: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let size = egui::vec2(width, PREVIEW_CONTROL_GROUP_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, 16.0, PdbgTheme::CHIP_BG);
    }

    let inner_rect = rect.shrink2(egui::vec2(10.0, 6.0));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(inner_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.set_min_size(inner_rect.size());
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
            add_contents(ui)
        },
    )
    .inner
}

fn preview_control_separator(ui: &mut egui::Ui) {
    let height = 26.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, height), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().line_segment(
            [rect.center_top(), rect.center_bottom()],
            egui::Stroke::new(1.0, PdbgTheme::BORDER),
        );
    }
}

fn draw_preview_control_icon(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    icon: PreviewControlIcon,
    enabled: bool,
) {
    let color = if enabled {
        PdbgTheme::TEXT
    } else {
        PdbgTheme::MUTED
    };
    let stroke = egui::Stroke::new(1.5, color);
    match icon {
        PreviewControlIcon::ZoomIn => draw_plus_minus_icon(ui, rect, true, stroke),
        PreviewControlIcon::ZoomOut => draw_plus_minus_icon(ui, rect, false, stroke),
        PreviewControlIcon::PreviousPage => draw_chevron_icon(ui, rect, false, stroke),
        PreviewControlIcon::NextPage => draw_chevron_icon(ui, rect, true, stroke),
        PreviewControlIcon::RotateRight => draw_rotate_icon(ui, rect, stroke),
    }
}

fn draw_plus_minus_icon(ui: &mut egui::Ui, rect: egui::Rect, plus: bool, stroke: egui::Stroke) {
    let painter = ui.painter();
    let center = rect.center();
    let half = rect.width().min(rect.height()) * 0.27;
    painter.line_segment(
        [
            center + egui::vec2(-half, 0.0),
            center + egui::vec2(half, 0.0),
        ],
        stroke,
    );
    if plus {
        painter.line_segment(
            [
                center + egui::vec2(0.0, -half),
                center + egui::vec2(0.0, half),
            ],
            stroke,
        );
    }
}

fn draw_chevron_icon(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    points_right: bool,
    stroke: egui::Stroke,
) {
    let painter = ui.painter();
    let center = rect.center();
    let half_x = rect.width() * 0.14;
    let half_y = rect.height() * 0.26;
    let outer_x = if points_right { -half_x } else { half_x };
    let inner_x = if points_right { half_x } else { -half_x };
    let top = center + egui::vec2(outer_x, -half_y);
    let middle = center + egui::vec2(inner_x, 0.0);
    let bottom = center + egui::vec2(outer_x, half_y);
    painter.line_segment([top, middle], stroke);
    painter.line_segment([middle, bottom], stroke);
}

fn draw_rotate_icon(ui: &mut egui::Ui, rect: egui::Rect, stroke: egui::Stroke) {
    let painter = ui.painter();
    let side = rect.width().min(rect.height());
    let origin = rect.center() - egui::vec2(side * 0.5, side * 0.5);
    let scale = side / 16.0;
    let point = |x: f32, y: f32| origin + egui::vec2(x * scale, y * scale);

    let box_rect = egui::Rect::from_min_max(point(2.5, 6.5), point(10.5, 14.5));
    painter.rect_stroke(box_rect, 2.0 * scale, stroke, egui::StrokeKind::Outside);

    let mut path = Vec::with_capacity(14);
    path.push(point(8.0, 2.5));
    path.push(point(10.5, 2.5));
    for step in 1..=10 {
        let t = step as f32 / 10.0;
        let one_minus_t = 1.0 - t;
        let x = one_minus_t.powi(3) * 10.5
            + 3.0 * one_minus_t.powi(2) * t * 12.709_139
            + 3.0 * one_minus_t * t.powi(2) * 14.5
            + t.powi(3) * 14.5;
        let y = one_minus_t.powi(3) * 2.5
            + 3.0 * one_minus_t.powi(2) * t * 2.5
            + 3.0 * one_minus_t * t.powi(2) * 4.290_861
            + t.powi(3) * 6.5;
        path.push(point(x, y));
    }
    path.push(point(14.5, 7.0));
    painter.line(path, stroke);

    painter.line(
        vec![point(9.5, 0.5), point(7.5, 2.5), point(9.5, 4.5)],
        stroke,
    );
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
            let detail_width = ui.available_width().max(0.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                truncated_label(
                    ui,
                    RichText::new(detail).small().color(PdbgTheme::MUTED),
                    detail_width,
                    Some(detail),
                );
            });
        }
    });
    ui.add_space(4.0);
}

fn truncated_label(
    ui: &mut egui::Ui,
    text: RichText,
    max_width: f32,
    hover_text: Option<&str>,
) -> egui::Response {
    let width = max_width.max(24.0);
    let response = ui.add_sized(
        egui::vec2(width, ui.text_style_height(&TextStyle::Body)),
        egui::Label::new(text).truncate(),
    );
    if let Some(hover_text) = hover_text.filter(|text| !text.is_empty()) {
        response.on_hover_text(hover_text)
    } else {
        response
    }
}

fn truncated_monospace(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    let text = text.into();
    let width = ui.available_width().max(24.0);
    let response = ui.add_sized(
        egui::vec2(width, ui.text_style_height(&TextStyle::Monospace)),
        egui::Label::new(dense_monospace_text(text.as_str())).truncate(),
    );
    response.on_hover_text(text)
}

fn dense_label(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.label(RichText::new(text.into()).size(DENSE_ROW_FONT_SIZE))
}

fn dense_monospace_text(text: impl Into<String>) -> RichText {
    RichText::new(text.into())
        .monospace()
        .size(DENSE_ROW_FONT_SIZE)
}

fn mono_font_id(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("pdbg-mono".into()))
}

#[derive(Default)]
struct SearchControlsOutput {
    submit: bool,
    cancel: bool,
    clear: bool,
}

fn draw_search_controls(
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
struct PanelWidthSpec {
    min: f32,
    default: f32,
    max: f32,
}

#[derive(Clone, Copy, Debug)]
struct WorkspacePanelLayout {
    left: PanelWidthSpec,
    right: PanelWidthSpec,
}

#[derive(Clone, Copy, Debug)]
struct WorkspaceRects {
    left: egui::Rect,
    left_splitter: egui::Rect,
    center: egui::Rect,
    right_splitter: egui::Rect,
    right: egui::Rect,
}

fn workspace_panel_layout(available_width: f32) -> WorkspacePanelLayout {
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

fn workspace_min_center_width(total_width: f32) -> f32 {
    WORKSPACE_MIN_CENTER_WIDTH.min((total_width * 0.35).max(220.0))
}

fn clamp_workspace_widths(
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

fn workspace_rects(
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

fn show_framed_child<R>(
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

fn draw_workspace_splitter(ui: &mut egui::Ui, rect: egui::Rect, response: &egui::Response) {
    let fill = if response.dragged() {
        PdbgTheme::ACCENT
    } else if response.hovered() {
        PdbgTheme::STRONG_BORDER
    } else {
        PdbgTheme::BORDER
    };
    ui.painter().rect_filled(rect, 0.0, fill);
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
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
            ui.label(
                RichText::new(label)
                    .font(FontId::new(11.0, FontFamily::Name("pdbg-sans".into())))
                    .strong()
                    .color(fg),
            );
        });
}

fn option_text(value: Option<&str>) -> &str {
    value.unwrap_or("-")
}

#[derive(Clone, Copy, Debug)]
struct PagePreviewClick {
    page_index: usize,
    render_x: f32,
    render_y: f32,
    normalized_x: f32,
    normalized_y: f32,
}

#[derive(Clone, Debug)]
struct PreviewVisualHit {
    page_index: usize,
    element_index: usize,
    kind: VisualElementKind,
    bbox: PageRect,
    object: Option<ObjectId>,
    untrusted: bool,
    contains_click: bool,
}

#[derive(Clone, Copy, Debug)]
struct RealVisualTarget {
    page_index: usize,
    object: ObjectId,
    allow_page_union: bool,
}

#[derive(Clone, Debug)]
struct PendingPreviewStreamSelection {
    page_index: usize,
    text_hit: Option<TextSearchHit>,
    visual_hit: Option<PreviewVisualHit>,
}

#[derive(Clone, Debug)]
struct VisualPageCache {
    entries: VecDeque<VisualPageCacheEntry>,
    max_pages: usize,
    max_elements: usize,
    current_elements: usize,
}

#[derive(Clone, Debug)]
struct VisualPageCacheEntry {
    page: VisualPage,
    element_count: usize,
}

impl VisualPageCache {
    fn new(max_pages: usize, max_elements: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_pages: max_pages.max(1),
            max_elements: max_elements.max(1),
            current_elements: 0,
        }
    }

    fn get(&mut self, page_index: usize) -> Option<VisualPage> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.page.page_index == page_index)?;
        let entry = self.entries.remove(index)?;
        let page = entry.page.clone();
        self.entries.push_back(entry);
        Some(page)
    }

    fn insert(&mut self, page: VisualPage) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.page.page_index == page.page_index)
        {
            if let Some(entry) = self.entries.remove(index) {
                self.current_elements = self.current_elements.saturating_sub(entry.element_count);
            }
        }

        let element_count = page.elements.len();
        if element_count > self.max_elements {
            return;
        }
        self.current_elements += element_count;
        self.entries.push_back(VisualPageCacheEntry {
            page,
            element_count,
        });
        self.evict_to_budget();
    }

    fn evict_to_budget(&mut self) {
        while self.entries.len() > self.max_pages || self.current_elements > self.max_elements {
            let Some(entry) = self.entries.pop_front() else {
                break;
            };
            self.current_elements = self.current_elements.saturating_sub(entry.element_count);
        }
    }
}

fn preview_click_from_pos(
    pos: egui::Pos2,
    image_rect: egui::Rect,
    render_width: u32,
    render_height: u32,
    page_index: usize,
) -> Option<PagePreviewClick> {
    if !image_rect.contains(pos) || render_width == 0 || render_height == 0 {
        return None;
    }
    let normalized_x = ((pos.x - image_rect.left()) / image_rect.width()).clamp(0.0, 1.0);
    let normalized_y = ((pos.y - image_rect.top()) / image_rect.height()).clamp(0.0, 1.0);
    Some(PagePreviewClick {
        page_index,
        render_x: normalized_x * render_width as f32,
        render_y: normalized_y * render_height as f32,
        normalized_x,
        normalized_y,
    })
}

fn text_hit_from_page_click(
    page: &TextPage,
    click: PagePreviewClick,
    zoom: f32,
) -> Option<TextSearchHit> {
    let (page_x, page_y) = page_point_from_preview_click(click, zoom)?;
    page.spans
        .iter()
        .enumerate()
        .filter(|(_, span)| {
            !span.text.trim().is_empty() && span.bbox.width > 0.0 && span.bbox.height > 0.0
        })
        .filter_map(|(index, span)| {
            let distance_sq = rect_distance_sq_to_point(&span.bbox, page_x, page_y);
            (distance_sq <= TEXT_CLICK_BBOX_TOLERANCE_PT * TEXT_CLICK_BBOX_TOLERANCE_PT)
                .then_some((index, span, distance_sq))
        })
        .min_by(|left, right| {
            left.2
                .partial_cmp(&right.2)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, span, _)| text_span_to_hit(page.page_index, index, span))
}

fn text_hit_for_text_fragments(page: &TextPage, fragments: &[String]) -> Option<TextSearchHit> {
    let mut matched_indices = Vec::new();
    let mut seen = HashSet::new();
    for fragment in fragments {
        for index in text_span_indices_for_fragment(page, fragment) {
            if seen.insert(index) {
                matched_indices.push(index);
            }
        }
    }

    let mut valid_indices = matched_indices.into_iter().filter(|index| {
        page.spans
            .get(*index)
            .is_some_and(|span| span.bbox.width > 0.0 && span.bbox.height > 0.0)
    });
    let first_index = valid_indices.next()?;
    let first_span = page.spans.get(first_index)?;
    let mut bbox = first_span.bbox.clone();
    let mut untrusted = first_span.untrusted;
    for index in valid_indices {
        if let Some(span) = page.spans.get(index) {
            union_page_rect_into(&mut bbox, &span.bbox);
            untrusted |= span.untrusted;
        }
    }

    Some(TextSearchHit {
        page_index: page.page_index,
        span_index: first_index,
        excerpt: fragments.join(" "),
        bbox: Some(bbox),
        untrusted,
    })
}

fn text_span_indices_for_fragment(page: &TextPage, fragment: &str) -> Vec<usize> {
    let fragment_key = normalized_text_match_key(fragment);
    if fragment_key.is_empty() {
        return Vec::new();
    }
    let span_keys = page
        .spans
        .iter()
        .map(|span| normalized_text_match_key(&span.text))
        .collect::<Vec<_>>();

    for (index, span_key) in span_keys.iter().enumerate() {
        if !span_key.is_empty() && span_key.contains(&fragment_key) {
            return vec![index];
        }
    }

    for start in 0..span_keys.len() {
        let mut combined = String::new();
        let mut indices = Vec::new();
        for (index, span_key) in span_keys.iter().enumerate().skip(start) {
            if span_key.is_empty() {
                continue;
            }
            if !combined.is_empty() {
                combined.push(' ');
            }
            combined.push_str(span_key);
            indices.push(index);
            if combined.contains(&fragment_key) {
                return indices;
            }
            if combined.len() > fragment_key.len().saturating_add(128) {
                break;
            }
        }
    }

    Vec::new()
}

fn visual_hit_from_page_click(
    page: &VisualPage,
    click: PagePreviewClick,
    zoom: f32,
) -> Option<PreviewVisualHit> {
    let (page_x, page_y) = page_point_from_preview_click(click, zoom)?;
    page.elements
        .iter()
        .enumerate()
        .filter(|(_, element)| element.bbox.width > 0.0 && element.bbox.height > 0.0)
        .filter_map(|(index, element)| {
            let contains = rect_contains_point(&element.bbox, page_x, page_y);
            let distance_sq = rect_distance_sq_to_point(&element.bbox, page_x, page_y);
            let tolerance_sq = VISUAL_CLICK_BBOX_TOLERANCE_PT * VISUAL_CLICK_BBOX_TOLERANCE_PT;
            (contains || distance_sq <= tolerance_sq).then_some((
                index,
                element,
                contains,
                distance_sq,
                rect_area(&element.bbox),
            ))
        })
        .min_by(|left, right| {
            let left_contains = left.2 as u8;
            let right_contains = right.2 as u8;
            right_contains
                .cmp(&left_contains)
                .then_with(|| {
                    left.4
                        .partial_cmp(&right.4)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    left.3
                        .partial_cmp(&right.3)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        })
        .map(|(index, element, contains_click, _, _)| {
            visual_element_to_hit(page.page_index, index, element, contains_click)
        })
}

fn visual_hit_for_object(page: &VisualPage, object: ObjectId) -> Option<PreviewVisualHit> {
    let mut matching = page.elements.iter().enumerate().filter(|(_, element)| {
        element.object == Some(object) && element.bbox.width > 0.0 && element.bbox.height > 0.0
    });
    let (first_index, first) = matching.next()?;
    let mut bbox = first.bbox.clone();
    let mut kind = first.kind;
    let mut untrusted = first.untrusted;
    for (_, element) in matching {
        union_page_rect_into(&mut bbox, &element.bbox);
        if element.kind != kind {
            kind = VisualElementKind::Unknown;
        }
        untrusted |= element.untrusted;
    }
    Some(PreviewVisualHit {
        page_index: page.page_index,
        element_index: first_index,
        kind,
        bbox,
        object: Some(object),
        untrusted,
        contains_click: false,
    })
}

fn visual_hit_for_page_visual_union(
    page: &VisualPage,
    object: ObjectId,
) -> Option<PreviewVisualHit> {
    let mut visible = page
        .elements
        .iter()
        .enumerate()
        .filter(|(_, element)| element.bbox.width > 0.0 && element.bbox.height > 0.0);
    let (first_index, first) = visible.next()?;
    let mut bbox = first.bbox.clone();
    let mut kind = first.kind;
    let mut untrusted = first.untrusted;
    for (_, element) in visible {
        union_page_rect_into(&mut bbox, &element.bbox);
        if element.kind != kind {
            kind = VisualElementKind::Unknown;
        }
        untrusted |= element.untrusted;
    }
    Some(PreviewVisualHit {
        page_index: page.page_index,
        element_index: first_index,
        kind,
        bbox,
        object: Some(object),
        untrusted,
        contains_click: false,
    })
}

fn visual_hit_for_element_indices(
    page: &VisualPage,
    indices: &[usize],
    object: ObjectId,
) -> Option<PreviewVisualHit> {
    let first_index = *indices.first()?;
    let first = page.elements.get(first_index)?;
    if first.bbox.width <= 0.0 || first.bbox.height <= 0.0 {
        return None;
    }
    let mut bbox = first.bbox.clone();
    let mut kind = first.kind;
    let mut untrusted = first.untrusted;
    for index in indices.iter().skip(1) {
        let Some(element) = page.elements.get(*index) else {
            continue;
        };
        if element.bbox.width <= 0.0 || element.bbox.height <= 0.0 {
            continue;
        }
        union_page_rect_into(&mut bbox, &element.bbox);
        if element.kind != kind {
            kind = VisualElementKind::Unknown;
        }
        untrusted |= element.untrusted;
    }
    Some(PreviewVisualHit {
        page_index: page.page_index,
        element_index: first_index,
        kind,
        bbox,
        object: Some(object),
        untrusted,
        contains_click: false,
    })
}

fn nice_stream_visual_hit_for_selection(
    page: &VisualPage,
    rows: &[NiceStreamRenderLine],
    selection_key: &str,
    object: ObjectId,
) -> Option<PreviewVisualHit> {
    if let Some(hit) = visual_hit_for_object(page, object) {
        return Some(hit);
    }

    let ops = nice_stream_visual_ops(rows);
    let selected_rows = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| nice_stream_row_matches_selection(row, selection_key))
        .map(|(index, _)| index)
        .collect::<HashSet<_>>();
    if selected_rows.is_empty() {
        return None;
    }

    let mut matched_indices = Vec::new();
    let mut seen = HashSet::new();
    for kind in nice_stream_selected_visual_kind_priority(&ops, &selected_rows) {
        let Some(first_selected_op_index) = ops
            .iter()
            .position(|op| selected_rows.contains(&op.row_index) && op.kind == kind)
        else {
            continue;
        };
        let before_count = ops[..first_selected_op_index]
            .iter()
            .filter(|op| visual_kind_matches(kind, op.kind))
            .count();
        let selected_count = ops
            .iter()
            .filter(|op| {
                selected_rows.contains(&op.row_index) && visual_kind_matches(kind, op.kind)
            })
            .count()
            .max(1);
        for index in page
            .elements
            .iter()
            .enumerate()
            .filter(|(_, element)| {
                element.bbox.width > 0.0
                    && element.bbox.height > 0.0
                    && visual_kind_matches(kind, element.kind)
            })
            .skip(before_count)
            .take(selected_count)
            .map(|(index, _)| index)
        {
            if seen.insert(index) {
                matched_indices.push(index);
            }
        }
    }

    if matched_indices.is_empty() {
        let first_selected_op_index = ops
            .iter()
            .position(|op| selected_rows.contains(&op.row_index))?;
        let before_count = ops[..first_selected_op_index].len();
        let selected_count = ops
            .iter()
            .filter(|op| selected_rows.contains(&op.row_index))
            .count()
            .max(1);
        matched_indices.extend(
            page.elements
                .iter()
                .enumerate()
                .filter(|(_, element)| element.bbox.width > 0.0 && element.bbox.height > 0.0)
                .skip(before_count)
                .take(selected_count)
                .map(|(index, _)| index),
        );
    }

    visual_hit_for_element_indices(page, &matched_indices, object)
}

fn nice_stream_selection_key_for_text_hit(
    rows: &[NiceStreamRenderLine],
    hit: &TextSearchHit,
) -> Option<String> {
    let query = normalized_text_match_key(&hit.excerpt);
    if query.is_empty() {
        return None;
    }

    rows.iter()
        .filter_map(|row| {
            nice_stream_line_text_fragments(&row.line.text)
                .into_iter()
                .filter_map(|fragment| {
                    let fragment_key = normalized_text_match_key(&fragment);
                    if fragment_key.is_empty() {
                        return None;
                    }
                    (fragment_key.contains(&query) || query.contains(&fragment_key))
                        .then_some(fragment_key.len().abs_diff(query.len()))
                })
                .min()
                .map(|score| (score, row.line_key.clone()))
        })
        .min_by_key(|(score, _)| *score)
        .map(|(_, key)| key)
}

fn nice_stream_selection_key_for_visual_hit(
    page: &VisualPage,
    rows: &[NiceStreamRenderLine],
    hit: &PreviewVisualHit,
) -> Option<String> {
    let element = page.elements.get(hit.element_index)?;
    let ops = nice_stream_visual_ops(rows);
    for expected_kind in nice_stream_reverse_visual_kind_candidates(element.kind) {
        let ordinal = page
            .elements
            .iter()
            .take(hit.element_index + 1)
            .filter(|element| {
                element.bbox.width > 0.0
                    && element.bbox.height > 0.0
                    && visual_kind_matches(expected_kind, element.kind)
            })
            .count()
            .checked_sub(1)?;
        if let Some(op) = ops
            .iter()
            .filter(|op| visual_kind_matches(expected_kind, op.kind))
            .nth(ordinal)
        {
            return rows.get(op.row_index).map(|row| row.line_key.clone());
        }
    }
    None
}

fn nice_stream_reverse_visual_kind_candidates(kind: VisualElementKind) -> Vec<VisualElementKind> {
    match kind {
        VisualElementKind::Text => vec![VisualElementKind::Text, VisualElementKind::Unknown],
        VisualElementKind::Image => vec![VisualElementKind::Image, VisualElementKind::Unknown],
        VisualElementKind::Vector | VisualElementKind::Grid => {
            vec![VisualElementKind::Vector, VisualElementKind::Unknown]
        }
        VisualElementKind::Unknown => vec![VisualElementKind::Unknown],
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NiceStreamVisualOp {
    row_index: usize,
    kind: VisualElementKind,
}

fn nice_stream_visual_ops(rows: &[NiceStreamRenderLine]) -> Vec<NiceStreamVisualOp> {
    rows.iter()
        .enumerate()
        .filter_map(|(row_index, row)| {
            nice_stream_line_visual_kind(&row.line.text)
                .map(|kind| NiceStreamVisualOp { row_index, kind })
        })
        .collect()
}

fn nice_stream_selection_has_non_text_visual_ops(
    rows: &[NiceStreamRenderLine],
    selection_key: &str,
) -> bool {
    rows.iter()
        .filter(|row| nice_stream_row_matches_selection(row, selection_key))
        .filter_map(|row| nice_stream_line_visual_kind(&row.line.text))
        .any(|kind| kind != VisualElementKind::Text)
}

fn nice_stream_selected_visual_kind_priority(
    ops: &[NiceStreamVisualOp],
    selected_rows: &HashSet<usize>,
) -> Vec<VisualElementKind> {
    let mut kinds = Vec::new();
    for op in ops
        .iter()
        .filter(|op| selected_rows.contains(&op.row_index))
    {
        if !kinds.contains(&op.kind) {
            kinds.push(op.kind);
        }
    }
    kinds
}

fn nice_stream_line_visual_kind(line: &str) -> Option<VisualElementKind> {
    let operator = nice_stream_line_operator(line)?;
    match operator.as_str() {
        "Tj" | "TJ" | "'" | "\"" => Some(VisualElementKind::Text),
        "Do" | "EI" => Some(VisualElementKind::Image),
        "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "sh" => {
            Some(VisualElementKind::Vector)
        }
        _ => None,
    }
}

fn nice_stream_line_operator(line: &str) -> Option<String> {
    pdf_content_tokens(line).pop()
}

fn visual_kind_matches(expected: VisualElementKind, actual: VisualElementKind) -> bool {
    match expected {
        VisualElementKind::Unknown => true,
        VisualElementKind::Vector => {
            matches!(actual, VisualElementKind::Vector | VisualElementKind::Grid)
        }
        _ => expected == actual,
    }
}

fn union_page_rect_into(target: &mut PageRect, rect: &PageRect) {
    let left = target.x.min(rect.x);
    let top = target.y.min(rect.y);
    let right = (target.x + target.width).max(rect.x + rect.width);
    let bottom = (target.y + target.height).max(rect.y + rect.height);
    target.x = left;
    target.y = top;
    target.width = (right - left).max(0.0);
    target.height = (bottom - top).max(0.0);
}

fn page_point_from_preview_click(click: PagePreviewClick, zoom: f32) -> Option<(f32, f32)> {
    (zoom > 0.0).then_some((click.render_x / zoom, click.render_y / zoom))
}

fn rect_contains_point(rect: &PageRect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
}

fn rect_area(rect: &PageRect) -> f32 {
    (rect.width.max(0.0)) * (rect.height.max(0.0))
}

fn rect_distance_sq_to_point(rect: &PageRect, x: f32, y: f32) -> f32 {
    let dx = if x < rect.x {
        rect.x - x
    } else if x > rect.x + rect.width {
        x - (rect.x + rect.width)
    } else {
        0.0
    };
    let dy = if y < rect.y {
        rect.y - y
    } else if y > rect.y + rect.height {
        y - (rect.y + rect.height)
    } else {
        0.0
    };
    dx * dx + dy * dy
}

fn text_span_to_hit(page_index: usize, span_index: usize, span: &TextSpan) -> TextSearchHit {
    TextSearchHit {
        page_index,
        span_index,
        excerpt: span.text.clone(),
        bbox: Some(span.bbox.clone()),
        untrusted: span.untrusted,
    }
}

fn visual_element_to_hit(
    page_index: usize,
    element_index: usize,
    element: &VisualElement,
    contains_click: bool,
) -> PreviewVisualHit {
    PreviewVisualHit {
        page_index,
        element_index,
        kind: element.kind,
        bbox: element.bbox.clone(),
        object: element.object,
        untrusted: element.untrusted,
        contains_click,
    }
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
        let render_max_dimension = render_max_dimension_or_default(options.render_max_dimension);
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
        let selected_row = tree.initial_selected_row();
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
            left_panel_width: None,
            right_panel_width: None,
            tree,
            stream: LargeStreamModel::default(),
            real_stream_preset: RealStreamPreset::Nice,
            real_stream_mode: StreamMode::Decoded,
            real_stream_view_mode: StreamViewMode::Text,
            real_stream_offset: 0,
            real_stream_limit: REAL_STREAM_DEFAULT_VIEW_LIMIT_BYTES,
            real_stream_key: None,
            real_stream_job: None,
            real_stream_chunk: None,
            real_stream_windows: VecDeque::new(),
            real_stream_collapsed_blocks: HashSet::new(),
            real_stream_selected_block: None,
            scroll_selected_nice_stream_row: false,
            real_stream_error: None,
            decoded_stream_cache: StreamChunkCache::new(
                DECODED_STREAM_CACHE_MAX_ITEMS,
                DECODED_STREAM_CACHE_MAX_BYTES,
            ),
            selected_row,
            scroll_selected_tree_row: false,
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
            render_max_dimension,
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
            visual_page_cache: VisualPageCache::new(
                VISUAL_CLICK_CACHE_MAX_PAGES,
                VISUAL_CLICK_CACHE_MAX_ELEMENTS,
            ),
            selected_text_hit: None,
            selected_visual_hit: None,
            pending_preview_stream_selection: None,
            preview_click: None,
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

    fn clear_preview_selection(&mut self) {
        self.preview_click = None;
        self.selected_text_hit = None;
        self.selected_visual_hit = None;
        self.pending_preview_stream_selection = None;
    }

    fn select_row_from_tree(&mut self, row: usize) {
        if self.selected_row == row {
            self.clear_preview_selection();
            self.expand_selected_real_tree_path();
            self.sync_render_page_for_tree_row(self.selected_row);
            self.select_visual_bbox_for_tree_row(self.selected_row);
            return;
        }
        self.selected_row = row;
        self.clear_preview_selection();
        self.forward_stack.clear();
        self.refresh_real_detail_for_selection();
        self.expand_selected_real_tree_path();
        self.sync_render_page_for_tree_row(self.selected_row);
        self.select_visual_bbox_for_tree_row(self.selected_row);
        self.status_log.push(format!(
            "selected {}",
            self.tree.row_label(self.selected_row)
        ));
    }

    fn follow_reference(&mut self, row: usize) {
        if self.selected_row == row || row >= self.tree.row_count() {
            return;
        }
        self.back_stack.push(self.selected_row);
        self.forward_stack.clear();
        self.selected_row = row;
        self.clear_preview_selection();
        self.selected_tab = InspectorTab::Object;
        self.refresh_real_detail_for_selection();
        self.expand_selected_real_tree_path();
        self.sync_render_page_for_tree_row(self.selected_row);
        self.select_visual_bbox_for_tree_row(self.selected_row);
        self.status_log.push(format!(
            "resolved reference to {}",
            self.tree.row_label(self.selected_row)
        ));
    }

    fn go_back(&mut self) {
        if let Some(row) = self.back_stack.pop() {
            self.forward_stack.push(self.selected_row);
            self.selected_row = row;
            self.clear_preview_selection();
            self.refresh_real_detail_for_selection();
            self.expand_selected_real_tree_path();
            self.sync_render_page_for_tree_row(self.selected_row);
            self.select_visual_bbox_for_tree_row(self.selected_row);
            self.status_log.push(format!(
                "back to {}",
                self.tree.row_label(self.selected_row)
            ));
        }
    }

    fn go_forward(&mut self) {
        if let Some(row) = self.forward_stack.pop() {
            self.back_stack.push(self.selected_row);
            self.selected_row = row;
            self.clear_preview_selection();
            self.refresh_real_detail_for_selection();
            self.expand_selected_real_tree_path();
            self.sync_render_page_for_tree_row(self.selected_row);
            self.select_visual_bbox_for_tree_row(self.selected_row);
            self.status_log.push(format!(
                "forward to {}",
                self.tree.row_label(self.selected_row)
            ));
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
            self.selected_visual_hit = None;
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
        self.selected_visual_hit = None;
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
        self.set_render_page(hit.page_index);
        self.selected_text_hit = Some(hit.clone());
        self.selected_visual_hit = None;
        self.status_log.push(format!(
            "opened text search hit page {} span {}",
            hit.page_index + 1,
            hit.span_index
        ));
    }

    fn expand_selected_real_row(&mut self) -> usize {
        let inserted = self.expand_real_tree_row(self.selected_row);
        if inserted > 0 {
            self.status_log.push(format!(
                "expanded {} bounded children under {}",
                inserted,
                self.tree.row_label(self.selected_row)
            ));
        }
        inserted
    }

    fn expand_real_tree_row(&mut self, row: usize) -> usize {
        let root_inserted = match &mut self.tree {
            TreeModel::Real(tree) => tree.expand_cached_document_root(row),
            TreeModel::Virtual(_) => 0,
        };
        if root_inserted > 0 {
            return root_inserted;
        }

        let Some(id) = (match &self.tree {
            TreeModel::Real(tree) => tree.summary(row).map(|summary| summary.id.clone()),
            TreeModel::Virtual(_) => None,
        }) else {
            return 0;
        };
        let Ok(state) = &self.state else {
            return 0;
        };
        let detail = match load_object_detail(state, &id) {
            Ok(detail) => detail,
            Err(err) => {
                if self.selected_row == row {
                    self.real_detail = None;
                    self.real_detail_error = Some(err.clone());
                }
                self.status_log
                    .push(format!("expand {} failed: {err}", self.tree.row_label(row)));
                return 0;
            }
        };
        let inserted = match &mut self.tree {
            TreeModel::Real(tree) => {
                tree.update_row_from_detail(row, &detail);
                tree.expand_row_from_detail(row, &detail)
            }
            TreeModel::Virtual(_) => 0,
        };
        if self.selected_row == row {
            self.real_detail = Some(detail);
            self.real_detail_error = None;
        }
        inserted
    }

    fn expand_selected_real_tree_path(&mut self) {
        let selected_id = match &self.tree {
            TreeModel::Real(tree) => tree
                .summary(self.selected_row)
                .map(|summary| summary.id.clone()),
            TreeModel::Virtual(_) => None,
        };
        let Some(selected_id) = selected_id else {
            return;
        };

        self.expand_real_tree_row(self.selected_row);
        if let TreeModel::Real(tree) = &mut self.tree {
            if let Some(row) = tree.collapse_expanded_rows_except_selected_path(&selected_id) {
                self.selected_row = row;
            }
        }
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
        self.real_stream_windows.clear();
        self.real_stream_collapsed_blocks.clear();
        self.real_stream_selected_block = None;
        self.scroll_selected_nice_stream_row = false;
        self.real_stream_error = None;
    }

    fn refresh_real_stream_chunk(&mut self, object: ObjectId) {
        let key = RealStreamKey {
            object,
            mode: self.real_stream_mode,
            offset: self.real_stream_offset,
            limit: self.real_stream_limit,
        };
        if self
            .real_stream_job
            .as_ref()
            .is_some_and(|job| job.key == key)
        {
            return;
        }
        if let Some(chunk) = self.real_stream_cached_window(key) {
            self.real_stream_key = Some(key);
            self.real_stream_chunk = Some(chunk);
            self.real_stream_error = None;
            return;
        }
        if self.real_stream_key == Some(key) && self.real_stream_job.is_some() {
            return;
        }
        if let Some(job) = self.real_stream_job.take() {
            job.cancel.cancel();
        }
        self.real_stream_key = Some(key);
        if self.real_stream_windows.is_empty() {
            self.real_stream_chunk = None;
        }
        self.real_stream_error = None;

        if key.mode == StreamMode::Decoded {
            if let Some(chunk) = self.decoded_stream_cache.get(&key) {
                self.insert_real_stream_window(key, chunk.clone());
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

    fn real_stream_cached_window(&self, key: RealStreamKey) -> Option<StreamChunk> {
        self.real_stream_windows
            .iter()
            .find(|window| window.key == key)
            .map(|window| window.chunk.clone())
    }

    fn insert_real_stream_window(&mut self, key: RealStreamKey, chunk: StreamChunk) {
        if let Some(window) = self
            .real_stream_windows
            .iter_mut()
            .find(|window| window.key == key)
        {
            window.chunk = chunk;
        } else {
            self.real_stream_windows
                .push_back(RealStreamLoadedWindow { key, chunk });
        }
        self.real_stream_windows
            .make_contiguous()
            .sort_by_key(|window| window.key.offset);
        while self.real_stream_windows.len() > REAL_STREAM_MAX_LOADED_WINDOWS {
            let front_distance = self
                .real_stream_windows
                .front()
                .map(|window| key.offset.saturating_sub(window.key.offset))
                .unwrap_or(0);
            let back_distance = self
                .real_stream_windows
                .back()
                .map(|window| window.key.offset.saturating_sub(key.offset))
                .unwrap_or(0);
            if front_distance > back_distance {
                self.real_stream_windows.pop_front();
            } else {
                self.real_stream_windows.pop_back();
            }
        }
    }

    fn apply_real_stream_preset(&mut self, stream: &StreamSummary) -> bool {
        let (mode, view_mode) =
            real_stream_preset_defaults(self.real_stream_preset, stream.can_decode);
        let limit = real_stream_default_limit(stream, mode);
        let mut changed = false;
        if self.real_stream_mode != mode {
            self.real_stream_mode = mode;
            changed = true;
        }
        if self.real_stream_view_mode != view_mode {
            self.real_stream_view_mode = view_mode;
            changed = true;
        }
        if self.real_stream_offset != 0 {
            self.real_stream_offset = 0;
            changed = true;
        }
        if self.real_stream_limit != limit {
            self.real_stream_limit = limit;
            changed = true;
        }
        changed
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
                        self.insert_real_stream_window(output.key, chunk.clone());
                        self.real_stream_chunk = Some(chunk);
                        self.real_stream_error = None;
                        if self.pending_preview_stream_selection.is_some() {
                            self.select_nice_stream_code_for_preview_selection();
                        }
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

    fn sync_render_page_for_tree_row(&mut self, row: usize) {
        let Some(page_index) = self.tree.real_row_page_index(row) else {
            return;
        };
        self.set_render_page(page_index);
    }

    fn current_render_key(&self) -> Option<RealRenderKey> {
        if !self.tree.is_real() || self.page_count() == 0 {
            return None;
        }
        Some(RealRenderKey::new(
            self.render_page_index,
            self.render_zoom,
            self.render_rotation_degrees,
            self.render_max_dimension,
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
        self.clear_preview_selection();
        self.refresh_real_render();
    }

    fn set_render_page_from_pager(&mut self, page_index: usize) {
        self.set_render_page(page_index);
        let page_index = self.render_page_index;
        self.sync_tree_to_render_page(page_index);
    }

    fn handle_page_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        let page_count = self.page_count();
        if page_count == 0 || ctx.egui_wants_keyboard_input() {
            return;
        }

        let shortcut = ctx.input(|input| {
            if input.modifiers.any() {
                return None;
            }
            if input.key_pressed(egui::Key::ArrowLeft) {
                Some(PageKeyboardShortcut::Previous)
            } else if input.key_pressed(egui::Key::ArrowRight) {
                Some(PageKeyboardShortcut::Next)
            } else if input.key_pressed(egui::Key::ArrowUp) {
                Some(PageKeyboardShortcut::First)
            } else if input.key_pressed(egui::Key::ArrowDown) {
                Some(PageKeyboardShortcut::Last)
            } else {
                None
            }
        });

        if let Some(page_index) = shortcut
            .and_then(|shortcut| {
                page_keyboard_target_page(self.render_page_index, page_count, shortcut)
            })
            .filter(|page_index| *page_index != self.render_page_index)
        {
            self.set_render_page_from_pager(page_index);
            self.status_log.push(format!(
                "keyboard switched preview to page {}",
                self.render_page_index + 1
            ));
        }
    }

    fn sync_tree_to_render_page(&mut self, page_index: usize) -> Option<usize> {
        if !self.tree.is_real() {
            return None;
        }
        let row = self.ensure_tree_page_row(page_index)?;
        self.select_row_from_tree(row);
        self.scroll_selected_tree_row = true;
        self.status_log
            .push(format!("synced document tree to page {}", page_index + 1));
        Some(row)
    }

    fn ensure_tree_page_row(&mut self, page_index: usize) -> Option<usize> {
        if let Some(row) = self.tree.real_row_for_page_index(page_index) {
            return Some(row);
        }

        let page_root_row = self.tree.real_page_root_row()?;
        self.expand_real_tree_row(page_root_row);
        if let Some(row) = self.tree.real_row_for_page_index(page_index) {
            return Some(row);
        }

        let summary = self.load_page_summary_for_tree(page_root_row, page_index)?;
        self.tree.ensure_real_page_child_row(page_root_row, summary)
    }

    fn ensure_page_content_stream_row(&mut self, page_index: usize) -> Option<usize> {
        let page_row = self.ensure_tree_page_row(page_index)?;
        self.expand_real_tree_row(page_row);
        for _ in 0..4 {
            if let Some(row) = self.tree.real_first_page_content_stream_row(page_row) {
                return Some(row);
            }
            let candidates = self.tree.real_page_content_candidate_rows(page_row);
            let mut expanded_any = false;
            for row in candidates {
                if self.expand_real_tree_row(row) > 0 {
                    expanded_any = true;
                    break;
                }
            }
            if !expanded_any {
                break;
            }
        }
        None
    }

    fn load_page_summary_for_tree(
        &mut self,
        page_root_row: usize,
        page_index: usize,
    ) -> Option<ObjectSummary> {
        let page_root_id = match &self.tree {
            TreeModel::Real(tree) => tree
                .summary(page_root_row)
                .map(|summary| summary.id.clone()),
            TreeModel::Virtual(_) => None,
        }?;
        let Ok(state) = self.state.as_ref() else {
            return None;
        };
        match state.session.run_task(|document| {
            document.children(
                &page_root_id,
                ChildRange {
                    offset: page_index,
                    limit: 1,
                },
                ChildContainer::Array,
            )
        }) {
            Ok(page) => page.items.into_iter().next(),
            Err(err) => {
                self.status_log.push(format!(
                    "page {} tree row load failed: {}",
                    page_index + 1,
                    err.message
                ));
                None
            }
        }
    }

    fn select_text_hit_for_preview_click(&mut self, click: PagePreviewClick) {
        self.selected_text_hit = None;
        if self.render_rotation_degrees != 0 {
            self.status_log
                .push("preview text bbox hit-test is only available at 0 deg rotation".to_string());
            return;
        }

        let page = match self.text_page_for_preview(click.page_index) {
            Ok(page) => page,
            Err(err) => {
                self.status_log.push(format!(
                    "preview text bbox hit-test page {} failed: {err}",
                    click.page_index + 1
                ));
                return;
            }
        };

        if let Some(hit) = text_hit_from_page_click(&page, click, self.render_zoom) {
            self.status_log.push(format!(
                "selected text bbox page {} span {}",
                hit.page_index + 1,
                hit.span_index
            ));
            self.selected_text_hit = Some(hit);
        } else {
            self.status_log.push(format!(
                "no text bbox at page {} preview click",
                click.page_index + 1
            ));
        }
    }

    fn select_nice_stream_selection_for_preview(
        &mut self,
        object: ObjectId,
        selection_key: &str,
        rows: &[NiceStreamRenderLine],
    ) {
        self.selected_text_hit = None;
        self.selected_visual_hit = None;
        if self.render_rotation_degrees != 0 {
            self.status_log
                .push("nice stream bbox highlight is only available at 0 deg rotation".to_string());
            return;
        }

        let page_index = self.render_page_index;
        let fragments = nice_stream_text_fragments_for_selection(rows, selection_key);
        let mut highlighted_text = false;
        if !fragments.is_empty() {
            match self.text_page_for_preview(page_index) {
                Ok(page) => {
                    if let Some(hit) = text_hit_for_text_fragments(&page, &fragments) {
                        self.status_log.push(format!(
                            "highlighted page {} text bbox from nice stream",
                            page_index + 1
                        ));
                        self.selected_text_hit = Some(hit);
                        highlighted_text = true;
                    } else {
                        self.status_log.push(format!(
                            "no positioned text bbox for selected nice stream text on page {}",
                            page_index + 1
                        ));
                    }
                }
                Err(err) => {
                    self.status_log.push(format!(
                        "nice stream text bbox page {} failed: {err}",
                        page_index + 1
                    ));
                }
            }
        }

        let needs_visual_highlight =
            !highlighted_text || nice_stream_selection_has_non_text_visual_ops(rows, selection_key);
        if needs_visual_highlight {
            match self.visual_page_for_preview(page_index) {
                Ok(page) => {
                    if let Some(hit) =
                        nice_stream_visual_hit_for_selection(&page, rows, selection_key, object)
                    {
                        self.status_log.push(format!(
                            "highlighted page {} visual bbox from nice stream",
                            page_index + 1
                        ));
                        self.selected_visual_hit = Some(hit);
                    } else if !highlighted_text {
                        self.status_log.push(format!(
                            "no visual bbox for selected nice stream object on page {}",
                            page_index + 1
                        ));
                    }
                }
                Err(err) => {
                    self.status_log.push(format!(
                        "nice stream visual bbox page {} failed: {err}",
                        page_index + 1
                    ));
                }
            }
        }

        if !highlighted_text && self.selected_visual_hit.is_none() && fragments.is_empty() {
            self.status_log
                .push("selected nice stream line has no previewable drawing operation".to_string());
        }
    }

    fn select_visual_hit_for_preview_click(&mut self, click: PagePreviewClick) {
        self.selected_visual_hit = None;
        if self.render_rotation_degrees != 0 {
            self.status_log.push(
                "preview visual bbox hit-test is only available at 0 deg rotation".to_string(),
            );
            return;
        }

        let page = match self.visual_page_for_preview(click.page_index) {
            Ok(page) => page,
            Err(err) => {
                self.status_log.push(format!(
                    "preview visual bbox hit-test page {} failed: {err}",
                    click.page_index + 1
                ));
                return;
            }
        };

        if let Some(hit) = visual_hit_from_page_click(&page, click, self.render_zoom) {
            self.status_log.push(format!(
                "selected visual bbox page {} element {}",
                hit.page_index + 1,
                hit.element_index
            ));
            self.selected_visual_hit = Some(hit);
        } else {
            self.status_log.push(format!(
                "no visual bbox at page {} preview click",
                click.page_index + 1
            ));
        }
    }

    fn select_nice_stream_code_for_preview_selection(&mut self) -> bool {
        let Some(stream) = self
            .real_detail
            .as_ref()
            .and_then(|detail| detail.stream.clone())
        else {
            return false;
        };
        let Some(chunks) = self.loaded_nice_stream_chunks(stream.object) else {
            return false;
        };
        let rows = real_stream_nice_render_lines(stream.object, &chunks);
        let selection = self.current_preview_stream_selection();
        let text_hit = selection.text_hit;
        let visual_hit = selection.visual_hit;
        let page_index = selection.page_index;

        let selection_key = text_hit
            .as_ref()
            .filter(|hit| hit.page_index == page_index)
            .and_then(|hit| nice_stream_selection_key_for_text_hit(&rows, hit))
            .or_else(|| {
                let hit = visual_hit
                    .as_ref()
                    .filter(|hit| hit.page_index == page_index)?;
                let page = self.visual_page_for_preview(hit.page_index).ok()?;
                nice_stream_selection_key_for_visual_hit(&page, &rows, hit)
            });

        let Some(selection_key) = selection_key else {
            return false;
        };

        self.expand_nice_stream_selection_path(&rows, &selection_key);
        self.real_stream_selected_block = Some(selection_key);
        self.scroll_selected_nice_stream_row = true;
        self.pending_preview_stream_selection = None;
        self.status_log
            .push("highlighted nice stream code from preview click".to_string());
        true
    }

    fn current_preview_stream_selection(&self) -> PendingPreviewStreamSelection {
        self.pending_preview_stream_selection
            .clone()
            .unwrap_or_else(|| PendingPreviewStreamSelection {
                page_index: self.render_page_index,
                text_hit: self.selected_text_hit.clone(),
                visual_hit: self.selected_visual_hit.clone(),
            })
    }

    fn open_nice_stream_for_preview_selection(&mut self, page_index: usize) -> bool {
        if self.selected_text_hit.is_none() && self.selected_visual_hit.is_none() {
            self.pending_preview_stream_selection = None;
            return false;
        }

        self.pending_preview_stream_selection = Some(PendingPreviewStreamSelection {
            page_index,
            text_hit: self.selected_text_hit.clone(),
            visual_hit: self.selected_visual_hit.clone(),
        });

        if self.select_nice_stream_code_for_preview_selection() {
            self.selected_tab = InspectorTab::Stream;
            return true;
        }

        let Some(row) = self.ensure_page_content_stream_row(page_index) else {
            self.status_log.push(format!(
                "no content stream tree row available for page {} preview selection",
                page_index + 1
            ));
            self.pending_preview_stream_selection = None;
            return false;
        };

        self.select_stream_row_from_preview(row);
        self.selected_tab = InspectorTab::Stream;
        self.prepare_real_stream_for_preview_selection()
    }

    fn select_stream_row_from_preview(&mut self, row: usize) {
        if self.selected_row != row {
            self.selected_row = row;
            self.forward_stack.clear();
        }
        self.refresh_real_detail_for_selection();
        self.expand_selected_real_tree_path();
        self.scroll_selected_tree_row = true;
    }

    fn prepare_real_stream_for_preview_selection(&mut self) -> bool {
        let Some(stream) = self
            .real_detail
            .as_ref()
            .and_then(|detail| detail.stream.clone())
        else {
            self.pending_preview_stream_selection = None;
            return false;
        };
        if !stream.can_decode {
            self.status_log.push(format!(
                "stream {} cannot decode for preview-to-Nice View selection",
                object_ref_text(stream.object)
            ));
            self.pending_preview_stream_selection = None;
            return false;
        }

        let limit = real_stream_default_limit(&stream, StreamMode::Decoded);
        let request_changed = self.real_stream_preset != RealStreamPreset::Nice
            || self.real_stream_mode != StreamMode::Decoded
            || self.real_stream_view_mode != StreamViewMode::Text
            || self.real_stream_offset != 0
            || self.real_stream_limit != limit
            || self
                .real_stream_key
                .is_some_and(|key| key.object != stream.object || key.mode != StreamMode::Decoded);

        self.real_stream_preset = RealStreamPreset::Nice;
        self.real_stream_mode = StreamMode::Decoded;
        self.real_stream_view_mode = StreamViewMode::Text;
        self.real_stream_offset = 0;
        self.real_stream_limit = limit;
        if request_changed {
            self.clear_real_stream_chunk();
        }

        self.refresh_real_stream_chunk(stream.object);
        self.select_nice_stream_code_for_preview_selection();
        true
    }

    fn loaded_nice_stream_chunks(&self, object: ObjectId) -> Option<Vec<StreamChunk>> {
        if self.real_stream_preset != RealStreamPreset::Nice
            || self.real_stream_mode != StreamMode::Decoded
            || self.real_stream_view_mode != StreamViewMode::Text
        {
            return None;
        }
        let key = self.real_stream_key?;
        if key.object != object || key.mode != StreamMode::Decoded {
            return None;
        }

        if !self.real_stream_windows.is_empty() {
            let chunks = self
                .real_stream_windows
                .iter()
                .filter(|window| {
                    window.key.object == object && window.key.mode == StreamMode::Decoded
                })
                .map(|window| window.chunk.clone())
                .collect::<Vec<_>>();
            return (!chunks.is_empty()).then_some(chunks);
        }

        self.real_stream_chunk.clone().map(|chunk| vec![chunk])
    }

    fn expand_nice_stream_selection_path(&mut self, rows: &[NiceStreamRenderLine], key: &str) {
        let Some(row) = rows
            .iter()
            .find(|row| row.line_key == key || row.block_key.as_deref() == Some(key))
        else {
            return;
        };
        for (_, block_key) in &row.guide_blocks {
            self.real_stream_collapsed_blocks.remove(block_key);
        }
        if let Some(block_key) = &row.block_key {
            self.real_stream_collapsed_blocks.remove(block_key);
        }
        if row.line.block_open {
            self.real_stream_collapsed_blocks.remove(&row.line_key);
        }
    }

    fn select_visual_bbox_for_tree_row(&mut self, row: usize) {
        let Some(target) = self.tree.real_row_visual_target(row) else {
            return;
        };
        if self.render_rotation_degrees != 0 {
            self.status_log
                .push("tree visual bbox highlight is only available at 0 deg rotation".to_string());
            return;
        }

        let page = match self.visual_page_for_preview(target.page_index) {
            Ok(page) => page,
            Err(err) => {
                self.status_log.push(format!(
                    "tree visual bbox page {} failed: {err}",
                    target.page_index + 1
                ));
                return;
            }
        };

        if let Some(hit) = visual_hit_for_object(&page, target.object) {
            self.selected_text_hit = None;
            self.selected_visual_hit = Some(hit);
            self.status_log.push(format!(
                "highlighted page {} visual bbox for {}",
                target.page_index + 1,
                object_ref_text(target.object)
            ));
            return;
        }

        if target.allow_page_union {
            if let Some(hit) = visual_hit_for_page_visual_union(&page, target.object) {
                self.selected_text_hit = None;
                self.selected_visual_hit = Some(hit);
                self.status_log.push(format!(
                    "highlighted page {} content stream bbox for {}",
                    target.page_index + 1,
                    object_ref_text(target.object)
                ));
            }
        }
    }

    fn visual_page_for_preview(&mut self, page_index: usize) -> Result<VisualPage, String> {
        if let Some(page) = self.visual_page_cache.get(page_index) {
            return Ok(page);
        }
        let state = self
            .state
            .as_ref()
            .map_err(|_| "document is not open".to_string())?;
        let mut request = VisualRequest::page(page_index);
        request.max_elements = VISUAL_CLICK_MAX_ELEMENTS_PER_PAGE;
        let page = state
            .session
            .run_task(|document| document.extract_visuals(&request))
            .map_err(|err| err.message)?;
        self.visual_page_cache.insert(page.clone());
        Ok(page)
    }

    fn text_page_for_preview(&mut self, page_index: usize) -> Result<TextPage, String> {
        if let Some(page) = self.text_search_cache.get(page_index) {
            return Ok(page);
        }
        let state = self
            .state
            .as_ref()
            .map_err(|_| "document is not open".to_string())?;
        let mut request = TextRequest::page(page_index);
        request.max_chars = TEXT_SEARCH_MAX_CHARS_PER_PAGE;
        request.max_blocks = TEXT_SEARCH_MAX_BLOCKS_PER_PAGE;
        let page = state
            .session
            .run_task(|document| document.extract_text(&request))
            .map_err(|err| err.message)?;
        self.text_search_cache.insert(page.clone());
        Ok(page)
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
                        if err.contains(RENDER_DIMENSION_LIMIT_ERROR)
                            && output.key.zoom() > DEFAULT_RENDER_ZOOM
                        {
                            self.render_zoom = DEFAULT_RENDER_ZOOM;
                            self.real_render = None;
                            self.real_render_texture = None;
                            self.real_render_error = None;
                            self.real_render_key = None;
                            self.status_log.push(format!(
                                "page {} render exceeded bounds at {:.0}%; retrying at 100%",
                                output.key.page_index + 1,
                                output.key.zoom() * 100.0
                            ));
                            self.refresh_real_render();
                            return;
                        }
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
        font_id: mono_font_id(DENSE_ROW_FONT_SIZE),
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

fn tree_kind_badge_text(summary: &ObjectSummary) -> &'static str {
    if (matches!(summary.kind, ObjectKind::Page) && is_page_list_summary(summary))
        || is_xref_object_summary(summary)
    {
        return kind_badge_text(&ObjectKind::Dict);
    }
    kind_badge_text(&summary.kind)
}

fn is_page_list_summary(summary: &ObjectSummary) -> bool {
    matches!(&summary.id, NodeId::Page { .. })
        || matches!(&summary.id, NodeId::ArrayEntry { parent, .. } if is_page_root_node(parent))
}

fn is_xref_object_summary(summary: &ObjectSummary) -> bool {
    matches!(&summary.id, NodeId::XrefObject { .. })
        || (matches!(summary.kind, ObjectKind::XrefEntry) && summary.object.is_some())
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

fn visual_object_attribution_text(hit: Option<&PreviewVisualHit>) -> Option<String> {
    hit.and_then(|hit| hit.object.map(object_ref_text))
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

fn text_hit_bbox_image_rect(
    hit: &TextSearchHit,
    image_rect: egui::Rect,
    render_width: u32,
    render_height: u32,
    zoom: f32,
    rotation_degrees: i32,
) -> Option<egui::Rect> {
    if rotation_degrees != 0 || render_width == 0 || render_height == 0 || zoom <= 0.0 {
        return None;
    }
    let bbox = hit.bbox.as_ref()?;
    page_bbox_image_rect(bbox, image_rect, render_width, render_height, zoom)
}

fn visual_hit_bbox_image_rect(
    hit: &PreviewVisualHit,
    image_rect: egui::Rect,
    render_width: u32,
    render_height: u32,
    zoom: f32,
    rotation_degrees: i32,
) -> Option<egui::Rect> {
    if rotation_degrees != 0 || render_width == 0 || render_height == 0 || zoom <= 0.0 {
        return None;
    }
    page_bbox_image_rect(&hit.bbox, image_rect, render_width, render_height, zoom)
}

fn page_bbox_image_rect(
    bbox: &PageRect,
    image_rect: egui::Rect,
    render_width: u32,
    render_height: u32,
    zoom: f32,
) -> Option<egui::Rect> {
    if bbox.width <= 0.0 || bbox.height <= 0.0 {
        return None;
    }
    let left = image_rect.left() + (bbox.x * zoom / render_width as f32) * image_rect.width();
    let top = image_rect.top() + (bbox.y * zoom / render_height as f32) * image_rect.height();
    let right = image_rect.left()
        + ((bbox.x + bbox.width) * zoom / render_width as f32) * image_rect.width();
    let bottom = image_rect.top()
        + ((bbox.y + bbox.height) * zoom / render_height as f32) * image_rect.height();
    Some(egui::Rect::from_min_max(
        egui::pos2(left, top),
        egui::pos2(right, bottom),
    ))
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
            dense_label(ui, "object");
            ui.label(dense_monospace_text(format!(
                "{} {} R",
                stream.object.num, stream.object.gen
            )));
            ui.end_row();
            dense_label(ui, "filters");
            ui.label(dense_monospace_text(if stream.filters.is_empty() {
                "-".to_string()
            } else {
                stream.filters.join(", ")
            }));
            ui.end_row();
            dense_label(ui, "raw size");
            ui.label(dense_monospace_text(optional_u64(stream.raw_size_hint)));
            ui.end_row();
            dense_label(ui, "decoded size");
            ui.label(dense_monospace_text(optional_u64(stream.decoded_size_hint)));
            ui.end_row();
            dense_label(ui, "can decode");
            ui.label(dense_monospace_text(stream.can_decode.to_string()));
            ui.end_row();
            dense_label(ui, "image preview");
            ui.label(dense_monospace_text(
                stream.image_preview_available.to_string(),
            ));
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

fn real_stream_preset_label(preset: RealStreamPreset) -> &'static str {
    match preset {
        RealStreamPreset::Nice => "Nice View",
        RealStreamPreset::Raw => "Raw View",
    }
}

fn real_stream_preset_defaults(
    preset: RealStreamPreset,
    can_decode: bool,
) -> (StreamMode, StreamViewMode) {
    match preset {
        RealStreamPreset::Nice if can_decode => (StreamMode::Decoded, StreamViewMode::Text),
        RealStreamPreset::Nice => (StreamMode::Raw, StreamViewMode::Text),
        RealStreamPreset::Raw => (StreamMode::Raw, StreamViewMode::Hex),
    }
}

fn real_stream_default_limit(stream: &StreamSummary, mode: StreamMode) -> usize {
    let size_hint = match mode {
        StreamMode::Raw => stream.raw_size_hint,
        StreamMode::Decoded => stream.decoded_size_hint,
    };
    size_hint
        .and_then(|size| usize::try_from(size).ok())
        .map(|size| size.clamp(1, REAL_STREAM_DEFAULT_VIEW_LIMIT_BYTES))
        .unwrap_or(REAL_STREAM_DEFAULT_VIEW_LIMIT_BYTES)
}

fn real_stream_chunk_has_more(chunk: &StreamChunk) -> bool {
    let loaded_end = chunk.offset.saturating_add(chunk.bytes.len() as u64);
    chunk
        .total_size
        .map(|total| loaded_end < total)
        .unwrap_or(chunk.truncated)
}

fn real_stream_chunks_range_label(chunks: &[StreamChunk]) -> String {
    let Some(first) = chunks.first() else {
        return "no bytes loaded".to_string();
    };
    let Some(last) = chunks.last() else {
        return "no bytes loaded".to_string();
    };
    let loaded_end = last.offset.saturating_add(last.bytes.len() as u64);
    let total = last
        .total_size
        .map(|total| format!(" / total {total}"))
        .unwrap_or_default();
    let more = if real_stream_chunk_has_more(last) {
        " / more"
    } else {
        ""
    };
    format!("bytes {}..{}{total}{more}", first.offset, loaded_end)
}

fn real_stream_chunks_has_more(chunks: &[StreamChunk]) -> bool {
    chunks.last().is_some_and(real_stream_chunk_has_more)
}

fn real_stream_chunks_visible_text(
    chunks: &[StreamChunk],
    view_mode: StreamViewMode,
    preset: RealStreamPreset,
) -> String {
    chunks
        .iter()
        .map(|chunk| real_stream_visible_text(chunk, view_mode, preset))
        .collect::<Vec<_>>()
        .join("\n")
}

fn real_stream_chunks_nice_lines(chunks: &[StreamChunk]) -> Vec<NiceStreamLine> {
    chunks
        .iter()
        .flat_map(|chunk| pdf_content_stream_nice_lines(&chunk.bytes))
        .collect()
}

fn real_stream_nice_render_lines(
    object: ObjectId,
    chunks: &[StreamChunk],
) -> Vec<NiceStreamRenderLine> {
    let mut block_stack: Vec<(usize, String)> = Vec::new();
    let mut rows = Vec::new();
    for (index, line) in real_stream_chunks_nice_lines(chunks)
        .into_iter()
        .enumerate()
    {
        let line_key = nice_stream_line_key(object, chunks, index, &line);
        if line.block_close {
            while block_stack
                .last()
                .is_some_and(|(indent, _)| *indent > line.indent)
            {
                block_stack.pop();
            }
        } else {
            while block_stack
                .last()
                .is_some_and(|(indent, _)| *indent >= line.indent)
            {
                block_stack.pop();
            }
        }
        let mut guide_blocks = block_stack.clone();
        if line.block_open {
            guide_blocks.push((line.indent, line_key.clone()));
        }
        let block_key = if line.block_open {
            Some(line_key.clone())
        } else {
            block_stack.last().map(|(_, key)| key.clone())
        };

        rows.push(NiceStreamRenderLine {
            line_key: line_key.clone(),
            block_key,
            guide_blocks,
            line: line.clone(),
        });

        if line.block_open {
            block_stack.push((line.indent, line_key));
        } else if line.block_close
            && block_stack
                .last()
                .is_some_and(|(indent, _)| *indent == line.indent)
        {
            block_stack.pop();
        }
    }
    rows
}

fn nice_stream_row_matches_selection(row: &NiceStreamRenderLine, selection_key: &str) -> bool {
    row.line_key == selection_key || row.block_key.as_deref() == Some(selection_key)
}

fn nice_stream_text_fragments_for_selection(
    rows: &[NiceStreamRenderLine],
    selection_key: &str,
) -> Vec<String> {
    let mut fragments = Vec::new();
    for row in rows {
        if nice_stream_row_matches_selection(row, selection_key) {
            fragments.extend(nice_stream_line_text_fragments(&row.line.text));
        }
    }
    dedupe_text_fragments(fragments)
}

fn nice_stream_line_text_fragments(line: &str) -> Vec<String> {
    let tokens = pdf_content_tokens(line);
    let Some(operator) = tokens.last().map(String::as_str) else {
        return Vec::new();
    };
    match operator {
        "Tj" | "TJ" | "'" | "\"" => {
            let mut fragments = Vec::new();
            for token in &tokens[..tokens.len().saturating_sub(1)] {
                push_pdf_text_fragments_from_token(token, &mut fragments);
            }
            fragments
        }
        _ => Vec::new(),
    }
}

fn push_pdf_text_fragments_from_token(token: &str, out: &mut Vec<String>) {
    if token.starts_with('(') {
        let text = decode_pdf_literal_string_token(token);
        if is_useful_pdf_text_fragment(&text) {
            out.push(text);
        }
    } else if token.starts_with('<') && !token.starts_with("<<") {
        let text = decode_pdf_hex_string_token(token);
        if is_useful_pdf_text_fragment(&text) {
            out.push(text);
        }
    } else if token.starts_with('[') && token.ends_with(']') {
        let inner = &token[1..token.len().saturating_sub(1)];
        for nested in pdf_content_tokens(inner) {
            push_pdf_text_fragments_from_token(&nested, out);
        }
    }
}

fn decode_pdf_literal_string_token(token: &str) -> String {
    let inner = token
        .strip_prefix('(')
        .and_then(|text| text.strip_suffix(')'))
        .unwrap_or(token);
    let mut out = String::new();
    let chars: Vec<char> = inner.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if ch != '\\' {
            out.push(ch);
            index += 1;
            continue;
        }
        index += 1;
        let Some(escaped) = chars.get(index).copied() else {
            break;
        };
        match escaped {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'b' => out.push('\u{0008}'),
            'f' => out.push('\u{000c}'),
            '(' | ')' | '\\' => out.push(escaped),
            '\n' | '\r' => {
                if escaped == '\r' && chars.get(index + 1) == Some(&'\n') {
                    index += 1;
                }
            }
            '0'..='7' => {
                let mut value = escaped.to_digit(8).unwrap_or(0);
                let mut consumed = 1usize;
                while consumed < 3 {
                    let Some(next) = chars.get(index + 1).copied() else {
                        break;
                    };
                    let Some(digit) = next.to_digit(8) else {
                        break;
                    };
                    value = value * 8 + digit;
                    index += 1;
                    consumed += 1;
                }
                out.push(char::from_u32(value).unwrap_or('\u{fffd}'));
            }
            _ => out.push(escaped),
        }
        index += 1;
    }
    out
}

fn decode_pdf_hex_string_token(token: &str) -> String {
    let inner = token
        .strip_prefix('<')
        .and_then(|text| text.strip_suffix('>'))
        .unwrap_or(token);
    let mut hex = inner
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    if hex.len() % 2 == 1 {
        hex.push('0');
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut index = 0usize;
    while index + 1 < hex.len() {
        if let Ok(byte) = u8::from_str_radix(&hex[index..index + 2], 16) {
            bytes.push(byte);
        }
        index += 2;
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        let mut out = String::new();
        for pair in bytes[2..].chunks(2) {
            if pair.len() == 2 {
                let unit = u16::from_be_bytes([pair[0], pair[1]]);
                out.push(char::from_u32(u32::from(unit)).unwrap_or('\u{fffd}'));
            }
        }
        out
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

fn is_useful_pdf_text_fragment(text: &str) -> bool {
    normalized_text_match_key(text).chars().count() >= 2
}

fn dedupe_text_fragments(fragments: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for fragment in fragments {
        let key = normalized_text_match_key(&fragment);
        if !key.is_empty() && seen.insert(key) {
            out.push(fragment);
        }
    }
    out
}

fn normalized_text_match_key(text: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in text.chars() {
        if ch.is_control() {
            continue;
        }
        if ch.is_whitespace() {
            if !out.is_empty() && !last_space {
                out.push(' ');
                last_space = true;
            }
            continue;
        }
        for lower in ch.to_lowercase() {
            out.push(lower);
        }
        last_space = false;
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

fn nice_stream_line_key(
    object: ObjectId,
    chunks: &[StreamChunk],
    line_index: usize,
    line: &NiceStreamLine,
) -> String {
    let first_offset = chunks.first().map(|chunk| chunk.offset).unwrap_or_default();
    format!(
        "{}:{}:{}:{}:{}",
        object.num, object.gen, first_offset, line_index, line.text
    )
}

fn draw_nice_stream_guides(
    ui: &egui::Ui,
    row_rect: egui::Rect,
    guide_blocks: &[(usize, String)],
    selected_block: Option<&str>,
) {
    let top = row_rect.top() - 1.0;
    let bottom = row_rect.bottom() + 1.0;
    for (indent, key) in guide_blocks {
        let selected = selected_block == Some(key.as_str());
        let color = if selected {
            PdbgTheme::ACCENT
        } else {
            PdbgTheme::BORDER
        };
        let stroke = egui::Stroke::new(if selected { 1.5 } else { 1.0 }, color);
        let x = row_rect.left() + (*indent as f32 * NICE_STREAM_INDENT_WIDTH) + 5.0;
        draw_dashed_vertical_line(ui, x, top, bottom, stroke);
    }
}

fn draw_dashed_vertical_line(ui: &egui::Ui, x: f32, top: f32, bottom: f32, stroke: egui::Stroke) {
    let mut y = top;
    while y < bottom {
        let segment_bottom = (y + 3.0).min(bottom);
        ui.painter()
            .line_segment([egui::pos2(x, y), egui::pos2(x, segment_bottom)], stroke);
        y += 6.0;
    }
}

fn real_stream_next_offset(chunks: &[StreamChunk]) -> Option<u64> {
    chunks
        .last()
        .map(|chunk| chunk.offset.saturating_add(chunk.bytes.len() as u64))
        .filter(|offset| *offset > 0)
}

fn real_stream_previous_offset(chunks: &[StreamChunk], limit: usize) -> Option<u64> {
    chunks
        .first()
        .and_then(|chunk| (chunk.offset > 0).then(|| chunk.offset.saturating_sub(limit as u64)))
}

fn real_stream_scroll_request(
    scroll_offset_y: f32,
    viewport_height: f32,
    content_height: f32,
    scroll_delta_y: f32,
    hovered: bool,
    chunks: &[StreamChunk],
    limit: usize,
) -> Option<u64> {
    if !hovered || scroll_delta_y.abs() < 0.5 {
        return None;
    }
    let max_offset = (content_height - viewport_height).max(0.0);
    let near_top = scroll_offset_y <= STREAM_VIEW_AUTO_LOAD_EDGE_PX;
    let near_bottom = scroll_offset_y >= (max_offset - STREAM_VIEW_AUTO_LOAD_EDGE_PX).max(0.0);
    if scroll_delta_y < 0.0 && near_bottom && real_stream_chunks_has_more(chunks) {
        real_stream_next_offset(chunks)
    } else if scroll_delta_y > 0.0 && near_top {
        real_stream_previous_offset(chunks, limit)
    } else {
        None
    }
}

fn real_stream_visible_text(
    chunk: &StreamChunk,
    view_mode: StreamViewMode,
    preset: RealStreamPreset,
) -> String {
    if preset == RealStreamPreset::Nice && view_mode == StreamViewMode::Text {
        pdf_content_stream_nice_text(&chunk.bytes)
    } else {
        stream_chunk_display_text(chunk, view_mode)
    }
}

fn pdf_content_stream_nice_text(bytes: &[u8]) -> String {
    nice_stream_lines_to_text(&pdf_content_stream_nice_lines(bytes))
}

fn pdf_content_stream_nice_lines(bytes: &[u8]) -> Vec<NiceStreamLine> {
    let raw = String::from_utf8_lossy(bytes);
    let tokens = pdf_content_tokens(&raw);
    if tokens.is_empty() {
        return vec![NiceStreamLine {
            indent: 0,
            text: "<empty content stream>".to_string(),
            block_open: false,
            block_close: false,
        }];
    }

    let mut lines = Vec::new();
    let mut operands = Vec::new();
    let mut indent = 0usize;
    for token in tokens {
        if is_pdf_content_operator(&token, &operands) {
            let block_close = is_pdf_content_block_close(&token);
            if matches!(token.as_str(), "Q" | "ET" | "EMC" | "EX") {
                indent = indent.saturating_sub(1);
            }
            lines.push(NiceStreamLine {
                indent,
                text: pdf_content_instruction_line(&token, &operands),
                block_open: is_pdf_content_block_open(&token),
                block_close,
            });
            if is_pdf_content_block_open(&token) {
                indent = (indent + 1).min(64);
            }
            operands.clear();
        } else {
            operands.push(token);
        }
    }

    if !operands.is_empty() {
        lines.push(NiceStreamLine {
            indent,
            text: compact_pdf_operands(&operands),
            block_open: false,
            block_close: false,
        });
    }
    lines
}

fn nice_stream_lines_to_text(lines: &[NiceStreamLine]) -> String {
    let mut out = String::new();
    for line in lines {
        push_pdf_content_line(&mut out, line.indent, &line.text);
    }
    out
}

fn push_pdf_content_line(out: &mut String, indent: usize, line: &str) {
    for _ in 0..indent {
        out.push_str("  ");
    }
    out.push_str(line);
    out.push('\n');
}

fn is_pdf_content_block_open(operator: &str) -> bool {
    matches!(operator, "q" | "BT" | "BDC" | "BX")
}

fn is_pdf_content_block_close(operator: &str) -> bool {
    matches!(operator, "Q" | "ET" | "EMC" | "EX")
}

fn pdf_content_tokens(input: &str) -> Vec<String> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }
        if ch == '%' {
            index += 1;
            while index < chars.len() && chars[index] != '\n' && chars[index] != '\r' {
                index += 1;
            }
            continue;
        }

        let (token, next) = if ch == '(' {
            parse_pdf_string_token(&chars, index)
        } else if ch == '[' {
            parse_balanced_pdf_token(&chars, index, '[', ']')
        } else if ch == '<' && chars.get(index + 1) != Some(&'<') {
            parse_balanced_pdf_token(&chars, index, '<', '>')
        } else if ch == '<' && chars.get(index + 1) == Some(&'<') {
            ("<<".to_string(), index + 2)
        } else if ch == '>' && chars.get(index + 1) == Some(&'>') {
            (">>".to_string(), index + 2)
        } else {
            parse_pdf_atom_token(&chars, index)
        };
        if !token.is_empty() {
            tokens.push(token);
        }
        index = next.max(index + 1);
    }
    tokens
}

fn parse_pdf_string_token(chars: &[char], start: usize) -> (String, usize) {
    let mut depth = 0usize;
    let mut escaped = false;
    let mut index = start;
    let mut out = String::new();
    while index < chars.len() {
        let ch = chars[index];
        out.push(ch);
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return (out, index + 1);
            }
        }
        index += 1;
    }
    (out, index)
}

fn parse_balanced_pdf_token(
    chars: &[char],
    start: usize,
    open: char,
    close: char,
) -> (String, usize) {
    let mut depth = 0usize;
    let mut index = start;
    let mut out = String::new();
    while index < chars.len() {
        let ch = chars[index];
        out.push(ch);
        if ch == '(' {
            let (string, next) = parse_pdf_string_token(chars, index);
            out.pop();
            out.push_str(&string);
            index = next;
            continue;
        }
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return (out, index + 1);
            }
        }
        index += 1;
    }
    (out, index)
}

fn parse_pdf_atom_token(chars: &[char], start: usize) -> (String, usize) {
    let mut index = start;
    let mut out = String::new();
    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() || matches!(ch, '[' | ']' | '(' | ')' | '<' | '>' | '%') {
            break;
        }
        out.push(ch);
        index += 1;
    }
    (out, index)
}

fn is_pdf_content_operator(operator: &str, operands: &[String]) -> bool {
    matches!(
        operator,
        "Tf" | "Do"
            | "gs"
            | "sh"
            | "CS"
            | "cs"
            | "BT"
            | "ET"
            | "Tj"
            | "TJ"
            | "'"
            | "\""
            | "Td"
            | "TD"
            | "Tm"
            | "T*"
            | "Tc"
            | "TL"
            | "Ts"
            | "Tw"
            | "Tz"
            | "q"
            | "Q"
            | "cm"
            | "m"
            | "l"
            | "c"
            | "v"
            | "y"
            | "h"
            | "re"
            | "S"
            | "s"
            | "f"
            | "F"
            | "f*"
            | "B"
            | "B*"
            | "b"
            | "b*"
            | "n"
            | "W"
            | "W*"
            | "rg"
            | "RG"
            | "g"
            | "G"
            | "k"
            | "K"
            | "sc"
            | "SC"
            | "scn"
            | "SCN"
            | "MP"
            | "DP"
            | "BDC"
            | "EMC"
            | "BX"
            | "EX"
            | "w"
            | "J"
            | "j"
            | "M"
            | "d"
            | "ri"
            | "BI"
            | "ID"
            | "EI"
    ) || (operator.len() <= 3 && !operands.is_empty() && operator.chars().all(char::is_alphabetic))
}

fn pdf_content_instruction_line(operator: &str, operands: &[String]) -> String {
    if operands.is_empty() {
        operator.to_string()
    } else {
        format!("{} {}", compact_pdf_operands(operands), operator)
    }
}

fn compact_pdf_operands(operands: &[String]) -> String {
    if operands.is_empty() {
        return "-".to_string();
    }
    operands
        .iter()
        .map(|operand| compact_pdf_token(operand))
        .collect::<Vec<_>>()
        .join(" ")
}

fn compact_pdf_token(token: &str) -> String {
    let mut out = String::new();
    for ch in token.chars() {
        if ch.is_control() {
            out.push(' ');
        } else {
            out.push(ch);
        }
        if out.len() >= 96 {
            out.push_str("...");
            break;
        }
    }
    out
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

fn page_index_for_node(id: &NodeId) -> Option<usize> {
    match id {
        NodeId::Page { index, .. } => Some(*index),
        NodeId::ResourceGroup { page_index, .. } => Some(*page_index),
        NodeId::ArrayEntry { parent, index, .. } if is_page_root_node(parent) => Some(*index),
        NodeId::DictEntry { parent, .. } | NodeId::ArrayEntry { parent, .. } => {
            page_index_for_node(parent)
        }
        _ => None,
    }
}

fn is_page_row_node_for_index(id: &NodeId, page_index: usize) -> bool {
    match id {
        NodeId::Page { index, .. } => *index == page_index,
        NodeId::ArrayEntry { parent, index, .. } if is_page_root_node(parent) => {
            *index == page_index
        }
        _ => false,
    }
}

fn is_page_content_stream_node(id: &NodeId) -> bool {
    match id {
        NodeId::DictEntry { parent, key, .. } if key == "Contents" => {
            page_index_for_node(parent).is_some()
        }
        NodeId::ArrayEntry { parent, .. } => is_page_content_stream_node(parent),
        _ => false,
    }
}

fn summary_has_stream(summary: &ObjectSummary) -> bool {
    summary.has_stream || matches!(summary.kind, ObjectKind::Stream)
}

fn is_page_root_node(id: &NodeId) -> bool {
    matches!(id, NodeId::PageRoot { .. })
        || matches!(id, NodeId::DictEntry { key, .. } if key == "Pages")
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
        self.handle_page_keyboard_shortcuts(&ctx);
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

        self.draw_workspace(ui, &ctx);

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
    fn draw_workspace(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let available_rect = ui.available_rect_before_wrap();
        if available_rect.width() <= 0.0 || available_rect.height() <= 0.0 {
            return;
        }

        let layout = workspace_panel_layout(available_rect.width());
        let mut left_width = self.left_panel_width.unwrap_or(layout.left.default);
        let mut right_width = self.right_panel_width.unwrap_or(layout.right.default);
        clamp_workspace_widths(
            &mut left_width,
            &mut right_width,
            layout,
            available_rect.width(),
        );

        let rects = workspace_rects(available_rect, left_width, right_width);
        let left_splitter_response = ui.interact(
            rects.left_splitter,
            egui::Id::new("workspace_left_splitter"),
            egui::Sense::drag(),
        );
        let right_splitter_response = ui.interact(
            rects.right_splitter,
            egui::Id::new("workspace_right_splitter"),
            egui::Sense::drag(),
        );
        let pointer_delta_x = ui.input(|input| input.pointer.delta().x);
        if left_splitter_response.dragged() {
            left_width += pointer_delta_x;
        }
        if right_splitter_response.dragged() {
            right_width -= pointer_delta_x;
        }
        clamp_workspace_widths(
            &mut left_width,
            &mut right_width,
            layout,
            available_rect.width(),
        );
        self.left_panel_width = Some(left_width);
        self.right_panel_width = Some(right_width);

        let rects = workspace_rects(available_rect, left_width, right_width);
        show_framed_child(ui, rects.left, panel_frame(), |ui| self.draw_tree(ui));
        show_framed_child(
            ui,
            rects.center,
            egui::Frame::new()
                .fill(PdbgTheme::CANVAS)
                .inner_margin(egui::Margin::symmetric(10, 10)),
            |ui| self.draw_page_preview(ui),
        );
        show_framed_child(ui, rects.right, panel_frame(), |ui| {
            self.draw_inspector(ui, ctx)
        });
        draw_workspace_splitter(ui, rects.left_splitter, &left_splitter_response);
        draw_workspace_splitter(ui, rects.right_splitter, &right_splitter_response);
        ui.advance_cursor_after_rect(available_rect);
    }

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
        self.real_stream_preset = RealStreamPreset::Nice;
        self.real_stream_mode = StreamMode::Decoded;
        self.real_stream_view_mode = StreamViewMode::Text;
        self.real_stream_offset = 0;
        self.real_stream_limit = REAL_STREAM_DEFAULT_VIEW_LIMIT_BYTES;
        self.real_stream_key = None;
        self.real_stream_chunk = None;
        self.real_stream_windows.clear();
        self.real_stream_error = None;
        self.decoded_stream_cache = StreamChunkCache::new(
            DECODED_STREAM_CACHE_MAX_ITEMS,
            DECODED_STREAM_CACHE_MAX_BYTES,
        );
        self.selected_row = self.tree.initial_selected_row();
        self.scroll_selected_tree_row = false;
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
        self.visual_page_cache = VisualPageCache::new(
            VISUAL_CLICK_CACHE_MAX_PAGES,
            VISUAL_CLICK_CACHE_MAX_ELEMENTS,
        );
        self.selected_text_hit = None;
        self.selected_visual_hit = None;
        self.pending_preview_stream_selection = None;
        self.preview_click = None;
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

        if self.tree.is_real() {
            self.draw_real_tree_rows(ui);
            return;
        }

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

    fn draw_real_tree_rows(&mut self, ui: &mut egui::Ui) {
        let mut clicked_row = None;
        let mut toggled_row = None;
        let scroll_selected = self.scroll_selected_tree_row;
        let mut scrolled_selected = false;
        ScrollArea::vertical()
            .id_salt("real_document_tree_rows")
            .show(ui, |ui| {
                for row in 0..self.tree.row_count() {
                    let selected = row == self.selected_row;
                    let depth = self.tree.real_row_depth(row).unwrap_or(0);
                    let marker = self.tree.real_row_tree_marker(row).unwrap_or(" ");
                    let job = self.tree.row_layout_job(row, selected);
                    let row_label = self.tree.row_label(row);

                    ui.horizontal(|ui| {
                        ui.add_space((depth as f32) * 18.0);
                        let marker_response = ui
                            .add(
                                egui::Label::new(dense_monospace_text(marker).color(if selected {
                                    PdbgTheme::ACCENT
                                } else {
                                    PdbgTheme::MUTED
                                }))
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_text("Expand or collapse");
                        if marker_response.clicked() && marker != " " {
                            toggled_row = Some(row);
                        }

                        let row_width = ui.available_width().max(24.0);
                        let row_response = ui
                            .add_sized(
                                egui::vec2(row_width, ui.text_style_height(&TextStyle::Body) + 4.0),
                                egui::Button::selectable(selected, ())
                                    .left_text(job)
                                    .truncate(),
                            )
                            .on_hover_text(row_label);
                        if selected && scroll_selected {
                            row_response.scroll_to_me(Some(egui::Align::Center));
                            scrolled_selected = true;
                        }
                        if row_response.double_clicked() {
                            toggled_row = Some(row);
                        } else if row_response.clicked() {
                            clicked_row = Some(row);
                        }
                    });
                }
            });
        if let Some(row) = toggled_row {
            self.select_row_from_tree(row);
        } else if let Some(row) = clicked_row {
            self.select_row_from_tree(row);
        }
        if scrolled_selected {
            self.scroll_selected_tree_row = false;
        }
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
            let controls = draw_search_controls(
                ui,
                &mut self.object_search_query,
                "object, key, name, scalar",
                self.object_search_job.is_some(),
            );
            run_search |= controls.submit;
            cancel_search |= controls.cancel;
            clear_search |= controls.clear;
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
                                .selectable_label(false, dense_monospace_text(label))
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
            let controls = draw_search_controls(
                ui,
                &mut self.text_search_query,
                "page text",
                self.text_search_job.is_some(),
            );
            run_search |= controls.submit;
            cancel_search |= controls.cancel;
            clear_search |= controls.clear;
        });

        if clear_search {
            self.cancel_text_search_job();
            self.text_search_query.clear();
            self.text_search_result = None;
            self.text_search_error = None;
            self.selected_text_hit = None;
            self.selected_visual_hit = None;
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
                                    dense_monospace_text(text_search_hit_summary(hit)),
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
        if self.real_render_job.is_some() {
            let available = ui.available_size();
            let desired = egui::vec2(
                available.x.max(PAGE_PREVIEW_MIN_WIDTH),
                available.y.max(PAGE_PREVIEW_MIN_HEIGHT),
            );
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
        let texture_id = texture.id();
        let texture_size = texture.size_vec2();
        let render_page_index = render.page_index;
        let render_width = render.width;
        let render_height = render.height;
        let render_zoom = self.render_zoom;
        let render_rotation = self.render_rotation_degrees;
        let preview_click = self.preview_click;
        let selected_hit = self
            .selected_text_hit
            .as_ref()
            .filter(|hit| hit.page_index == render_page_index)
            .cloned();
        let selected_visual_hit = self
            .selected_visual_hit
            .as_ref()
            .filter(|hit| hit.page_index == render_page_index)
            .cloned();

        let display_size = page_preview_display_size(
            texture_size,
            available,
            PAGE_PREVIEW_FOOTER_RESERVED_HEIGHT,
            render_zoom,
        );
        let image_area_height = (available.y - PAGE_PREVIEW_FOOTER_RESERVED_HEIGHT).max(1.0);
        ScrollArea::both()
            .id_salt("real_page_preview_scroll")
            .max_height(image_area_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(page_preview_leading_space(
                        ui.available_width(),
                        display_size.x,
                    ));
                    let response = ui.add(
                        egui::Image::new((texture_id, display_size))
                            .bg_fill(PdbgTheme::PAGE)
                            .corner_radius(3)
                            .sense(egui::Sense::click()),
                    );
                    let image_rect = response.rect;
                    if response.clicked() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            let click = preview_click_from_pos(
                                pos,
                                image_rect,
                                render_width,
                                render_height,
                                render_page_index,
                            );
                            self.preview_click = click;
                            let mut highlighted_stream_code = false;
                            if let Some(click) = click {
                                self.select_text_hit_for_preview_click(click);
                                self.select_visual_hit_for_preview_click(click);
                                highlighted_stream_code =
                                    self.open_nice_stream_for_preview_selection(click.page_index);
                            }
                            self.selected_tab = if highlighted_stream_code {
                                InspectorTab::Stream
                            } else {
                                InspectorTab::Object
                            };
                        }
                    }
                    let painter = ui.painter_at(image_rect);
                    if let Some(hit) = &selected_visual_hit {
                        if let Some(rect) = visual_hit_bbox_image_rect(
                            hit,
                            image_rect,
                            render_width,
                            render_height,
                            render_zoom,
                            render_rotation,
                        ) {
                            painter.rect_stroke(
                                rect,
                                0.0,
                                egui::Stroke::new(2.0, PdbgTheme::ACCENT),
                                egui::StrokeKind::Outside,
                            );
                        }
                    }
                    if let Some(hit) = &selected_hit {
                        if let Some(rect) = text_hit_bbox_image_rect(
                            hit,
                            image_rect,
                            render_width,
                            render_height,
                            render_zoom,
                            render_rotation,
                        ) {
                            painter.rect_stroke(
                                rect,
                                0.0,
                                egui::Stroke::new(2.0, PdbgTheme::OPERATOR),
                                egui::StrokeKind::Outside,
                            );
                        }
                    }
                    if let Some(click) =
                        preview_click.filter(|click| click.page_index == render_page_index)
                    {
                        let marker = egui::pos2(
                            image_rect.left() + click.normalized_x * image_rect.width(),
                            image_rect.top() + click.normalized_y * image_rect.height(),
                        );
                        painter.circle_stroke(
                            marker,
                            5.0,
                            egui::Stroke::new(1.5, PdbgTheme::ACCENT),
                        );
                        painter.line_segment(
                            [
                                marker + egui::vec2(-8.0, 0.0),
                                marker + egui::vec2(8.0, 0.0),
                            ],
                            egui::Stroke::new(1.0, PdbgTheme::ACCENT),
                        );
                        painter.line_segment(
                            [
                                marker + egui::vec2(0.0, -8.0),
                                marker + egui::vec2(0.0, 8.0),
                            ],
                            egui::Stroke::new(1.0, PdbgTheme::ACCENT),
                        );
                    }
                });
            });
        ui.add_space(8.0);
        if let Some(hit) = &selected_hit {
            ui.add_space(4.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(text_search_hit_summary(hit))
                        .monospace()
                        .color(PdbgTheme::ACCENT),
                )
                .on_hover_text(text_search_hit_hover(hit));
            });
        }
        true
    }

    fn draw_real_render_controls(&mut self, ui: &mut egui::Ui) {
        let page_count = self.page_count();
        if page_count == 0 {
            return;
        }

        let mut rerender = false;
        let total_width =
            PREVIEW_ZOOM_CONTROL_WIDTH + PREVIEW_CONTROL_GAP + PREVIEW_PAGER_CONTROL_WIDTH;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), PREVIEW_CONTROL_ROW_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                let leading = ((ui.available_width() - total_width) * 0.5).max(0.0);
                ui.add_space(leading);
                preview_control_group(ui, PREVIEW_ZOOM_CONTROL_WIDTH, |ui| {
                    let previous_zoom = previous_render_zoom(self.render_zoom);
                    if preview_icon_button(
                        ui,
                        PreviewControlIcon::ZoomOut,
                        previous_zoom.is_some(),
                        "Zoom out",
                    )
                    .clicked()
                    {
                        if let Some(zoom) = previous_zoom {
                            self.render_zoom = zoom;
                            rerender = true;
                        }
                    }
                    ui.add_sized(
                        [64.0, 30.0],
                        egui::Label::new(
                            RichText::new(format!("{:.0}%", self.render_zoom * 100.0))
                                .size(18.0)
                                .color(PdbgTheme::TEXT),
                        ),
                    );
                    let next_zoom = next_render_zoom(self.render_zoom);
                    if preview_icon_button(
                        ui,
                        PreviewControlIcon::ZoomIn,
                        next_zoom.is_some(),
                        "Zoom in",
                    )
                    .clicked()
                    {
                        if let Some(zoom) = next_zoom {
                            self.render_zoom = zoom;
                            rerender = true;
                        }
                    }
                    preview_control_separator(ui);
                    if preview_icon_button(
                        ui,
                        PreviewControlIcon::RotateRight,
                        true,
                        format!(
                            "Rotate to {} deg",
                            next_render_rotation(self.render_rotation_degrees)
                        ),
                    )
                    .clicked()
                    {
                        self.render_rotation_degrees =
                            next_render_rotation(self.render_rotation_degrees);
                        rerender = true;
                    }
                });
                ui.add_space(PREVIEW_CONTROL_GAP);
                self.draw_real_preview_pager(ui);
            },
        );
        if rerender {
            self.refresh_real_render();
        }
        ui.add_space(6.0);
    }

    fn draw_real_preview_pager(&mut self, ui: &mut egui::Ui) {
        let page_count = self.page_count();
        if page_count == 0 {
            return;
        }
        preview_control_group(ui, PREVIEW_PAGER_CONTROL_WIDTH, |ui| {
            if preview_icon_button(
                ui,
                PreviewControlIcon::PreviousPage,
                self.render_page_index > 0,
                "Previous page",
            )
            .clicked()
            {
                self.set_render_page_from_pager(self.render_page_index - 1);
            }
            ui.add_sized(
                [64.0, 30.0],
                egui::Label::new(
                    RichText::new(format!(
                        "{}/{}",
                        self.render_page_index + 1,
                        page_count.max(1)
                    ))
                    .size(18.0)
                    .color(PdbgTheme::TEXT),
                ),
            );
            if preview_icon_button(
                ui,
                PreviewControlIcon::NextPage,
                self.render_page_index + 1 < page_count,
                "Next page",
            )
            .clicked()
            {
                self.set_render_page_from_pager(self.render_page_index + 1);
            }
        });
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
                    truncated_monospace(ui, preview);
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
                            dense_label(ui, "hash");
                            truncated_monospace(ui, option_text(summary.file_hash.as_deref()));
                            ui.end_row();
                            dense_label(ui, "version");
                            truncated_monospace(ui, option_text(summary.pdf_version.as_deref()));
                            ui.end_row();
                            dense_label(ui, "permissions");
                            truncated_monospace(
                                ui,
                                format!(
                                    "print={} copy={} modify={}",
                                    summary.permissions.print,
                                    summary.permissions.copy,
                                    summary.permissions.modify
                                ),
                            );
                            ui.end_row();
                        });
                });
            }
        } else if let Err(err) = &self.state {
            ui.colored_label(PdbgTheme::ERROR_FG, err);
        }
    }

    fn draw_real_object_panel(&mut self, ui: &mut egui::Ui) {
        if self.draw_preview_click_panel(ui) {
            return;
        }

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
                let label_width = if detail.object.is_some() {
                    (ui.available_width() - 88.0).max(24.0)
                } else {
                    ui.available_width().max(24.0)
                };
                truncated_label(
                    ui,
                    RichText::new(&detail.label)
                        .monospace()
                        .strong()
                        .color(PdbgTheme::TEXT),
                    label_width,
                    Some(&detail.label),
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
                    dense_label(ui, "kind");
                    truncated_monospace(ui, object_kind_label(&detail.kind));
                    ui.end_row();
                    dense_label(ui, "value");
                    truncated_monospace(ui, object_value_preview(&detail.value, &detail.preview));
                    ui.end_row();
                    dense_label(ui, "path");
                    truncated_monospace(ui, node_breadcrumb(&detail.id));
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
                            ui.label(dense_monospace_text(format!("/{}", entry.key)));
                            type_badge(ui, &entry.value.kind);
                            truncated_monospace(ui, summary_inline_text(&entry.value));
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
                            ui.label(dense_monospace_text(format!("[{index}]")));
                            type_badge(ui, &entry.kind);
                            truncated_monospace(ui, summary_inline_text(entry));
                            ui.end_row();
                        }
                    });
            });
        }
    }

    fn draw_preview_click_panel(&self, ui: &mut egui::Ui) -> bool {
        let Some(click) = self.preview_click else {
            return false;
        };

        section_frame().show(ui, |ui| {
            ui.label(RichText::new("Preview hit").small().color(PdbgTheme::MUTED));
            ui.add_space(3.0);
            egui::Grid::new("preview_click_grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    dense_label(ui, "page");
                    truncated_monospace(ui, format!("{}", click.page_index + 1));
                    ui.end_row();

                    dense_label(ui, "render px");
                    truncated_monospace(
                        ui,
                        format!("{:.1}, {:.1}", click.render_x, click.render_y),
                    );
                    ui.end_row();

                    dense_label(ui, "normalized");
                    truncated_monospace(
                        ui,
                        format!("{:.4}, {:.4}", click.normalized_x, click.normalized_y),
                    );
                    ui.end_row();

                    let text_bbox = self
                        .selected_text_hit
                        .as_ref()
                        .filter(|hit| hit.page_index == click.page_index)
                        .and_then(|hit| hit.bbox.as_ref())
                        .map(|bbox| {
                            format!(
                                "x={:.1} y={:.1} w={:.1} h={:.1}",
                                bbox.x, bbox.y, bbox.width, bbox.height
                            )
                        });
                    if let Some(text_bbox) = text_bbox {
                        dense_label(ui, "text bbox");
                        truncated_monospace(ui, text_bbox);
                        ui.end_row();
                    }

                    let visual_hit = self
                        .selected_visual_hit
                        .as_ref()
                        .filter(|hit| hit.page_index == click.page_index);

                    if let Some(hit) = visual_hit {
                        dense_label(ui, "visual kind");
                        truncated_monospace(
                            ui,
                            format!(
                                "{} #{}{}{}",
                                hit.kind.as_public_str(),
                                hit.element_index,
                                if hit.contains_click {
                                    " contains"
                                } else {
                                    " near"
                                },
                                if hit.untrusted { " untrusted" } else { "" }
                            ),
                        );
                        ui.end_row();

                        dense_label(ui, "visual bbox");
                        truncated_monospace(
                            ui,
                            format!(
                                "x={:.1} y={:.1} w={:.1} h={:.1}",
                                hit.bbox.x, hit.bbox.y, hit.bbox.width, hit.bbox.height
                            ),
                        );
                        ui.end_row();

                        if let Some(object) = visual_object_attribution_text(Some(hit)) {
                            dense_label(ui, "object attribution");
                            truncated_monospace(ui, object);
                            ui.end_row();
                        }
                    }
                });
        });
        ui.add_space(8.0);
        true
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
                    dense_label(ui, "Offset");
                    let offset_response = ui.add(
                        egui::DragValue::new(&mut self.stream.offset)
                            .range(0..=STREAM_TOTAL_BYTES.saturating_sub(HEX_WINDOW_BYTES))
                            .speed(64),
                    );
                    if offset_response.changed() {
                        self.stream.sync_hex_window();
                    }
                    ui.end_row();

                    dense_label(ui, "Fallback range");
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

        if self.real_stream_key.is_none()
            && self.real_stream_chunk.is_none()
            && self.real_stream_job.is_none()
        {
            self.apply_real_stream_preset(&stream);
        }

        ui.add_space(8.0);
        let mut request_changed = false;
        let mut force_reload = false;
        section_frame().show(ui, |ui| {
            section_header(ui, "Stream View", Some("nice operations / raw bytes"));
            ui.horizontal(|ui| {
                ui.label(RichText::new("View").small().color(PdbgTheme::MUTED));
                if ui
                    .selectable_value(
                        &mut self.real_stream_preset,
                        RealStreamPreset::Nice,
                        "Nice View",
                    )
                    .changed()
                {
                    request_changed |= self.apply_real_stream_preset(&stream);
                }
                if ui
                    .selectable_value(
                        &mut self.real_stream_preset,
                        RealStreamPreset::Raw,
                        "Raw View",
                    )
                    .changed()
                {
                    request_changed |= self.apply_real_stream_preset(&stream);
                }
                if ui.button("Reload").clicked() {
                    force_reload = true;
                }
            });
            ui.collapsing("Advanced", |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Mode").small().color(PdbgTheme::MUTED));
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
                    ui.label(RichText::new("Format").small().color(PdbgTheme::MUTED));
                    request_changed |= ui
                        .selectable_value(
                            &mut self.real_stream_view_mode,
                            StreamViewMode::Hex,
                            "Hex",
                        )
                        .changed();
                    request_changed |= ui
                        .selectable_value(
                            &mut self.real_stream_view_mode,
                            StreamViewMode::Text,
                            "Text",
                        )
                        .changed();
                    request_changed |= ui
                        .selectable_value(
                            &mut self.real_stream_view_mode,
                            StreamViewMode::Bytes,
                            "Bytes",
                        )
                        .changed();
                });
                ui.add_space(4.0);
                egui::Grid::new("real_stream_controls_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        dense_label(ui, "Offset");
                        request_changed |= ui
                            .add(egui::DragValue::new(&mut self.real_stream_offset).speed(64))
                            .changed();
                        ui.end_row();

                        dense_label(ui, "Limit");
                        request_changed |= ui
                            .add(
                                egui::DragValue::new(&mut self.real_stream_limit)
                                    .range(1..=REAL_STREAM_MAX_VIEW_LIMIT_BYTES)
                                    .speed(64),
                            )
                            .changed();
                        ui.end_row();
                    });
            });
            if request_changed {
                self.clear_real_stream_chunk();
            }
            if force_reload {
                self.clear_real_stream_chunk();
                self.refresh_real_stream_chunk(stream.object);
            }
            if self.real_stream_job.is_some() {
                if ui.button("Cancel load").clicked() {
                    self.cancel_real_stream_job();
                }
            } else if self.real_stream_key.is_none() {
                self.refresh_real_stream_chunk(stream.object);
            }
        });

        if self.real_stream_job.is_some() && self.real_stream_chunk.is_none() {
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
        let loaded_chunks = if self.real_stream_windows.is_empty() {
            vec![chunk.clone()]
        } else {
            self.real_stream_windows
                .iter()
                .map(|window| window.chunk.clone())
                .collect::<Vec<_>>()
        };
        ui.add_space(8.0);
        section_frame().show(ui, |ui| {
            section_header(
                ui,
                real_stream_preset_label(self.real_stream_preset),
                Some(&format!(
                    "{} bytes / {}",
                    loaded_chunks
                        .iter()
                        .map(|chunk| chunk.bytes.len())
                        .sum::<usize>(),
                    real_stream_chunks_range_label(&loaded_chunks)
                )),
            );
            let visible_text = real_stream_chunks_visible_text(
                &loaded_chunks,
                self.real_stream_view_mode,
                self.real_stream_preset,
            );
            let view_height = (ui.available_height() - 32.0).max(STREAM_VIEW_MIN_HEIGHT);
            let scroll_output = ScrollArea::both()
                .id_salt((
                    "real_stream_visible_chunk",
                    loaded_chunks
                        .first()
                        .map(|chunk| chunk.offset)
                        .unwrap_or_default(),
                    stream_mode_label(chunk.mode),
                    stream_view_mode_label(self.real_stream_view_mode),
                ))
                .min_scrolled_height(view_height)
                .max_height(view_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.real_stream_preset == RealStreamPreset::Nice
                        && self.real_stream_view_mode == StreamViewMode::Text
                    {
                        self.draw_nice_stream_lines(ui, stream.object, &loaded_chunks);
                    } else {
                        ui.add(
                            egui::Label::new(
                                RichText::new(visible_text.as_str())
                                    .font(mono_font_id(STREAM_VIEW_FONT_SIZE))
                                    .color(PdbgTheme::TEXT),
                            )
                            .extend()
                            .selectable(true),
                        );
                    }
                });
            let auto_request_offset = {
                let hovered = ui.input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pos| scroll_output.inner_rect.contains(pos))
                });
                let scroll_delta_y = ui.input(|input| input.smooth_scroll_delta.y);
                real_stream_scroll_request(
                    scroll_output.state.offset.y,
                    scroll_output.inner_rect.height(),
                    scroll_output.content_size.y,
                    scroll_delta_y,
                    hovered,
                    &loaded_chunks,
                    self.real_stream_limit,
                )
            };
            if let Some(offset) = auto_request_offset {
                self.real_stream_offset = offset;
                self.refresh_real_stream_chunk(stream.object);
            }
            ui.horizontal(|ui| {
                if ui.button("Copy visible text").clicked() {
                    let escaped =
                        escape_pdf_text(&visible_text, EgressFormat::Markdown, COPY_LIMIT_BYTES);
                    ctx.copy_text(escaped.text.clone());
                    self.status_log.push(format!(
                        "copied visible {} {} stream text{}",
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
                if self.real_stream_job.is_some() {
                    ui.label(
                        RichText::new("loading more...")
                            .small()
                            .color(PdbgTheme::MUTED),
                    );
                } else if real_stream_chunks_has_more(&loaded_chunks) {
                    ui.label(
                        RichText::new("more available; scroll down to load")
                            .small()
                            .color(PdbgTheme::MUTED),
                    );
                }
            });
        });

        if !chunk.decode_diagnostics.is_empty() {
            ui.add_space(8.0);
            for diagnostic in &chunk.decode_diagnostics {
                draw_diagnostic_card(ui, diagnostic);
            }
        }
    }

    fn draw_nice_stream_lines(
        &mut self,
        ui: &mut egui::Ui,
        object: ObjectId,
        chunks: &[StreamChunk],
    ) {
        let rows = real_stream_nice_render_lines(object, chunks);
        let mut hidden_block_indent = None;
        ui.spacing_mut().item_spacing.y = 1.0;
        for row in &rows {
            let line = &row.line;
            if let Some(indent) = hidden_block_indent {
                if line.indent > indent {
                    continue;
                }
                if line.indent == indent && line.block_close {
                    hidden_block_indent = None;
                    continue;
                }
                hidden_block_indent = None;
            }

            let mut collapsed =
                line.block_open && self.real_stream_collapsed_blocks.contains(&row.line_key);
            let selection_key = row.block_key.as_deref().unwrap_or(row.line_key.as_str());
            let selected_key = self.real_stream_selected_block.as_deref();
            let selected =
                selected_key == Some(selection_key) || selected_key == Some(row.line_key.as_str());
            let row_response = ui.horizontal(|ui| {
                ui.add_space(line.indent as f32 * NICE_STREAM_INDENT_WIDTH);
                if line.block_open {
                    let marker = if collapsed { "+" } else { "-" };
                    if ui
                        .small_button(marker)
                        .on_hover_text("Collapse or expand this content block")
                        .clicked()
                    {
                        collapsed = !collapsed;
                        if collapsed {
                            self.real_stream_collapsed_blocks
                                .insert(row.line_key.clone());
                        } else {
                            self.real_stream_collapsed_blocks.remove(&row.line_key);
                        }
                        self.real_stream_selected_block = Some(row.line_key.clone());
                        self.select_nice_stream_selection_for_preview(object, &row.line_key, &rows);
                    }
                } else {
                    ui.add_space(22.0);
                }

                let color = if line.block_open {
                    PdbgTheme::ACCENT
                } else if line.block_close {
                    PdbgTheme::MUTED
                } else {
                    PdbgTheme::TEXT
                };
                let response = ui.selectable_label(
                    selected,
                    RichText::new(line.text.as_str())
                        .font(mono_font_id(STREAM_VIEW_FONT_SIZE))
                        .color(color),
                );
                if response.clicked() {
                    self.real_stream_selected_block = Some(selection_key.to_string());
                    self.select_nice_stream_selection_for_preview(object, selection_key, &rows);
                    if line.block_open {
                        collapsed = !collapsed;
                        if collapsed {
                            self.real_stream_collapsed_blocks
                                .insert(row.line_key.clone());
                        } else {
                            self.real_stream_collapsed_blocks.remove(&row.line_key);
                        }
                    }
                }
            });
            if selected && self.scroll_selected_nice_stream_row {
                row_response
                    .response
                    .scroll_to_me(Some(egui::Align::Center));
                self.scroll_selected_nice_stream_row = false;
            }
            draw_nice_stream_guides(
                ui,
                row_response.response.rect,
                &row.guide_blocks,
                self.real_stream_selected_block.as_deref(),
            );
            if line.block_open && collapsed {
                hidden_block_indent = Some(line.indent);
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RealStreamPreset {
    Nice,
    Raw,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RealStreamKey {
    object: ObjectId,
    mode: StreamMode,
    offset: u64,
    limit: usize,
}

#[derive(Clone, Debug)]
struct RealStreamLoadedWindow {
    key: RealStreamKey,
    chunk: StreamChunk,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NiceStreamLine {
    indent: usize,
    text: String,
    block_open: bool,
    block_close: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NiceStreamRenderLine {
    line: NiceStreamLine,
    line_key: String,
    block_key: Option<String>,
    guide_blocks: Vec<(usize, String)>,
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

    fn initial_selected_row(&self) -> usize {
        match self {
            Self::Virtual(_) => 0,
            Self::Real(tree) => tree.initial_selected_row(),
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

    fn real_row_depth(&self, row: usize) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.row_depth(row),
            Self::Virtual(_) => None,
        }
    }

    fn real_row_tree_marker(&self, row: usize) -> Option<&'static str> {
        match self {
            Self::Real(tree) => tree.row_tree_marker(row),
            Self::Virtual(_) => None,
        }
    }

    fn real_row_page_index(&self, row: usize) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.row_page_index(row),
            Self::Virtual(_) => None,
        }
    }

    fn real_row_for_page_index(&self, page_index: usize) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.row_for_page_index(page_index),
            Self::Virtual(_) => None,
        }
    }

    fn real_page_root_row(&self) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.page_root_row(),
            Self::Virtual(_) => None,
        }
    }

    fn real_first_page_content_stream_row(&self, page_row: usize) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.first_page_content_stream_row(page_row),
            Self::Virtual(_) => None,
        }
    }

    fn real_page_content_candidate_rows(&self, page_row: usize) -> Vec<usize> {
        match self {
            Self::Real(tree) => tree.page_content_candidate_rows(page_row),
            Self::Virtual(_) => Vec::new(),
        }
    }

    fn ensure_real_page_child_row(
        &mut self,
        page_root_row: usize,
        summary: ObjectSummary,
    ) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.ensure_page_child_row(page_root_row, summary),
            Self::Virtual(_) => None,
        }
    }

    fn real_row_visual_target(&self, row: usize) -> Option<RealVisualTarget> {
        match self {
            Self::Real(tree) => tree.row_visual_target(row),
            Self::Virtual(_) => None,
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
    root_children: Vec<ObjectSummary>,
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
        let mut rows = Vec::new();
        if let Some(first) = page.items.first() {
            rows.push(RealTreeRow {
                summary: ObjectSummary {
                    id: NodeId::DocumentRoot {
                        doc: first.id.document_id(),
                    },
                    kind: ObjectKind::Unknown,
                    label: "Document".to_string(),
                    preview: "PDF object graph".to_string(),
                    object: None,
                    has_children: true,
                    has_stream: false,
                    child_count: page.total,
                    byte_size_hint: None,
                    diagnostics: Vec::new(),
                },
                depth: 0,
                expanded: true,
            });
        }
        rows.extend(page.items.iter().cloned().map(|summary| RealTreeRow {
            summary,
            depth: 1,
            expanded: false,
        }));

        Self {
            rows,
            root_children: page.items.clone(),
            total: page.total,
        }
    }

    fn row_count(&self) -> usize {
        self.rows.len().max(1)
    }

    fn loaded_child_count(&self) -> usize {
        self.rows
            .len()
            .saturating_sub(self.has_document_root_row() as usize)
    }

    fn has_document_root_row(&self) -> bool {
        self.rows
            .first()
            .is_some_and(|row| matches!(row.summary.id, NodeId::DocumentRoot { .. }))
    }

    fn initial_selected_row(&self) -> usize {
        if self.has_document_root_row() && self.rows.len() > 1 {
            1
        } else {
            0
        }
    }

    fn row_count_label(&self) -> String {
        match self.total {
            Some(total) => format!("{} loaded / {total} total", self.loaded_child_count()),
            None => format!("{} loaded", self.loaded_child_count()),
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

    fn page_root_row(&self) -> Option<usize> {
        self.rows.iter().position(|row| {
            matches!(
                &row.summary.id,
                NodeId::DictEntry { key, .. } if key == "Pages"
            ) || matches!(&row.summary.id, NodeId::PageRoot { .. })
        })
    }

    fn row_label(&self, row: usize) -> String {
        self.summary(row)
            .map(summary_inline_text)
            .unwrap_or_else(|| "no real rows loaded".to_string())
    }

    fn row_depth(&self, row: usize) -> Option<usize> {
        self.rows.get(row).map(|row| row.depth)
    }

    fn row_tree_marker(&self, row: usize) -> Option<&'static str> {
        let row = self.rows.get(row)?;
        if !row.summary.has_children {
            return Some(" ");
        }
        if row.expanded {
            Some("-")
        } else {
            Some("+")
        }
    }

    fn row_page_index(&self, row: usize) -> Option<usize> {
        self.rows
            .get(row)
            .and_then(|row| page_index_for_node(&row.summary.id))
    }

    fn row_for_page_index(&self, page_index: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| is_page_row_node_for_index(&row.summary.id, page_index))
    }

    fn first_page_content_stream_row(&self, page_row: usize) -> Option<usize> {
        let end = self.subtree_end(page_row);
        (page_row + 1..end).find(|row| {
            self.rows.get(*row).is_some_and(|tree_row| {
                is_page_content_stream_node(&tree_row.summary.id)
                    && summary_has_stream(&tree_row.summary)
            })
        })
    }

    fn page_content_candidate_rows(&self, page_row: usize) -> Vec<usize> {
        let end = self.subtree_end(page_row);
        (page_row + 1..end)
            .filter(|row| {
                self.rows.get(*row).is_some_and(|tree_row| {
                    is_page_content_stream_node(&tree_row.summary.id)
                        && (tree_row.summary.has_children || summary_has_stream(&tree_row.summary))
                })
            })
            .collect()
    }

    fn row_visual_target(&self, row: usize) -> Option<RealVisualTarget> {
        let row = self.rows.get(row)?;
        let page_index = page_index_for_node(&row.summary.id)?;
        let object = row.summary.object.or_else(|| row.summary.id.object_id())?;
        if !row.summary.has_stream && !matches!(row.summary.kind, ObjectKind::Stream) {
            return None;
        }
        Some(RealVisualTarget {
            page_index,
            object,
            allow_page_union: is_page_content_stream_node(&row.summary.id),
        })
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
        job.append(
            tree_kind_badge_text(&row.summary),
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
            depth: if self.has_document_root_row() { 1 } else { 0 },
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
                depth: hit
                    .depth
                    .saturating_add(self.has_document_root_row() as usize)
                    .min(8),
                expanded: false,
            });
            return Some(self.rows.len() - 1);
        }

        hit.object.map(|object| self.ensure_object_row(doc, object))
    }

    fn ensure_page_child_row(
        &mut self,
        page_root_row: usize,
        summary: ObjectSummary,
    ) -> Option<usize> {
        if let Some(row) = self.row_for_node(&summary.id) {
            return Some(row);
        }
        let parent_depth = self.rows.get(page_root_row)?.depth;
        if let Some(parent) = self.rows.get_mut(page_root_row) {
            parent.expanded = true;
        }
        let insert_at = self.subtree_end(page_root_row);
        self.rows.insert(
            insert_at,
            RealTreeRow {
                summary,
                depth: parent_depth + 1,
                expanded: false,
            },
        );
        Some(insert_at)
    }

    fn subtree_end(&self, row: usize) -> usize {
        let Some(parent) = self.rows.get(row) else {
            return self.rows.len();
        };
        let parent_depth = parent.depth;
        let mut end = row + 1;
        while end < self.rows.len() && self.rows[end].depth > parent_depth {
            end += 1;
        }
        end
    }

    fn update_row_from_detail(&mut self, row: usize, detail: &ObjectDetail) {
        let Some(row) = self.rows.get_mut(row) else {
            return;
        };
        row.summary.kind = detail.kind.clone();
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

    fn expand_cached_document_root(&mut self, row: usize) -> usize {
        let Some(parent) = self.rows.get_mut(row) else {
            return 0;
        };
        if parent.expanded || !matches!(parent.summary.id, NodeId::DocumentRoot { .. }) {
            return 0;
        }
        parent.expanded = true;
        let child_depth = parent.depth + 1;
        let children = self
            .root_children
            .iter()
            .cloned()
            .map(|summary| RealTreeRow {
                summary,
                depth: child_depth,
                expanded: false,
            })
            .collect::<Vec<_>>();
        let inserted = children.len();
        self.rows.splice(row + 1..row + 1, children);
        inserted
    }

    fn collapse_expanded_rows_except_selected_path(
        &mut self,
        selected_id: &NodeId,
    ) -> Option<usize> {
        let selected_row = self.row_for_node(selected_id)?;
        let keep = self.selected_path_node_ids(selected_row);
        let mut row = self.rows.len();
        while row > 0 {
            row -= 1;
            if keep.contains(&self.rows[row].summary.id) {
                continue;
            }
            if self.rows[row].expanded {
                self.collapse_row(row);
            }
        }
        self.row_for_node(selected_id)
    }

    fn selected_path_node_ids(&self, selected_row: usize) -> HashSet<NodeId> {
        let mut keep = HashSet::new();
        let Some(selected) = self.rows.get(selected_row) else {
            return keep;
        };
        keep.insert(selected.summary.id.clone());
        let mut next_depth = selected.depth;
        for row in (0..selected_row).rev() {
            let candidate = &self.rows[row];
            if candidate.depth < next_depth {
                keep.insert(candidate.summary.id.clone());
                next_depth = candidate.depth;
                if next_depth == 0 {
                    break;
                }
            }
        }
        keep
    }

    fn collapse_row(&mut self, row: usize) -> usize {
        let Some(parent) = self.rows.get_mut(row) else {
            return 0;
        };
        if !parent.expanded || !parent.summary.has_children {
            return 0;
        }
        parent.expanded = false;
        let parent_depth = parent.depth;
        let end = self.rows[row + 1..]
            .iter()
            .position(|child| child.depth <= parent_depth)
            .map(|offset| row + 1 + offset)
            .unwrap_or(self.rows.len());
        let removed = end.saturating_sub(row + 1);
        if removed > 0 {
            self.rows.drain(row + 1..end);
        }
        removed
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

    #[test]
    fn workspace_layout_preserves_initial_preview_width_on_retina_sized_window() {
        let layout = workspace_panel_layout(1024.0);

        assert!(layout.left.default + layout.right.default <= 1024.0 - PAGE_PREVIEW_MIN_WIDTH);
        assert!(layout.left.default < LEFT_PANEL_DEFAULT_WIDTH);
        assert!(layout.right.default < RIGHT_PANEL_DEFAULT_WIDTH);
        assert_eq!(layout.left.max, LEFT_PANEL_MAX_WIDTH);
        assert_eq!(layout.right.max, RIGHT_PANEL_MAX_WIDTH);
    }

    #[test]
    fn workspace_layout_keeps_wide_window_side_panels_roomy() {
        let layout = workspace_panel_layout(1920.0);

        assert_eq!(layout.left.default, LEFT_PANEL_DEFAULT_WIDTH);
        assert_eq!(layout.right.default, RIGHT_PANEL_DEFAULT_WIDTH);
        assert_eq!(layout.left.max, LEFT_PANEL_MAX_WIDTH);
        assert_eq!(layout.right.max, RIGHT_PANEL_MAX_WIDTH);
    }

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

    #[test]
    fn font_families_include_cjk_fallback_for_pdf_names() {
        let fonts = pdbg_fonts();
        assert!(fonts.font_data.contains_key(CJK_FONT_NAME));
        assert!(fonts
            .families
            .get(&FontFamily::Name("pdbg-sans".into()))
            .unwrap()
            .iter()
            .any(|name| name == "pdbg-cjk"));
        assert!(fonts
            .families
            .get(&FontFamily::Name("pdbg-mono".into()))
            .unwrap()
            .iter()
            .any(|name| name == "pdbg-cjk"));
        assert!(fonts
            .families
            .get(&FontFamily::Proportional)
            .unwrap()
            .iter()
            .any(|name| name == "pdbg-cjk"));
        assert!(fonts
            .families
            .get(&FontFamily::Monospace)
            .unwrap()
            .iter()
            .any(|name| name == "pdbg-cjk"));
    }

    #[test]
    fn page_preview_display_size_reserves_footer_space() {
        let texture_size = egui::vec2(900.0, 1400.0);
        let available = egui::vec2(900.0, 1000.0);
        let display_size = page_preview_display_size(texture_size, available, 80.0, 1.0);
        assert!(display_size.y <= 920.0);
        assert!(available.y - display_size.y >= 80.0);
    }

    #[test]
    fn page_preview_display_size_applies_visual_zoom() {
        let base_texture_size = egui::vec2(900.0, 1400.0);
        let available = egui::vec2(900.0, 1000.0);
        let fit_size = page_preview_display_size(base_texture_size, available, 80.0, 1.0);
        let zoomed_size = page_preview_display_size(base_texture_size * 2.0, available, 80.0, 2.0);
        assert!(zoomed_size.y > fit_size.y * 1.9);
    }

    #[test]
    fn page_preview_leading_space_does_not_hide_wide_image_left_edge() {
        assert_eq!(page_preview_leading_space(900.0, 1400.0), 0.0);
        assert_eq!(page_preview_leading_space(900.0, 700.0), 100.0);
    }

    #[test]
    fn render_zoom_steps_through_supported_levels() {
        assert_eq!(previous_render_zoom(0.5), None);
        assert_eq!(next_render_zoom(0.5), Some(1.0));
        assert_eq!(previous_render_zoom(1.0), Some(0.5));
        assert_eq!(next_render_zoom(1.0), Some(1.5));
        assert_eq!(previous_render_zoom(2.25), Some(2.0));
        assert_eq!(next_render_zoom(2.25), Some(3.0));
        assert_eq!(previous_render_zoom(4.0), Some(3.0));
        assert_eq!(next_render_zoom(4.0), None);
    }

    #[test]
    fn render_rotation_cycles_through_right_angles() {
        assert_eq!(next_render_rotation(0), 90);
        assert_eq!(next_render_rotation(90), 180);
        assert_eq!(next_render_rotation(180), 270);
        assert_eq!(next_render_rotation(270), 0);
        assert_eq!(next_render_rotation(450), 180);
        assert_eq!(next_render_rotation(-90), 0);
    }

    #[test]
    fn page_keyboard_shortcuts_choose_expected_pages() {
        assert_eq!(
            page_keyboard_target_page(2, 5, PageKeyboardShortcut::Previous),
            Some(1)
        );
        assert_eq!(
            page_keyboard_target_page(2, 5, PageKeyboardShortcut::Next),
            Some(3)
        );
        assert_eq!(
            page_keyboard_target_page(2, 5, PageKeyboardShortcut::First),
            Some(0)
        );
        assert_eq!(
            page_keyboard_target_page(2, 5, PageKeyboardShortcut::Last),
            Some(4)
        );
        assert_eq!(
            page_keyboard_target_page(0, 5, PageKeyboardShortcut::Previous),
            Some(0)
        );
        assert_eq!(
            page_keyboard_target_page(4, 5, PageKeyboardShortcut::Next),
            Some(4)
        );
        assert_eq!(
            page_keyboard_target_page(0, 0, PageKeyboardShortcut::Next),
            None
        );
    }

    #[test]
    fn render_key_request_applies_configured_dimension_limit() {
        let key = RealRenderKey::new(0, 1.5, 90, 8192);
        let request = key.request();

        assert_eq!(request.max_width, 8192);
        assert_eq!(request.max_height, 8192);
        assert_eq!(request.max_pixels, 8192 * 8192);
        assert_eq!(request.max_output_bytes, 8192 * 8192 * 4);
        assert_eq!(request.zoom, 1.5);
        assert_eq!(request.rotation_degrees, 90);
    }

    #[test]
    fn gui_options_default_render_dimension_limit_when_unset_or_zero() {
        assert_eq!(
            render_max_dimension_or_default(None),
            DEFAULT_RENDER_MAX_DIMENSION
        );
        assert_eq!(
            render_max_dimension_or_default(Some(0)),
            DEFAULT_RENDER_MAX_DIMENSION
        );
        assert_eq!(render_max_dimension_or_default(Some(8192)), 8192);
    }

    #[test]
    fn preview_click_maps_to_render_pixel_coordinates() {
        let image_rect =
            egui::Rect::from_min_size(egui::pos2(100.0, 40.0), egui::vec2(300.0, 600.0));
        let click =
            preview_click_from_pos(egui::pos2(250.0, 340.0), image_rect, 900, 1800, 4).unwrap();
        assert_eq!(click.page_index, 4);
        assert!((click.render_x - 450.0).abs() < f32::EPSILON);
        assert!((click.render_y - 900.0).abs() < f32::EPSILON);
    }

    #[test]
    fn preview_click_hit_tests_text_span_bbox_in_page_space() {
        let page = TextPage {
            page_index: 1,
            spans: vec![TextSpan {
                text: "Abstract".to_string(),
                bbox: PageRect {
                    x: 100.0,
                    y: 50.0,
                    width: 200.0,
                    height: 24.0,
                },
                untrusted: false,
            }],
        };
        let click = PagePreviewClick {
            page_index: 1,
            render_x: 300.0,
            render_y: 120.0,
            normalized_x: 0.0,
            normalized_y: 0.0,
        };
        let hit = text_hit_from_page_click(&page, click, 2.0).unwrap();
        assert_eq!(hit.page_index, 1);
        assert_eq!(hit.span_index, 0);
        assert_eq!(hit.excerpt, "Abstract");

        let miss = PagePreviewClick {
            render_x: 20.0,
            render_y: 20.0,
            ..click
        };
        assert!(text_hit_from_page_click(&page, miss, 2.0).is_none());
    }

    #[test]
    fn preview_click_hit_tests_visual_bbox_in_page_space() {
        let page = VisualPage {
            page_index: 2,
            elements: vec![
                VisualElement {
                    kind: VisualElementKind::Text,
                    bbox: PageRect {
                        x: 10.0,
                        y: 10.0,
                        width: 500.0,
                        height: 500.0,
                    },
                    object: None,
                    untrusted: true,
                },
                VisualElement {
                    kind: VisualElementKind::Image,
                    bbox: PageRect {
                        x: 100.0,
                        y: 50.0,
                        width: 200.0,
                        height: 80.0,
                    },
                    object: Some(ObjectId { num: 12, gen: 0 }),
                    untrusted: false,
                },
            ],
        };
        let click = PagePreviewClick {
            page_index: 2,
            render_x: 300.0,
            render_y: 140.0,
            normalized_x: 0.0,
            normalized_y: 0.0,
        };

        let hit = visual_hit_from_page_click(&page, click, 2.0).unwrap();
        assert_eq!(hit.page_index, 2);
        assert_eq!(hit.element_index, 1);
        assert_eq!(hit.kind, VisualElementKind::Image);
        assert_eq!(hit.object, Some(ObjectId { num: 12, gen: 0 }));
        assert!(hit.contains_click);

        let miss = PagePreviewClick {
            render_x: 2.0,
            render_y: 2.0,
            ..click
        };
        assert!(visual_hit_from_page_click(&page, miss, 2.0).is_none());
    }

    #[test]
    fn visual_hit_for_object_unions_matching_bboxes() {
        let object = ObjectId { num: 30, gen: 0 };
        let page = VisualPage {
            page_index: 0,
            elements: vec![
                VisualElement {
                    kind: VisualElementKind::Text,
                    bbox: PageRect {
                        x: 20.0,
                        y: 10.0,
                        width: 40.0,
                        height: 30.0,
                    },
                    object: Some(object),
                    untrusted: false,
                },
                VisualElement {
                    kind: VisualElementKind::Vector,
                    bbox: PageRect {
                        x: 50.0,
                        y: 30.0,
                        width: 60.0,
                        height: 25.0,
                    },
                    object: Some(object),
                    untrusted: true,
                },
                VisualElement {
                    kind: VisualElementKind::Image,
                    bbox: PageRect {
                        x: 0.0,
                        y: 0.0,
                        width: 10.0,
                        height: 10.0,
                    },
                    object: Some(ObjectId { num: 99, gen: 0 }),
                    untrusted: false,
                },
            ],
        };

        let hit = visual_hit_for_object(&page, object).unwrap();

        assert_eq!(hit.page_index, 0);
        assert_eq!(hit.element_index, 0);
        assert_eq!(hit.kind, VisualElementKind::Unknown);
        assert_eq!(hit.object, Some(object));
        assert!(hit.untrusted);
        assert!(!hit.contains_click);
        assert_eq!(hit.bbox.x, 20.0);
        assert_eq!(hit.bbox.y, 10.0);
        assert_eq!(hit.bbox.width, 90.0);
        assert_eq!(hit.bbox.height, 45.0);
    }

    #[test]
    fn visual_hit_for_page_visual_union_uses_all_visible_bboxes() {
        let object = ObjectId { num: 30, gen: 0 };
        let page = VisualPage {
            page_index: 1,
            elements: vec![
                VisualElement {
                    kind: VisualElementKind::Vector,
                    bbox: PageRect {
                        x: 10.0,
                        y: 20.0,
                        width: 30.0,
                        height: 40.0,
                    },
                    object: None,
                    untrusted: false,
                },
                VisualElement {
                    kind: VisualElementKind::Vector,
                    bbox: PageRect {
                        x: 50.0,
                        y: 5.0,
                        width: 20.0,
                        height: 30.0,
                    },
                    object: None,
                    untrusted: false,
                },
                VisualElement {
                    kind: VisualElementKind::Image,
                    bbox: PageRect {
                        x: 80.0,
                        y: 80.0,
                        width: 0.0,
                        height: 10.0,
                    },
                    object: None,
                    untrusted: true,
                },
            ],
        };

        let hit = visual_hit_for_page_visual_union(&page, object).unwrap();

        assert_eq!(hit.page_index, 1);
        assert_eq!(hit.element_index, 0);
        assert_eq!(hit.kind, VisualElementKind::Vector);
        assert_eq!(hit.object, Some(object));
        assert!(!hit.untrusted);
        assert_eq!(hit.bbox.x, 10.0);
        assert_eq!(hit.bbox.y, 5.0);
        assert_eq!(hit.bbox.width, 60.0);
        assert_eq!(hit.bbox.height, 55.0);
    }

    #[test]
    fn page_content_stream_node_detection_follows_contents_arrays() {
        let doc = pdbg_core::DocumentId(7);
        let page = NodeId::Page {
            doc: doc.clone(),
            index: 0,
        };
        let contents = NodeId::DictEntry {
            doc: doc.clone(),
            parent: Box::new(page),
            key: "Contents".to_string(),
        };
        let content_item = NodeId::ArrayEntry {
            doc: doc.clone(),
            parent: Box::new(contents),
            index: 0,
        };
        let media_box = NodeId::DictEntry {
            doc,
            parent: Box::new(NodeId::Page {
                doc: pdbg_core::DocumentId(7),
                index: 0,
            }),
            key: "MediaBox".to_string(),
        };

        assert!(is_page_content_stream_node(&content_item));
        assert!(!is_page_content_stream_node(&media_box));
    }

    #[test]
    fn real_tree_finds_page_content_stream_descendant() {
        let doc = pdbg_core::DocumentId(7);
        let page = NodeId::ArrayEntry {
            doc: doc.clone(),
            parent: Box::new(NodeId::PageRoot { doc: doc.clone() }),
            index: 1,
        };
        let content_stream = ObjectSummary {
            id: NodeId::DictEntry {
                doc: doc.clone(),
                parent: Box::new(page.clone()),
                key: "Contents".to_string(),
            },
            kind: ObjectKind::Stream,
            label: "Contents".to_string(),
            preview: "7 0 R stream".to_string(),
            object: Some(ObjectId { num: 7, gen: 0 }),
            has_children: false,
            has_stream: true,
            child_count: None,
            byte_size_hint: None,
            diagnostics: Vec::new(),
        };
        let media_box = ObjectSummary {
            id: NodeId::DictEntry {
                doc,
                parent: Box::new(page.clone()),
                key: "MediaBox".to_string(),
            },
            kind: ObjectKind::Array,
            label: "MediaBox".to_string(),
            preview: String::new(),
            object: None,
            has_children: false,
            has_stream: false,
            child_count: None,
            byte_size_hint: None,
            diagnostics: Vec::new(),
        };
        let tree = RealObjectTree {
            rows: vec![
                RealTreeRow {
                    summary: ObjectSummary {
                        id: page,
                        kind: ObjectKind::Page,
                        label: "Page 2".to_string(),
                        preview: String::new(),
                        object: None,
                        has_children: true,
                        has_stream: false,
                        child_count: Some(2),
                        byte_size_hint: None,
                        diagnostics: Vec::new(),
                    },
                    depth: 0,
                    expanded: true,
                },
                RealTreeRow {
                    summary: media_box,
                    depth: 1,
                    expanded: false,
                },
                RealTreeRow {
                    summary: content_stream,
                    depth: 1,
                    expanded: false,
                },
            ],
            root_children: Vec::new(),
            total: Some(1),
        };

        assert_eq!(tree.first_page_content_stream_row(0), Some(2));
        assert_eq!(tree.page_content_candidate_rows(0), vec![2]);
    }

    #[test]
    fn selecting_tree_row_clears_preview_hit_selection() {
        let mut app = GuiShellApp::new();
        app.preview_click = Some(PagePreviewClick {
            page_index: 0,
            render_x: 10.0,
            render_y: 20.0,
            normalized_x: 0.1,
            normalized_y: 0.2,
        });
        app.selected_visual_hit = Some(PreviewVisualHit {
            page_index: 0,
            element_index: 0,
            kind: VisualElementKind::Text,
            bbox: PageRect {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
            },
            object: None,
            untrusted: true,
            contains_click: true,
        });

        app.select_row_from_tree(app.selected_row);

        assert!(app.preview_click.is_none());
        assert!(app.selected_visual_hit.is_none());
        assert!(app.selected_text_hit.is_none());
    }

    #[test]
    fn visual_page_cache_reuses_lru_and_respects_element_budget() {
        let mut cache = VisualPageCache::new(2, 3);
        cache.insert(VisualPage {
            page_index: 1,
            elements: vec![visual_test_element(1.0)],
        });
        cache.insert(VisualPage {
            page_index: 2,
            elements: vec![visual_test_element(2.0)],
        });

        assert!(cache.get(1).is_some());
        cache.insert(VisualPage {
            page_index: 3,
            elements: vec![visual_test_element(3.0)],
        });

        assert!(cache.get(2).is_none());
        assert!(cache.get(1).is_some());
        assert!(cache.get(3).is_some());

        cache.insert(VisualPage {
            page_index: 4,
            elements: vec![
                visual_test_element(4.0),
                visual_test_element(5.0),
                visual_test_element(6.0),
                visual_test_element(7.0),
            ],
        });
        assert!(cache.get(4).is_none());
    }

    #[test]
    fn visual_object_attribution_text_hides_missing_object() {
        let mut hit = PreviewVisualHit {
            page_index: 0,
            element_index: 0,
            kind: VisualElementKind::Text,
            bbox: PageRect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            object: None,
            untrusted: true,
            contains_click: true,
        };

        assert_eq!(visual_object_attribution_text(None), None);
        assert_eq!(visual_object_attribution_text(Some(&hit)), None);
        hit.object = Some(ObjectId { num: 12, gen: 0 });
        assert_eq!(
            visual_object_attribution_text(Some(&hit)).as_deref(),
            Some("12 0 R")
        );
    }

    fn visual_test_element(x: f32) -> VisualElement {
        VisualElement {
            kind: VisualElementKind::Text,
            bbox: PageRect {
                x,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            object: None,
            untrusted: true,
        }
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
    fn pdf_content_stream_nice_text_indents_content_stream_structure() {
        let nice = pdf_content_stream_nice_text(
            b"q 1 0 0 -1 0 792 cm q /GS63 gs 0 0 541.44 753.96 re f* Q \
              /P << /MCID 0 >> BDC BT /FT68 360 Tf <0017> Tj ET EMC Q",
        );

        assert_eq!(
            nice,
            "q\n  1 0 0 -1 0 792 cm\n  q\n    /GS63 gs\n    0 0 541.44 753.96 re\n    f*\n  Q\n  /P << /MCID 0 >> BDC\n    BT\n      /FT68 360 Tf\n      <0017> Tj\n    ET\n  EMC\nQ\n"
        );
    }

    #[test]
    fn nice_stream_render_lines_group_selection_by_structural_block() {
        let object = ObjectId { num: 38, gen: 0 };
        let chunks = vec![StreamChunk {
            mode: StreamMode::Decoded,
            offset: 0,
            bytes: b"q /P << /MCID 0 >> BDC BT /F1 9 Tf (Hi) Tj ET EMC Q".to_vec(),
            total_size: Some(64),
            truncated: false,
            decode_diagnostics: Vec::new(),
        }];

        let rows = real_stream_nice_render_lines(object, &chunks);
        let bdc_key = rows
            .iter()
            .find(|row| row.line.text.ends_with(" BDC"))
            .unwrap()
            .line_key
            .clone();
        let bt_key = rows
            .iter()
            .find(|row| row.line.text == "BT")
            .unwrap()
            .line_key
            .clone();

        assert_eq!(
            rows.iter()
                .find(|row| row.line.text == "/F1 9 Tf")
                .unwrap()
                .block_key
                .as_deref(),
            Some(bt_key.as_str())
        );
        assert!(rows
            .iter()
            .find(|row| row.line.text == "/F1 9 Tf")
            .unwrap()
            .guide_blocks
            .iter()
            .any(|(_, key)| key == &bdc_key));
        assert!(rows
            .iter()
            .find(|row| row.line.text == "/F1 9 Tf")
            .unwrap()
            .guide_blocks
            .iter()
            .any(|(_, key)| key == &bt_key));
        assert_eq!(
            rows.iter()
                .find(|row| row.line.text == "EMC")
                .unwrap()
                .block_key
                .as_deref(),
            Some(bdc_key.as_str())
        );
    }

    #[test]
    fn nice_stream_selection_extracts_text_fragments_from_block() {
        let object = ObjectId { num: 7, gen: 0 };
        let chunks = vec![StreamChunk {
            mode: StreamMode::Decoded,
            offset: 0,
            bytes: b"BT /F1 12 Tf (Country of Citizenship) Tj ET".to_vec(),
            total_size: Some(47),
            truncated: false,
            decode_diagnostics: Vec::new(),
        }];
        let rows = real_stream_nice_render_lines(object, &chunks);
        let bt_key = rows
            .iter()
            .find(|row| row.line.text == "BT")
            .unwrap()
            .line_key
            .clone();

        assert_eq!(
            nice_stream_text_fragments_for_selection(&rows, &bt_key),
            vec!["Country of Citizenship"]
        );
    }

    #[test]
    fn text_hit_for_fragments_unions_matching_text_spans() {
        let page = TextPage {
            page_index: 1,
            spans: vec![
                TextSpan {
                    text: "Country of".to_string(),
                    bbox: PageRect {
                        x: 10.0,
                        y: 20.0,
                        width: 40.0,
                        height: 8.0,
                    },
                    untrusted: false,
                },
                TextSpan {
                    text: "Citizenship".to_string(),
                    bbox: PageRect {
                        x: 52.0,
                        y: 18.0,
                        width: 48.0,
                        height: 12.0,
                    },
                    untrusted: true,
                },
            ],
        };

        let hit = text_hit_for_text_fragments(&page, &["Country of Citizenship".to_string()])
            .expect("expected positioned text hit");

        assert_eq!(hit.page_index, 1);
        assert_eq!(hit.span_index, 0);
        assert!(hit.untrusted);
        let bbox = hit.bbox.unwrap();
        assert_eq!(bbox.x, 10.0);
        assert_eq!(bbox.y, 18.0);
        assert_eq!(bbox.width, 90.0);
        assert_eq!(bbox.height, 12.0);
    }

    #[test]
    fn nice_stream_text_hit_selects_matching_text_show_line() {
        let object = ObjectId { num: 7, gen: 0 };
        let chunks = vec![StreamChunk {
            mode: StreamMode::Decoded,
            offset: 0,
            bytes: b"BT /F1 12 Tf (Country of Citizenship) Tj (China) Tj ET".to_vec(),
            total_size: Some(59),
            truncated: false,
            decode_diagnostics: Vec::new(),
        }];
        let rows = real_stream_nice_render_lines(object, &chunks);
        let hit = TextSearchHit {
            page_index: 0,
            span_index: 0,
            excerpt: "Country of Citizenship".to_string(),
            bbox: None,
            untrusted: false,
        };

        let key = nice_stream_selection_key_for_text_hit(&rows, &hit).unwrap();
        let row = rows.iter().find(|row| row.line_key == key).unwrap();

        assert_eq!(row.line.text, "(Country of Citizenship) Tj");
    }

    #[test]
    fn nice_stream_visual_selection_matches_vector_and_image_order() {
        let object = ObjectId { num: 7, gen: 0 };
        let chunks = vec![StreamChunk {
            mode: StreamMode::Decoded,
            offset: 0,
            bytes: b"BT (Title) Tj ET q 10 10 20 20 re f Q q /Im0 Do Q".to_vec(),
            total_size: Some(57),
            truncated: false,
            decode_diagnostics: Vec::new(),
        }];
        let rows = real_stream_nice_render_lines(object, &chunks);
        let page = VisualPage {
            page_index: 0,
            elements: vec![
                VisualElement {
                    kind: VisualElementKind::Text,
                    bbox: PageRect {
                        x: 1.0,
                        y: 1.0,
                        width: 10.0,
                        height: 5.0,
                    },
                    object: None,
                    untrusted: false,
                },
                VisualElement {
                    kind: VisualElementKind::Vector,
                    bbox: PageRect {
                        x: 10.0,
                        y: 10.0,
                        width: 20.0,
                        height: 20.0,
                    },
                    object: None,
                    untrusted: false,
                },
                VisualElement {
                    kind: VisualElementKind::Image,
                    bbox: PageRect {
                        x: 40.0,
                        y: 50.0,
                        width: 30.0,
                        height: 25.0,
                    },
                    object: None,
                    untrusted: false,
                },
            ],
        };
        let vector_key = rows
            .iter()
            .find(|row| row.line.text == "f")
            .and_then(|row| row.block_key.clone())
            .unwrap();
        let image_key = rows
            .iter()
            .find(|row| row.line.text.ends_with(" Do"))
            .and_then(|row| row.block_key.clone())
            .unwrap();

        let vector_hit =
            nice_stream_visual_hit_for_selection(&page, &rows, &vector_key, object).unwrap();
        let image_hit =
            nice_stream_visual_hit_for_selection(&page, &rows, &image_key, object).unwrap();

        assert_eq!(vector_hit.element_index, 1);
        assert_eq!(vector_hit.kind, VisualElementKind::Vector);
        assert_eq!(vector_hit.bbox.x, 10.0);
        assert_eq!(image_hit.element_index, 2);
        assert_eq!(image_hit.kind, VisualElementKind::Image);
        assert_eq!(image_hit.bbox.x, 40.0);
    }

    #[test]
    fn nice_stream_visual_hit_selects_matching_draw_operation() {
        let object = ObjectId { num: 7, gen: 0 };
        let chunks = vec![StreamChunk {
            mode: StreamMode::Decoded,
            offset: 0,
            bytes: b"q 10 10 20 20 re f Q q /Im0 Do Q".to_vec(),
            total_size: Some(37),
            truncated: false,
            decode_diagnostics: Vec::new(),
        }];
        let rows = real_stream_nice_render_lines(object, &chunks);
        let page = VisualPage {
            page_index: 0,
            elements: vec![
                VisualElement {
                    kind: VisualElementKind::Vector,
                    bbox: PageRect {
                        x: 10.0,
                        y: 10.0,
                        width: 20.0,
                        height: 20.0,
                    },
                    object: None,
                    untrusted: false,
                },
                VisualElement {
                    kind: VisualElementKind::Image,
                    bbox: PageRect {
                        x: 40.0,
                        y: 50.0,
                        width: 30.0,
                        height: 25.0,
                    },
                    object: None,
                    untrusted: false,
                },
            ],
        };
        let hit = PreviewVisualHit {
            page_index: 0,
            element_index: 1,
            kind: VisualElementKind::Image,
            bbox: page.elements[1].bbox.clone(),
            object: None,
            untrusted: false,
            contains_click: true,
        };

        let key = nice_stream_selection_key_for_visual_hit(&page, &rows, &hit).unwrap();
        let row = rows.iter().find(|row| row.line_key == key).unwrap();

        assert_eq!(row.line.text, "/Im0 Do");
    }

    #[test]
    fn real_stream_default_limit_uses_bounded_windows() {
        let stream = StreamSummary {
            object: ObjectId { num: 262, gen: 0 },
            filters: vec!["FlateDecode".to_string()],
            raw_size_hint: Some(6417),
            decoded_size_hint: None,
            can_decode: true,
            image_preview_available: false,
        };

        assert_eq!(real_stream_default_limit(&stream, StreamMode::Raw), 6417);
        assert_eq!(
            real_stream_default_limit(&stream, StreamMode::Decoded),
            REAL_STREAM_DEFAULT_VIEW_LIMIT_BYTES
        );
    }

    #[test]
    fn real_stream_chunks_range_label_describes_loaded_span_not_manual_windows() {
        let chunks = vec![StreamChunk {
            mode: StreamMode::Decoded,
            offset: 0,
            bytes: vec![b' '; 4096],
            total_size: Some(72_511),
            truncated: true,
            decode_diagnostics: Vec::new(),
        }];

        let label = real_stream_chunks_range_label(&chunks);

        assert_eq!(label, "bytes 0..4096 / total 72511 / more");
        assert!(!label.contains("truncated"));
        assert!(real_stream_chunks_has_more(&chunks));
    }

    #[test]
    fn real_stream_scroll_request_loads_adjacent_content_at_edges() {
        let chunks = vec![
            StreamChunk {
                mode: StreamMode::Decoded,
                offset: 4096,
                bytes: vec![b' '; 4096],
                total_size: Some(16_000),
                truncated: false,
                decode_diagnostics: Vec::new(),
            },
            StreamChunk {
                mode: StreamMode::Decoded,
                offset: 8192,
                bytes: vec![b' '; 4096],
                total_size: Some(16_000),
                truncated: true,
                decode_diagnostics: Vec::new(),
            },
        ];

        assert_eq!(
            real_stream_scroll_request(300.0, 100.0, 400.0, -24.0, true, &chunks, 4096),
            Some(12_288)
        );
        assert_eq!(
            real_stream_scroll_request(0.0, 100.0, 400.0, 24.0, true, &chunks, 4096),
            Some(0)
        );
        assert_eq!(
            real_stream_scroll_request(160.0, 100.0, 400.0, -24.0, true, &chunks, 4096),
            None
        );
        assert_eq!(
            real_stream_scroll_request(300.0, 100.0, 400.0, -24.0, false, &chunks, 4096),
            None
        );
    }

    #[test]
    fn real_stream_window_cache_keeps_recent_neighbors_bounded() {
        let mut app = GuiShellApp::new();
        let object = ObjectId { num: 4, gen: 0 };
        for index in 0..(REAL_STREAM_MAX_LOADED_WINDOWS + 2) {
            let offset = (index * 64) as u64;
            let key = RealStreamKey {
                object,
                mode: StreamMode::Decoded,
                offset,
                limit: 64,
            };
            app.insert_real_stream_window(
                key,
                StreamChunk {
                    mode: StreamMode::Decoded,
                    offset,
                    bytes: vec![b' '; 64],
                    total_size: Some(1024),
                    truncated: true,
                    decode_diagnostics: Vec::new(),
                },
            );
        }

        assert_eq!(
            app.real_stream_windows.len(),
            REAL_STREAM_MAX_LOADED_WINDOWS
        );
        assert_eq!(app.real_stream_windows.front().unwrap().key.offset, 2 * 64);
        assert_eq!(
            app.real_stream_windows.back().unwrap().key.offset,
            (REAL_STREAM_MAX_LOADED_WINDOWS + 1) as u64 * 64
        );
    }

    #[test]
    fn real_stream_view_presets_choose_nice_text_and_raw_hex() {
        assert_eq!(
            real_stream_preset_defaults(RealStreamPreset::Nice, true),
            (StreamMode::Decoded, StreamViewMode::Text)
        );
        assert_eq!(
            real_stream_preset_defaults(RealStreamPreset::Nice, false),
            (StreamMode::Raw, StreamViewMode::Text)
        );
        assert_eq!(
            real_stream_preset_defaults(RealStreamPreset::Raw, true),
            (StreamMode::Raw, StreamViewMode::Hex)
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
        assert_eq!(tree.row_depth(0), Some(0));
        assert_eq!(tree.row_depth(1), Some(1));
        assert_eq!(tree.row_tree_marker(0), Some("-"));
        assert!(tree.row_label(1).contains("[12 0 R]"));
        let job = tree.row_layout_job(1, false);
        let row_text = job.text;
        assert!(row_text.contains("<> Info (3) [12 0 R]"));
        assert!(!row_text.contains("preview"));
        assert!(row_text.contains("stream"));
    }

    #[test]
    fn real_tree_detail_refresh_keeps_structural_label_stable() {
        let id = NodeId::DocumentRoot {
            doc: pdbg_core::DocumentId(7),
        };
        let summary = ObjectSummary {
            id: id.clone(),
            kind: ObjectKind::Unknown,
            label: "Trailer".to_string(),
            preview: "PDF trailer dictionary".to_string(),
            object: None,
            has_children: true,
            has_stream: false,
            child_count: Some(4),
            byte_size_hint: None,
            diagnostics: Vec::new(),
        };
        let mut tree = RealObjectTree {
            rows: vec![RealTreeRow {
                summary,
                depth: 0,
                expanded: false,
            }],
            root_children: Vec::new(),
            total: Some(1),
        };
        let detail = ObjectDetail {
            id,
            kind: ObjectKind::Trailer,
            object: None,
            label: "Object".to_string(),
            preview: "<object preview exceeds max depth>".to_string(),
            value: ObjectValue::Container,
            dictionary_entries: Some(pdbg_core::ChildPage {
                total: Some(4),
                items: Vec::new(),
            }),
            array_entries: None,
            stream: None,
            diagnostics: Vec::new(),
        };

        tree.update_row_from_detail(0, &detail);

        let row_text = tree.row_layout_job(0, false).text;
        assert!(row_text.contains("trl Trailer (4)"));
        assert!(!row_text.contains("Object"));
        assert_eq!(tree.row_label(0), "PDF trailer dictionary");
    }

    #[test]
    fn real_tree_page_rows_use_dictionary_badge_before_detail_load() {
        let doc = pdbg_core::DocumentId(9);
        let summary = ObjectSummary {
            id: NodeId::ArrayEntry {
                doc: doc.clone(),
                parent: Box::new(NodeId::PageRoot { doc: doc.clone() }),
                index: 3,
            },
            kind: ObjectKind::Page,
            label: "Page 4".to_string(),
            preview: String::new(),
            object: None,
            has_children: true,
            has_stream: false,
            child_count: None,
            byte_size_hint: None,
            diagnostics: Vec::new(),
        };
        let tree = RealObjectTree::from_child_page(&pdbg_core::ChildPage {
            total: Some(1),
            items: vec![summary],
        });

        let row_text = tree.row_layout_job(1, false).text;
        assert!(row_text.starts_with("<> Page 4"));
        assert!(!row_text.starts_with("page Page 4"));
    }

    #[test]
    fn real_tree_xref_object_rows_use_object_badge_before_detail_load() {
        let doc = pdbg_core::DocumentId(10);
        let summary = ObjectSummary {
            id: NodeId::XrefObject {
                doc,
                object: ObjectId { num: 4, gen: 0 },
            },
            kind: ObjectKind::XrefEntry,
            label: "Object 4 0 R".to_string(),
            preview: String::new(),
            object: Some(ObjectId { num: 4, gen: 0 }),
            has_children: true,
            has_stream: false,
            child_count: Some(5),
            byte_size_hint: None,
            diagnostics: Vec::new(),
        };
        let tree = RealObjectTree::from_child_page(&pdbg_core::ChildPage {
            total: Some(1),
            items: vec![summary],
        });

        let row_text = tree.row_layout_job(1, false).text;
        assert!(row_text.starts_with("<> Object 4 0 R"));
        assert!(!row_text.starts_with("xref Object 4 0 R"));
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
    fn page_index_for_node_follows_page_children() {
        let id = NodeId::DictEntry {
            doc: pdbg_core::DocumentId(1),
            parent: Box::new(NodeId::ArrayEntry {
                doc: pdbg_core::DocumentId(1),
                parent: Box::new(NodeId::Page {
                    doc: pdbg_core::DocumentId(1),
                    index: 3,
                }),
                index: 0,
            }),
            key: "Contents".to_string(),
        };

        assert_eq!(page_index_for_node(&id), Some(3));
        assert_eq!(
            page_index_for_node(&NodeId::ArrayEntry {
                doc: pdbg_core::DocumentId(1),
                parent: Box::new(NodeId::DictEntry {
                    doc: pdbg_core::DocumentId(1),
                    parent: Box::new(NodeId::Catalog {
                        doc: pdbg_core::DocumentId(1),
                    }),
                    key: "Pages".to_string(),
                }),
                index: 1,
            }),
            Some(1)
        );
        assert_eq!(
            page_index_for_node(&NodeId::ResourceGroup {
                doc: pdbg_core::DocumentId(1),
                page_index: 2,
                group: pdbg_core::ResourceGroup::XObjects,
            }),
            Some(2)
        );
        assert_eq!(
            page_index_for_node(&NodeId::Trailer {
                doc: pdbg_core::DocumentId(1),
            }),
            None
        );
    }

    #[test]
    fn page_row_node_detection_ignores_page_child_nodes() {
        let doc = pdbg_core::DocumentId(1);
        let page = NodeId::ArrayEntry {
            doc: doc.clone(),
            parent: Box::new(NodeId::PageRoot { doc: doc.clone() }),
            index: 2,
        };
        let media_box = NodeId::DictEntry {
            doc,
            parent: Box::new(page.clone()),
            key: "MediaBox".to_string(),
        };

        assert!(is_page_row_node_for_index(&page, 2));
        assert!(!is_page_row_node_for_index(&media_box, 2));
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
            render_max_dimension: None,
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
            render_max_dimension: None,
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
            render_max_dimension: None,
        });
        assert!(app.real_render_job.is_some());
        wait_for_real_render(&mut app);

        assert!(app.state.is_ok());
        assert!(matches!(app.tree, TreeModel::Real(_)));
        assert_eq!(app.tree.row_count(), 5);
        assert_eq!(app.tree.row_count_label(), "4 loaded / 4 total");
        assert!(app.real_detail.is_some());
        let pages = app.real_pages.as_ref().unwrap();
        assert_eq!(pages.total, Some(1));
        assert_eq!(pages.items[0].label, "Page 1");
        assert!(app.real_render.is_some());
        assert!(app.breadcrumb_label().contains("Root"));
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
            render_max_dimension: None,
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
        assert_eq!(app.tree.row_count(), 5);

        let xref_expand_start = Instant::now();
        app.select_row_from_tree(4);
        let xref_elapsed = xref_expand_start.elapsed();
        assert_eq!(app.tree.real_row_tree_marker(app.selected_row), Some("-"));
        assert!(app.tree.row_count() > 5);
        assert!(app.tree.row_count() < xref_size / 10);

        let pages_expand_start = Instant::now();
        app.select_row_from_tree(3);
        let pages_elapsed = pages_expand_start.elapsed();
        assert_eq!(app.tree.real_row_tree_marker(app.selected_row), Some("-"));
        assert!(app.tree.real_row_for_page_index(0).is_some());

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
            render_max_dimension: None,
        });
        wait_for_real_render(&mut app);

        app.follow_real_reference(ObjectId { num: 4, gen: 0 });
        assert!(app
            .real_detail
            .as_ref()
            .is_some_and(|detail| detail.stream.is_some()));
        app.real_stream_mode = StreamMode::Raw;
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
            render_max_dimension: None,
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
            render_max_dimension: None,
        });
        wait_for_real_render(&mut app);

        app.follow_real_reference(ObjectId { num: 4, gen: 0 });
        app.real_stream_mode = StreamMode::Raw;
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
            render_max_dimension: None,
        });
        wait_for_real_render(&mut app);

        assert_eq!(app.page_count(), 2);
        assert_eq!(app.real_pages.as_ref().unwrap().total, Some(2));
        assert_eq!(app.real_pages.as_ref().unwrap().items[1].label, "Page 2");
        let initial = app.real_render.as_ref().unwrap();
        assert_eq!(initial.page_index, 0);
        assert_eq!((initial.width, initial.height), (200, 100));

        app.render_zoom = 2.0;
        app.refresh_real_render();
        wait_for_real_render(&mut app);
        let zoomed = app.real_render.as_ref().unwrap();
        assert_eq!(zoomed.page_index, 0);
        assert_eq!((zoomed.width, zoomed.height), (400, 200));

        app.render_zoom = 1.0;
        app.refresh_real_render();
        wait_for_real_render(&mut app);
        let reset_zoom = app.real_render.as_ref().unwrap();
        assert_eq!(reset_zoom.page_index, 0);
        assert_eq!((reset_zoom.width, reset_zoom.height), (200, 100));

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
    fn real_gui_selecting_page_tree_row_refreshes_preview_page() {
        let path = write_temp_pdf("gui-tree-page-sync", &synthetic_two_page_pdf());
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: Some(path.to_string_lossy().to_string()),
            recent_files_path: None,
            start_empty_when_no_pdf: false,
            render_max_dimension: None,
        });
        wait_for_real_render(&mut app);

        let page_root_row = match &app.tree {
            TreeModel::Real(tree) => tree
                .rows
                .iter()
                .position(|row| {
                    matches!(
                        &row.summary.id,
                        NodeId::DictEntry { key, .. } if key == "Pages"
                    ) || matches!(&row.summary.id, NodeId::PageRoot { .. })
                })
                .unwrap(),
            TreeModel::Virtual(_) => panic!("expected real tree"),
        };
        app.select_row_from_tree(page_root_row);
        app.expand_selected_real_row();
        let page_two_row = match &app.tree {
            TreeModel::Real(tree) => tree
                .rows
                .iter()
                .position(|row| page_index_for_node(&row.summary.id) == Some(1))
                .unwrap(),
            TreeModel::Virtual(_) => panic!("expected real tree"),
        };

        app.select_row_from_tree(page_two_row);
        wait_for_real_render(&mut app);

        assert_eq!(app.render_page_index, 1);
        assert_eq!(app.real_render.as_ref().unwrap().page_index, 1);

        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_gui_pager_expands_and_selects_matching_page_tree_row() {
        let path = write_temp_pdf("gui-pager-tree-sync", &synthetic_two_page_pdf());
        let mut app = GuiShellApp::new_with_options(GuiRunOptions {
            smoke_exit_after: None,
            pdf_path: Some(path.to_string_lossy().to_string()),
            recent_files_path: None,
            start_empty_when_no_pdf: false,
            render_max_dimension: None,
        });
        wait_for_real_render(&mut app);

        assert!(app.tree.real_row_for_page_index(1).is_none());
        let page_root_row = app.tree.real_page_root_row().unwrap();
        let non_page_row = (0..app.tree.row_count())
            .find(|row| *row != page_root_row && app.tree.real_row_tree_marker(*row) == Some("+"))
            .unwrap();
        let non_page_id = match &app.tree {
            TreeModel::Real(tree) => tree
                .summary(non_page_row)
                .map(|summary| summary.id.clone())
                .unwrap(),
            TreeModel::Virtual(_) => panic!("expected real tree"),
        };
        app.expand_real_tree_row(non_page_row);
        let expanded_non_page_row = match &app.tree {
            TreeModel::Real(tree) => tree.row_for_node(&non_page_id).unwrap(),
            TreeModel::Virtual(_) => panic!("expected real tree"),
        };
        assert_eq!(
            app.tree.real_row_tree_marker(expanded_non_page_row),
            Some("-")
        );

        app.set_render_page_from_pager(1);
        wait_for_real_render(&mut app);

        let page_row = app.tree.real_row_for_page_index(1).unwrap();
        let page_root_row = app.tree.real_page_root_row().unwrap();
        let collapsed_non_page_row = match &app.tree {
            TreeModel::Real(tree) => tree.row_for_node(&non_page_id).unwrap(),
            TreeModel::Virtual(_) => panic!("expected real tree"),
        };
        assert_eq!(app.render_page_index, 1);
        assert_eq!(app.selected_row, page_row);
        assert_eq!(app.tree.real_row_tree_marker(page_root_row), Some("-"));
        assert_eq!(app.tree.real_row_tree_marker(page_row), Some("-"));
        assert_eq!(
            app.tree.real_row_tree_marker(collapsed_non_page_row),
            Some("+")
        );
        assert!(app.scroll_selected_tree_row);

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
            render_max_dimension: None,
        });
        wait_for_real_render(&mut app);

        app.set_render_page(1);
        wait_for_real_render(&mut app);
        assert_eq!(app.render_cache.len(), 2);

        app.set_render_page(0);

        assert!(app.real_render_job.is_none());
        let render = app.real_render.as_ref().unwrap();
        assert_eq!(render.page_index, 0);
        assert_eq!((render.width, render.height), (200, 100));
        assert!(app
            .status_log
            .iter()
            .any(|line| line.starts_with("reused cached page 1 @ 100%")));

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
            render_max_dimension: None,
        });
        assert!(app.real_render_job.is_some());

        app.set_render_page(1);
        wait_for_real_render(&mut app);

        let render = app.real_render.as_ref().unwrap();
        assert_eq!(render.page_index, 1);
        assert_eq!((render.width, render.height), (100, 200));
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
            recent_files_path: None,
            start_empty_when_no_pdf: false,
            render_max_dimension: None,
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
