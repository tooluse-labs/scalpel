use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) fn configure_egui(ctx: &egui::Context) {
    ctx.set_fonts(pdbg_fonts());
    ctx.set_global_style(pdbg_style());
}

pub(crate) fn pdbg_fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "InterVariable".to_string(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/InterVariable.ttf")).into(),
    );
    fonts.font_data.insert(
        "JetBrainsMono-Regular".to_string(),
        egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/JetBrainsMono-Regular.ttf"
        ))
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

pub(crate) fn insert_font_fallback(family: &mut Vec<String>) {
    if !family.iter().any(|font| font == CJK_FONT_NAME) {
        family.push(CJK_FONT_NAME.to_string());
    }
}

pub(crate) fn add_runtime_cjk_font(fonts: &mut FontDefinitions) -> bool {
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
pub(crate) fn pdbg_style() -> egui::Style {
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

    let mut visuals = if dark_mode_enabled() {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = theme().panel;
    visuals.window_fill = theme().surface;
    visuals.faint_bg_color = theme().canvas;
    visuals.extreme_bg_color = theme().code_bg;
    visuals.text_edit_bg_color = Some(theme().code_bg);
    visuals.code_bg_color = theme().code_bg;
    visuals.hyperlink_color = theme().accent;
    visuals.warn_fg_color = theme().warn_fg;
    visuals.error_fg_color = theme().error_fg;
    visuals.selection.bg_fill = theme().selected_bg;
    visuals.selection.stroke = egui::Stroke::new(1.0, theme().accent);
    visuals.widgets.noninteractive.fg_stroke.color = theme().text;
    visuals.widgets.inactive.weak_bg_fill = theme().chip_bg;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, theme().border);
    visuals.widgets.hovered.weak_bg_fill = theme().selected_bg;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, theme().accent);
    visuals.widgets.active.weak_bg_fill = theme().selected_bg;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.5, theme().accent);
    visuals.widgets.open.weak_bg_fill = theme().selected_bg;
    visuals.button_frame = true;
    visuals.striped = true;
    style.visuals = visuals;
    style
}

pub(crate) struct Palette {
    pub(crate) surface: Color32,
    pub(crate) panel: Color32,
    pub(crate) canvas: Color32,
    pub(crate) page: Color32,
    pub(crate) code_bg: Color32,
    pub(crate) chip_bg: Color32,
    pub(crate) selected_bg: Color32,
    pub(crate) top_bar: Color32,
    pub(crate) top_bar_text: Color32,
    pub(crate) top_bar_muted: Color32,
    pub(crate) text: Color32,
    pub(crate) muted: Color32,
    pub(crate) border: Color32,
    pub(crate) strong_border: Color32,
    pub(crate) accent: Color32,
    pub(crate) operator: Color32,
    pub(crate) safe: Color32,
    pub(crate) warn_bg: Color32,
    pub(crate) warn_fg: Color32,
    pub(crate) error_bg: Color32,
    pub(crate) error_fg: Color32,
}

const LIGHT_PALETTE: Palette = Palette {
    surface: Color32::from_rgb(251, 252, 253),
    panel: Color32::from_rgb(247, 249, 251),
    canvas: Color32::from_rgb(233, 237, 242),
    page: Color32::from_rgb(255, 253, 248),
    code_bg: Color32::from_rgb(245, 247, 250),
    chip_bg: Color32::from_rgb(238, 243, 247),
    selected_bg: Color32::from_rgb(232, 245, 246),
    top_bar: Color32::from_rgb(28, 37, 48),
    top_bar_text: Color32::from_rgb(236, 241, 246),
    top_bar_muted: Color32::from_rgb(170, 183, 196),
    text: Color32::from_rgb(31, 41, 51),
    muted: Color32::from_rgb(104, 116, 131),
    border: Color32::from_rgb(207, 215, 225),
    strong_border: Color32::from_rgb(179, 190, 203),
    accent: Color32::from_rgb(8, 127, 140),
    operator: Color32::from_rgb(215, 100, 53),
    safe: Color32::from_rgb(22, 132, 92),
    warn_bg: Color32::from_rgb(255, 244, 223),
    warn_fg: Color32::from_rgb(184, 107, 0),
    error_bg: Color32::from_rgb(255, 240, 238),
    error_fg: Color32::from_rgb(180, 35, 24),
};

