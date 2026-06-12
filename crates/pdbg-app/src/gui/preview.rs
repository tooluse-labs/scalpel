use super::*;

pub(crate) fn page_preview_display_size(
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

pub(crate) fn page_preview_leading_space(available_width: f32, display_width: f32) -> f32 {
    ((available_width - display_width) * 0.5).max(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PreviewControlsLayout {
    pub(crate) stacked: bool,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) pos: egui::Pos2,
}

pub(crate) fn preview_controls_overlay_layout(content_rect: egui::Rect) -> PreviewControlsLayout {
    let margin = 8.0;
    let total_width =
        PREVIEW_ZOOM_CONTROL_WIDTH + PREVIEW_CONTROL_GAP + PREVIEW_PAGER_CONTROL_WIDTH;
    // Stack the two groups vertically when the preview column is too narrow
    // for the single-row layout.
    let stacked = content_rect.width() < total_width + 2.0 * margin;
    let width = if stacked {
        PREVIEW_ZOOM_CONTROL_WIDTH
    } else {
        total_width
    };
    let height = if stacked {
        2.0 * PREVIEW_CONTROL_GROUP_HEIGHT + 8.0
    } else {
        PREVIEW_CONTROL_GROUP_HEIGHT
    };
    let min_x = content_rect.left() + margin;
    let max_x = (content_rect.right() - width - margin).max(min_x);
    let pos = egui::pos2(
        (content_rect.center().x - width * 0.5).clamp(min_x, max_x),
        content_rect.bottom() - height - 14.0,
    );
    PreviewControlsLayout {
        stacked,
        width,
        height,
        pos,
    }
}

pub(crate) fn previous_render_zoom(current: f32) -> Option<f32> {
    RENDER_ZOOM_LEVELS
        .iter()
        .rev()
        .copied()
        .find(|zoom| *zoom < current - f32::EPSILON)
}

pub(crate) fn next_render_zoom(current: f32) -> Option<f32> {
    RENDER_ZOOM_LEVELS
        .iter()
        .copied()
        .find(|zoom| *zoom > current + f32::EPSILON)
}

pub(crate) fn next_render_rotation(current: i32) -> i32 {
    match current.rem_euclid(360) {
        0 => 90,
        90 => 180,
        180 => 270,
        _ => 0,
    }
}

pub(crate) fn render_max_dimension_or_default(value: Option<u32>) -> u32 {
    value
        .filter(|dimension| *dimension > 0)
        .unwrap_or(DEFAULT_RENDER_MAX_DIMENSION)
}

pub(crate) fn render_max_pixels(max_dimension: u32) -> u64 {
    u64::from(max_dimension).saturating_mul(u64::from(max_dimension))
}

pub(crate) fn render_max_output_bytes(max_dimension: u32) -> u64 {
    render_max_pixels(max_dimension)
        .saturating_mul(4)
        .max(DEFAULT_RENDER_MAX_OUTPUT_BYTES)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum PreviewControlIcon {
    ZoomIn,
    ZoomOut,
    PreviousPage,
    NextPage,
    RotateRight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageKeyboardShortcut {
    Previous,
    Next,
    First,
    Last,
}

pub(crate) fn page_keyboard_target_page(
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

pub(crate) fn preview_scroll_target_page(
    current_page: usize,
    page_count: usize,
    accumulator: &mut egui::Vec2,
    delta: egui::Vec2,
) -> Option<usize> {
    if page_count == 0 {
        *accumulator = egui::Vec2::ZERO;
        return None;
    }
    if !delta.x.is_finite() || !delta.y.is_finite() {
        *accumulator = egui::Vec2::ZERO;
        return None;
    }
    if delta.x.abs() < f32::EPSILON && delta.y.abs() < f32::EPSILON {
        return None;
    }

    let page_delta = if delta.x.abs() > delta.y.abs() {
        accumulator.x += delta.x;
        accumulator.y = 0.0;
        page_delta_from_scroll_accumulator(&mut accumulator.x)
    } else {
        accumulator.y += delta.y;
        accumulator.x = 0.0;
        page_delta_from_scroll_accumulator(&mut accumulator.y)
    };
    if page_delta == 0 {
        return None;
    }

    let last_page = page_count - 1;
    Some(if page_delta > 0 {
        current_page
            .saturating_add(page_delta as usize)
            .min(last_page)
    } else {
        current_page.saturating_sub(page_delta.unsigned_abs() as usize)
    })
}

fn page_delta_from_scroll_accumulator(accumulator: &mut f32) -> i32 {
    let steps = (*accumulator / PREVIEW_PAGE_SCROLL_THRESHOLD)
        .abs()
        .floor()
        .min(PREVIEW_PAGE_SCROLL_MAX_STEPS as f32) as i32;
    if steps == 0 {
        return 0;
    }

    let sign = accumulator.signum();
    *accumulator -= sign * PREVIEW_PAGE_SCROLL_THRESHOLD * steps as f32;
    if sign < 0.0 {
        steps
    } else {
        -steps
    }
}

pub(crate) fn preview_icon_button(
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

pub(crate) fn preview_control_group<R>(
    ui: &mut egui::Ui,
    width: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let size = egui::vec2(width, PREVIEW_CONTROL_GROUP_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, 16.0, theme().chip_bg);
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

pub(crate) fn preview_control_separator(ui: &mut egui::Ui) {
    let height = 26.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, height), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().line_segment(
            [rect.center_top(), rect.center_bottom()],
            egui::Stroke::new(1.0, theme().border),
        );
    }
}

pub(crate) fn draw_preview_control_icon(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    icon: PreviewControlIcon,
    enabled: bool,
) {
    let color = if enabled { theme().text } else { theme().muted };
    let stroke = egui::Stroke::new(1.5, color);
    match icon {
        PreviewControlIcon::ZoomIn => draw_plus_minus_icon(ui, rect, true, stroke),
        PreviewControlIcon::ZoomOut => draw_plus_minus_icon(ui, rect, false, stroke),
        PreviewControlIcon::PreviousPage => draw_chevron_icon(ui, rect, false, stroke),
        PreviewControlIcon::NextPage => draw_chevron_icon(ui, rect, true, stroke),
        PreviewControlIcon::RotateRight => draw_rotate_icon(ui, rect, stroke),
    }
}

pub(crate) fn draw_plus_minus_icon(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    plus: bool,
    stroke: egui::Stroke,
) {
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

pub(crate) fn draw_chevron_icon(
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

pub(crate) fn draw_rotate_icon(ui: &mut egui::Ui, rect: egui::Rect, stroke: egui::Stroke) {
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
#[derive(Clone, Copy, Debug)]
pub(crate) struct PagePreviewClick {
    pub(crate) page_index: usize,
    pub(crate) render_x: f32,
    pub(crate) render_y: f32,
    pub(crate) normalized_x: f32,
    pub(crate) normalized_y: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct PreviewVisualHit {
    pub(crate) page_index: usize,
    pub(crate) element_index: usize,
    pub(crate) kind: VisualElementKind,
    pub(crate) bbox: PageRect,
    pub(crate) object: Option<ObjectId>,
    pub(crate) untrusted: bool,
    pub(crate) contains_click: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RealVisualTarget {
    pub(crate) page_index: usize,
    pub(crate) object: ObjectId,
    pub(crate) allow_page_union: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingPreviewStreamSelection {
    pub(crate) page_index: usize,
    pub(crate) text_hit: Option<TextSearchHit>,
    pub(crate) visual_hit: Option<PreviewVisualHit>,
}

#[derive(Clone, Debug)]
pub(crate) struct VisualPageCache {
    pub(crate) entries: VecDeque<VisualPageCacheEntry>,
    pub(crate) max_pages: usize,
    pub(crate) max_elements: usize,
    pub(crate) current_elements: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct VisualPageCacheEntry {
    pub(crate) page: VisualPage,
    pub(crate) element_count: usize,
}

impl VisualPageCache {
    pub(crate) fn new(max_pages: usize, max_elements: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_pages: max_pages.max(1),
            max_elements: max_elements.max(1),
            current_elements: 0,
        }
    }

    pub(crate) fn get(&mut self, page_index: usize) -> Option<VisualPage> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.page.page_index == page_index)?;
        let entry = self.entries.remove(index)?;
        let page = entry.page.clone();
        self.entries.push_back(entry);
        Some(page)
    }

    pub(crate) fn insert(&mut self, page: VisualPage) {
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

    pub(crate) fn evict_to_budget(&mut self) {
        while self.entries.len() > self.max_pages || self.current_elements > self.max_elements {
            let Some(entry) = self.entries.pop_front() else {
                break;
            };
            self.current_elements = self.current_elements.saturating_sub(entry.element_count);
        }
    }
}

pub(crate) fn preview_click_from_pos(
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

pub(crate) fn text_hit_from_page_click(
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

pub(crate) fn text_hit_for_text_fragments(
    page: &TextPage,
    fragments: &[String],
) -> Option<TextSearchHit> {
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

pub(crate) fn text_span_indices_for_fragment(page: &TextPage, fragment: &str) -> Vec<usize> {
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

pub(crate) fn visual_hit_from_page_click(
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

pub(crate) fn visual_hit_for_object(
    page: &VisualPage,
    object: ObjectId,
) -> Option<PreviewVisualHit> {
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

pub(crate) fn visual_hit_for_page_visual_union(
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

pub(crate) fn visual_hit_for_element_indices(
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

pub(crate) fn nice_stream_visual_hit_for_selection(
    page: &VisualPage,
    rows: &[NiceStreamRenderLine],
    selection_key: &str,
    object: ObjectId,
    page_height: Option<f32>,
) -> Option<PreviewVisualHit> {
    if let Some(hit) = visual_hit_for_object(page, object) {
        return Some(hit);
    }

    if let Some(content_bbox) = nice_stream_image_bbox_for_selection(rows, selection_key) {
        let (bbox, element_index) =
            best_page_bbox_for_content_bbox(page, &content_bbox, page_height);
        return Some(PreviewVisualHit {
            page_index: page.page_index,
            element_index: element_index.unwrap_or(0),
            kind: VisualElementKind::Image,
            bbox,
            object: Some(object),
            untrusted: true,
            contains_click: false,
        });
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

pub(crate) fn nice_stream_selection_key_for_text_hit(
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

pub(crate) fn nice_stream_selection_key_for_visual_hit(
    page: &VisualPage,
    rows: &[NiceStreamRenderLine],
    hit: &PreviewVisualHit,
    page_height: Option<f32>,
) -> Option<String> {
    let element = page.elements.get(hit.element_index)?;
    if matches!(
        element.kind,
        VisualElementKind::Image | VisualElementKind::Unknown
    ) {
        if let Some(key) = nice_stream_image_selection_key_for_bbox(rows, &hit.bbox, page_height) {
            return Some(key);
        }
    }

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

pub(crate) fn nice_stream_image_selection_key_for_bbox(
    rows: &[NiceStreamRenderLine],
    bbox: &PageRect,
    page_height: Option<f32>,
) -> Option<String> {
    nice_stream_image_bboxes(rows)
        .into_iter()
        .filter_map(|(row_index, content_bbox)| {
            page_bbox_candidates_for_content_bbox(&content_bbox, page_height)
                .into_iter()
                .map(|image_bbox| {
                    (
                        rect_intersection_area(&image_bbox, bbox),
                        rect_area(&image_bbox),
                        row_index,
                    )
                })
                .filter(|(area, _, _)| *area > 0.0)
                .max_by(|left, right| {
                    left.0
                        .partial_cmp(&right.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| {
                            right
                                .1
                                .partial_cmp(&left.1)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                })
        })
        .max_by(|left, right| {
            left.0
                .partial_cmp(&right.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    right
                        .1
                        .partial_cmp(&left.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        })
        .and_then(|(_, _, row_index)| rows.get(row_index).map(|row| row.line_key.clone()))
}

pub(crate) fn nice_stream_image_bbox_for_selection(
    rows: &[NiceStreamRenderLine],
    selection_key: &str,
) -> Option<PageRect> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| nice_stream_row_matches_selection(row, selection_key))
        .find_map(|(row_index, row)| {
            nice_stream_do_resource_name(&row.line.text)
                .and_then(|_| nice_stream_image_bbox_before_row(rows, row_index))
        })
}

pub(crate) fn nice_stream_image_bboxes(rows: &[NiceStreamRenderLine]) -> Vec<(usize, PageRect)> {
    rows.iter()
        .enumerate()
        .filter_map(|(row_index, row)| {
            nice_stream_do_resource_name(&row.line.text)
                .and_then(|_| nice_stream_image_bbox_before_row(rows, row_index))
                .map(|bbox| (row_index, bbox))
        })
        .collect()
}

pub(crate) fn nice_stream_image_bbox_before_row(
    rows: &[NiceStreamRenderLine],
    row_index: usize,
) -> Option<PageRect> {
    let current_indent = rows.get(row_index)?.line.indent;
    rows[..row_index]
        .iter()
        .rev()
        .take_while(|row| row.line.indent >= current_indent)
        .find_map(|row| nice_stream_image_bbox_from_cm_line(&row.line.text))
}

pub(crate) fn nice_stream_image_bbox_from_cm_line(line: &str) -> Option<PageRect> {
    let tokens = pdf_content_tokens(line);
    if tokens.last().map(String::as_str) != Some("cm") || tokens.len() < 7 {
        return None;
    }
    let values = tokens[tokens.len() - 7..tokens.len() - 1]
        .iter()
        .map(|token| token.parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let matrix: [f32; 6] = values.try_into().ok()?;
    let points = [
        transform_unit_point(matrix, 0.0, 0.0),
        transform_unit_point(matrix, 1.0, 0.0),
        transform_unit_point(matrix, 0.0, 1.0),
        transform_unit_point(matrix, 1.0, 1.0),
    ];
    let min_x = points.iter().map(|(x, _)| *x).fold(f32::INFINITY, f32::min);
    let max_x = points
        .iter()
        .map(|(x, _)| *x)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = points.iter().map(|(_, y)| *y).fold(f32::INFINITY, f32::min);
    let max_y = points
        .iter()
        .map(|(_, y)| *y)
        .fold(f32::NEG_INFINITY, f32::max);
    let width = max_x - min_x;
    let height = max_y - min_y;
    (width > 0.0 && height > 0.0).then_some(PageRect {
        x: min_x,
        y: min_y,
        width,
        height,
    })
}

pub(crate) fn transform_unit_point(matrix: [f32; 6], x: f32, y: f32) -> (f32, f32) {
    let [a, b, c, d, e, f] = matrix;
    (a * x + c * y + e, b * x + d * y + f)
}

pub(crate) fn best_visual_element_overlap_for_bbox(
    page: &VisualPage,
    bbox: &PageRect,
) -> Option<(usize, f32)> {
    page.elements
        .iter()
        .enumerate()
        .filter_map(|(index, element)| {
            let area = rect_intersection_area(&element.bbox, bbox);
            (area > 0.0).then_some((index, area))
        })
        .max_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

pub(crate) fn best_page_bbox_for_content_bbox(
    page: &VisualPage,
    content_bbox: &PageRect,
    page_height: Option<f32>,
) -> (PageRect, Option<usize>) {
    let mut candidates = page_bbox_candidates_for_content_bbox(content_bbox, page_height);
    if candidates.is_empty() {
        candidates.push(content_bbox.clone());
    }

    let mut best: Option<(f32, PageRect, usize)> = None;
    for candidate in &candidates {
        let Some((element_index, area)) = best_visual_element_overlap_for_bbox(page, candidate)
        else {
            continue;
        };
        let should_replace = best
            .as_ref()
            .and_then(|(best_area, _, _)| area.partial_cmp(best_area))
            .is_some_and(|ordering| ordering == std::cmp::Ordering::Greater);
        if best.is_none() || should_replace {
            best = Some((area, candidate.clone(), element_index));
        }
    }

    if let Some((_, bbox, element_index)) = best {
        (bbox, Some(element_index))
    } else {
        (candidates.remove(0), None)
    }
}

pub(crate) fn page_bbox_candidates_for_content_bbox(
    content_bbox: &PageRect,
    page_height: Option<f32>,
) -> Vec<PageRect> {
    let mut candidates = Vec::new();
    if let Some(height) = page_height.filter(|height| height.is_finite() && *height > 0.0) {
        candidates.push(flip_page_rect_y(content_bbox, height));
    }
    if !candidates
        .iter()
        .any(|candidate| page_rects_approximately_equal(candidate, content_bbox))
    {
        candidates.push(content_bbox.clone());
    }
    candidates
}

pub(crate) fn flip_page_rect_y(rect: &PageRect, page_height: f32) -> PageRect {
    PageRect {
        x: rect.x,
        y: page_height - rect.y - rect.height,
        width: rect.width,
        height: rect.height,
    }
}

pub(crate) fn page_rects_approximately_equal(a: &PageRect, b: &PageRect) -> bool {
    const EPSILON: f32 = 0.01;
    (a.x - b.x).abs() <= EPSILON
        && (a.y - b.y).abs() <= EPSILON
        && (a.width - b.width).abs() <= EPSILON
        && (a.height - b.height).abs() <= EPSILON
}

pub(crate) fn nice_stream_reverse_visual_kind_candidates(
    kind: VisualElementKind,
) -> Vec<VisualElementKind> {
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
pub(crate) struct NiceStreamVisualOp {
    pub(crate) row_index: usize,
    pub(crate) kind: VisualElementKind,
}

pub(crate) fn nice_stream_visual_ops(rows: &[NiceStreamRenderLine]) -> Vec<NiceStreamVisualOp> {
    rows.iter()
        .enumerate()
        .filter_map(|(row_index, row)| {
            nice_stream_line_visual_kind(&row.line.text)
                .map(|kind| NiceStreamVisualOp { row_index, kind })
        })
        .collect()
}

pub(crate) fn nice_stream_selection_has_non_text_visual_ops(
    rows: &[NiceStreamRenderLine],
    selection_key: &str,
) -> bool {
    rows.iter()
        .filter(|row| nice_stream_row_matches_selection(row, selection_key))
        .filter_map(|row| nice_stream_line_visual_kind(&row.line.text))
        .any(|kind| kind != VisualElementKind::Text)
}

pub(crate) fn nice_stream_selected_visual_kind_priority(
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

pub(crate) fn nice_stream_line_visual_kind(line: &str) -> Option<VisualElementKind> {
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

pub(crate) fn nice_stream_line_operator(line: &str) -> Option<String> {
    pdf_content_tokens(line).pop()
}

pub(crate) fn visual_kind_matches(expected: VisualElementKind, actual: VisualElementKind) -> bool {
    match expected {
        VisualElementKind::Unknown => true,
        VisualElementKind::Vector => {
            matches!(actual, VisualElementKind::Vector | VisualElementKind::Grid)
        }
        _ => expected == actual,
    }
}

pub(crate) fn union_page_rect_into(target: &mut PageRect, rect: &PageRect) {
    let left = target.x.min(rect.x);
    let top = target.y.min(rect.y);
    let right = (target.x + target.width).max(rect.x + rect.width);
    let bottom = (target.y + target.height).max(rect.y + rect.height);
    target.x = left;
    target.y = top;
    target.width = (right - left).max(0.0);
    target.height = (bottom - top).max(0.0);
}

pub(crate) fn page_point_from_preview_click(
    click: PagePreviewClick,
    zoom: f32,
) -> Option<(f32, f32)> {
    (zoom > 0.0).then_some((click.render_x / zoom, click.render_y / zoom))
}

pub(crate) fn rect_contains_point(rect: &PageRect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
}

pub(crate) fn rect_area(rect: &PageRect) -> f32 {
    (rect.width.max(0.0)) * (rect.height.max(0.0))
}

pub(crate) fn rect_intersection_area(a: &PageRect, b: &PageRect) -> f32 {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);
    ((right - left).max(0.0)) * ((bottom - top).max(0.0))
}

pub(crate) fn rect_distance_sq_to_point(rect: &PageRect, x: f32, y: f32) -> f32 {
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

pub(crate) fn text_span_to_hit(
    page_index: usize,
    span_index: usize,
    span: &TextSpan,
) -> TextSearchHit {
    TextSearchHit {
        page_index,
        span_index,
        excerpt: span.text.clone(),
        bbox: Some(span.bbox.clone()),
        untrusted: span.untrusted,
    }
}

pub(crate) fn visual_element_to_hit(
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