// The page backdrop stays light in dark mode because rendered PDF pages are
// typically white; a dark frame around a white page reads better than tinting.
const DARK_PALETTE: Palette = Palette {
    surface: Color32::from_rgb(30, 35, 42),
    panel: Color32::from_rgb(25, 30, 36),
    canvas: Color32::from_rgb(18, 22, 27),
    page: Color32::from_rgb(255, 253, 248),
    code_bg: Color32::from_rgb(22, 27, 33),
    chip_bg: Color32::from_rgb(44, 52, 62),
    selected_bg: Color32::from_rgb(31, 58, 62),
    top_bar: Color32::from_rgb(17, 22, 29),
    top_bar_text: Color32::from_rgb(236, 241, 246),
    top_bar_muted: Color32::from_rgb(170, 183, 196),
    text: Color32::from_rgb(222, 228, 235),
    muted: Color32::from_rgb(146, 158, 172),
    border: Color32::from_rgb(58, 67, 79),
    strong_border: Color32::from_rgb(84, 96, 110),
    accent: Color32::from_rgb(72, 184, 196),
    operator: Color32::from_rgb(235, 142, 92),
    safe: Color32::from_rgb(78, 192, 144),
    warn_bg: Color32::from_rgb(64, 51, 22),
    warn_fg: Color32::from_rgb(240, 184, 92),
    error_bg: Color32::from_rgb(68, 32, 28),
    error_fg: Color32::from_rgb(255, 126, 114),
};

static DARK_MODE: AtomicBool = AtomicBool::new(false);

pub(crate) fn theme() -> &'static Palette {
    if dark_mode_enabled() {
        &DARK_PALETTE
    } else {
        &LIGHT_PALETTE
    }
}

pub(crate) fn dark_mode_enabled() -> bool {
    DARK_MODE.load(Ordering::Relaxed)
}

pub(crate) fn set_dark_mode(enabled: bool) {
    DARK_MODE.store(enabled, Ordering::Relaxed);
}

impl Palette {
    pub(crate) fn severity_fg(&self, severity: &DiagnosticSeverity) -> Color32 {
        match severity {
            DiagnosticSeverity::Info => self.accent,
            DiagnosticSeverity::Warning => self.warn_fg,
            DiagnosticSeverity::Error => self.error_fg,
        }
    }

    pub(crate) fn severity_bg(&self, severity: &DiagnosticSeverity) -> Color32 {
        match severity {
            DiagnosticSeverity::Info => self.selected_bg,
            DiagnosticSeverity::Warning => self.warn_bg,
            DiagnosticSeverity::Error => self.error_bg,
        }
    }
}

pub(crate) fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme().panel)
        .stroke(egui::Stroke::new(1.0, theme().border))
        .inner_margin(egui::Margin::symmetric(10, 10))
}

pub(crate) fn section_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme().surface)
        .stroke(egui::Stroke::new(1.0, theme().border))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(10, 8))
}

pub(crate) fn section_header(ui: &mut egui::Ui, title: &str, detail: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong().size(13.0).color(theme().text));
        if let Some(detail) = detail.filter(|detail| !detail.trim().is_empty()) {
            let detail_width = ui.available_width().max(0.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                truncated_label(
                    ui,
                    RichText::new(detail).small().color(theme().muted),
                    detail_width,
                    Some(detail),
                );
            });
        }
    });
    ui.add_space(4.0);
}

pub(crate) fn search_match_label(hits: usize, truncated: bool) -> String {
    let noun = if hits == 1 { "match" } else { "matches" };
    let suffix = if truncated { " · truncated" } else { "" };
    format!("{hits} {noun}{suffix}")
}

/// Grouped, single-select control rendered as one rounded pill. Each option is
/// `(value, label, enabled)`; the selected segment is filled with the accent and
/// disabled segments are muted and non-interactive. Returns `true` if the
/// selection changed this frame.
pub(crate) fn segmented_control<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    current: &mut T,
    options: &[(T, &str, bool)],
) -> bool {
    let mut changed = false;
    egui::Frame::new()
        .fill(theme().chip_bg)
        .stroke(egui::Stroke::new(1.0, theme().border))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(3.0, 0.0);
                ui.spacing_mut().button_padding = egui::vec2(11.0, 3.0);
                for (value, label, enabled) in options {
                    let selected = *current == *value;
                    let fg = if selected {
                        Color32::WHITE
                    } else if *enabled {
                        theme().text
                    } else {
                        theme().muted
                    };
                    let fill = if selected {
                        theme().accent
                    } else {
                        Color32::TRANSPARENT
                    };
                    let button = egui::Button::new(RichText::new(*label).size(12.0).color(fg))
                        .fill(fill)
                        .stroke(egui::Stroke::NONE)
                        .corner_radius(4);
                    if ui.add_enabled(*enabled, button).clicked() && !selected {
                        *current = *value;
                        changed = true;
                    }
                }
            });
        });
    changed
}

pub(crate) fn truncated_label(
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

pub(crate) fn truncated_monospace(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    let text = text.into();
    let width = ui.available_width().max(24.0);
    let response = ui.add_sized(
        egui::vec2(width, ui.text_style_height(&TextStyle::Monospace)),
        egui::Label::new(dense_monospace_text(text.as_str())).truncate(),
    );
    response.on_hover_text(text)
}

pub(crate) fn dense_label(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.label(RichText::new(text.into()).size(DENSE_ROW_FONT_SIZE))
}

pub(crate) fn dense_monospace_text(text: impl Into<String>) -> RichText {
    RichText::new(text.into())
        .monospace()
        .size(DENSE_ROW_FONT_SIZE)
}

pub(crate) fn mono_font_id(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("pdbg-mono".into()))
}
