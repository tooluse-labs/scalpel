use super::*;

pub(crate) fn draw_stream_summary_grid(
    ui: &mut egui::Ui,
    stream: &StreamSummary,
    decoded_size_fallback: Option<u64>,
) {
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
            if stream.filters.is_empty() {
                summary_empty_value(ui);
            } else {
                ui.label(dense_monospace_text(stream.filters.join(", ")));
            }
            ui.end_row();
            dense_label(ui, "raw size");
            summary_optional_size(ui, stream.raw_size_hint);
            ui.end_row();
            dense_label(ui, "decoded size");
            match stream.decoded_size_hint.or(decoded_size_fallback) {
                Some(size) => {
                    ui.label(dense_monospace_text(size.to_string()));
                }
                None if stream.can_decode => {
                    ui.label(
                        RichText::new("known after decode")
                            .font(mono_font_id(DENSE_ROW_FONT_SIZE))
                            .color(theme().muted),
                    );
                }
                None => summary_empty_value(ui),
            }
            ui.end_row();
            dense_label(ui, "can decode");
            summary_flag(ui, stream.can_decode);
            ui.end_row();
            dense_label(ui, "image preview");
            summary_flag(ui, stream.image_preview_available);
            ui.end_row();
        });
}

pub(crate) fn summary_empty_value(ui: &mut egui::Ui) {
    ui.label(
        RichText::new("—")
            .font(mono_font_id(DENSE_ROW_FONT_SIZE))
            .color(theme().muted),
    );
}

pub(crate) fn summary_optional_size(ui: &mut egui::Ui, value: Option<u64>) {
    match value {
        Some(value) => {
            ui.label(dense_monospace_text(value.to_string()));
        }
        None => summary_empty_value(ui),
    }
}

pub(crate) fn summary_flag(ui: &mut egui::Ui, value: bool) {
    let (text, color) = if value {
        ("yes", theme().safe)
    } else {
        ("no", theme().muted)
    };
    ui.label(
        RichText::new(text)
            .font(mono_font_id(DENSE_ROW_FONT_SIZE))
            .color(color),
    );
}

pub(crate) fn render_result_color_image(render: &RenderResult) -> Option<egui::ColorImage> {
    rgba_color_image(
        render.width,
        render.height,
        render.stride,
        &render.pixels_rgba,
    )
}

pub(crate) fn image_preview_color_image(preview: &ImagePreview) -> Option<egui::ColorImage> {
    rgba_color_image(
        preview.width,
        preview.height,
        preview.stride,
        &preview.pixels_rgba,
    )
}

pub(crate) fn rgba_color_image(
    width: u32,
    height: u32,
    stride: usize,
    pixels_rgba: &[u8],
) -> Option<egui::ColorImage> {
    let width = width as usize;
    let height = height as usize;
    if width == 0 || height == 0 || stride < width.checked_mul(4)? {
        return None;
    }
    let required = stride.checked_mul(height)?;
    if pixels_rgba.len() < required {
        return None;
    }

    let row_len = width * 4;
    let mut compact = Vec::with_capacity(row_len * height);
    for row in 0..height {
        let start = row * stride;
        compact.extend_from_slice(&pixels_rgba[start..start + row_len]);
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [width, height],
        &compact,
    ))
}

pub(crate) fn stream_mode_label(mode: StreamMode) -> &'static str {
    match mode {
        StreamMode::Raw => "raw",
        StreamMode::Decoded => "decoded",
    }
}

pub(crate) fn stream_view_mode_label(mode: StreamViewMode) -> &'static str {
    match mode {
        StreamViewMode::Hex => "Hex",
        StreamViewMode::Text => "Text",
        StreamViewMode::Bytes => "Bytes",
    }
}

pub(crate) fn real_stream_preset_label(preset: RealStreamPreset) -> &'static str {
    match preset {
        RealStreamPreset::Nice => "Formatted",
        RealStreamPreset::Raw => "Raw",
    }
}

pub(crate) fn real_stream_preset_defaults(
    preset: RealStreamPreset,
    can_decode: bool,
) -> (StreamMode, StreamViewMode) {
    match preset {
        RealStreamPreset::Nice if can_decode => (StreamMode::Decoded, StreamViewMode::Text),
        RealStreamPreset::Nice => (StreamMode::Raw, StreamViewMode::Text),
        RealStreamPreset::Raw => (StreamMode::Raw, StreamViewMode::Hex),
    }
}

pub(crate) fn real_stream_default_limit(stream: &StreamSummary, mode: StreamMode) -> usize {
    let size_hint = match mode {
        StreamMode::Raw => stream.raw_size_hint,
        StreamMode::Decoded => stream.decoded_size_hint,
    };
    size_hint
        .and_then(|size| usize::try_from(size).ok())
        .map(|size| size.clamp(1, REAL_STREAM_DEFAULT_VIEW_LIMIT_BYTES))
        .unwrap_or(REAL_STREAM_DEFAULT_VIEW_LIMIT_BYTES)
}

pub(crate) fn real_stream_chunk_has_more(chunk: &StreamChunk) -> bool {
    let loaded_end = chunk.offset.saturating_add(chunk.bytes.len() as u64);
    chunk
        .total_size
        .map(|total| loaded_end < total)
        .unwrap_or(chunk.truncated)
}

pub(crate) fn real_stream_chunks_has_more(chunks: &[StreamChunk]) -> bool {
    chunks.last().is_some_and(real_stream_chunk_has_more)
}

pub(crate) fn real_stream_loaded_label(chunks: &[StreamChunk]) -> String {
    let loaded: u64 = chunks.iter().map(|chunk| chunk.bytes.len() as u64).sum();
    let total = chunks.last().and_then(|chunk| chunk.total_size);
    match total {
        Some(total) if loaded < total => format!("{loaded} of {total} bytes"),
        Some(total) => format!("{total} bytes"),
        None if real_stream_chunks_has_more(chunks) => format!("{loaded} bytes loaded · more"),
        None => format!("{loaded} bytes"),
    }
}

pub(crate) fn real_stream_chunks_visible_text(
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

pub(crate) fn real_stream_chunks_nice_lines(chunks: &[StreamChunk]) -> Vec<NiceStreamLine> {
    chunks
        .iter()
        .flat_map(|chunk| pdf_content_stream_nice_lines(&chunk.bytes))
        .collect()
}

pub(crate) fn real_stream_nice_render_lines(
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

pub(crate) fn nice_stream_row_matches_selection(
    row: &NiceStreamRenderLine,
    selection_key: &str,
) -> bool {
    row.line_key == selection_key || row.block_key.as_deref() == Some(selection_key)
}

pub(crate) fn nice_stream_text_fragments_for_selection(
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

pub(crate) fn nice_stream_line_text_fragments(line: &str) -> Vec<String> {
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

pub(crate) fn nice_stream_do_resource_for_selection(
    rows: &[NiceStreamRenderLine],
    selection_key: &str,
) -> Option<String> {
    rows.iter()
        .filter(|row| nice_stream_row_matches_selection(row, selection_key))
        .find_map(|row| nice_stream_do_resource_name(&row.line.text))
}

pub(crate) fn nice_stream_do_resource_name(line: &str) -> Option<String> {
    let tokens = pdf_content_tokens(line);
    let operator = tokens.last()?;
    if operator != "Do" {
        return None;
    }
    let name = tokens.get(tokens.len().checked_sub(2)?)?;
    name.strip_prefix('/')
        .filter(|resource| !resource.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn push_pdf_text_fragments_from_token(token: &str, out: &mut Vec<String>) {
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

pub(crate) fn decode_pdf_literal_string_token(token: &str) -> String {
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

pub(crate) fn decode_pdf_hex_string_token(token: &str) -> String {
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

pub(crate) fn is_useful_pdf_text_fragment(text: &str) -> bool {
    normalized_text_match_key(text).chars().count() >= 2
}

pub(crate) fn dedupe_text_fragments(fragments: Vec<String>) -> Vec<String> {
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

pub(crate) fn normalized_text_match_key(text: &str) -> String {
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

pub(crate) fn nice_stream_line_key(
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

pub(crate) fn draw_nice_stream_guides(
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
            theme().accent
        } else {
            theme().border
        };
        let stroke = egui::Stroke::new(if selected { 1.5 } else { 1.0 }, color);
        let x = row_rect.left() + (*indent as f32 * NICE_STREAM_INDENT_WIDTH) + 5.0;
        draw_dashed_vertical_line(ui, x, top, bottom, stroke);
    }
}

pub(crate) fn draw_dashed_vertical_line(
    ui: &egui::Ui,
    x: f32,
    top: f32,
    bottom: f32,
    stroke: egui::Stroke,
) {
    let mut y = top;
    while y < bottom {
        let segment_bottom = (y + 3.0).min(bottom);
        ui.painter()
            .line_segment([egui::pos2(x, y), egui::pos2(x, segment_bottom)], stroke);
        y += 6.0;
    }
}

pub(crate) fn real_stream_next_offset(chunks: &[StreamChunk]) -> Option<u64> {
    chunks
        .last()
        .map(|chunk| chunk.offset.saturating_add(chunk.bytes.len() as u64))
        .filter(|offset| *offset > 0)
}

pub(crate) fn real_stream_previous_offset(chunks: &[StreamChunk], limit: usize) -> Option<u64> {
    chunks
        .first()
        .and_then(|chunk| (chunk.offset > 0).then(|| chunk.offset.saturating_sub(limit as u64)))
}

pub(crate) fn real_stream_scroll_request(
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

pub(crate) fn real_stream_visible_text(
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

pub(crate) fn pdf_content_stream_nice_text(bytes: &[u8]) -> String {
    nice_stream_lines_to_text(&pdf_content_stream_nice_lines(bytes))
}

pub(crate) fn pdf_content_stream_nice_lines(bytes: &[u8]) -> Vec<NiceStreamLine> {
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

pub(crate) fn nice_stream_lines_to_text(lines: &[NiceStreamLine]) -> String {
    let mut out = String::new();
    for line in lines {
        push_pdf_content_line(&mut out, line.indent, &line.text);
    }
    out
}

pub(crate) fn push_pdf_content_line(out: &mut String, indent: usize, line: &str) {
    for _ in 0..indent {
        out.push_str("  ");
    }
    out.push_str(line);
    out.push('\n');
}

pub(crate) fn is_pdf_content_block_open(operator: &str) -> bool {
    matches!(operator, "q" | "BT" | "BDC" | "BX")
}

pub(crate) fn is_pdf_content_block_close(operator: &str) -> bool {
    matches!(operator, "Q" | "ET" | "EMC" | "EX")
}

pub(crate) fn pdf_content_tokens(input: &str) -> Vec<String> {
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

pub(crate) fn parse_pdf_string_token(chars: &[char], start: usize) -> (String, usize) {
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

pub(crate) fn parse_balanced_pdf_token(
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

pub(crate) fn parse_pdf_atom_token(chars: &[char], start: usize) -> (String, usize) {
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

pub(crate) fn is_pdf_content_operator(operator: &str, operands: &[String]) -> bool {
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

pub(crate) fn pdf_content_instruction_line(operator: &str, operands: &[String]) -> String {
    if operands.is_empty() {
        operator.to_string()
    } else {
        format!("{} {}", compact_pdf_operands(operands), operator)
    }
}

pub(crate) fn compact_pdf_operands(operands: &[String]) -> String {
    if operands.is_empty() {
        return "-".to_string();
    }
    operands
        .iter()
        .map(|operand| compact_pdf_token(operand))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn compact_pdf_token(token: &str) -> String {
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

pub(crate) fn stream_chunk_display_text(chunk: &StreamChunk, view_mode: StreamViewMode) -> String {
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

pub(crate) fn hex_dump_row(line_offset: u64, chunk: &[u8]) -> String {
    let mut out = format!("{line_offset:08x}  ");
    for byte in chunk {
        out.push_str(&format!("{byte:02x} "));
    }
    for _ in chunk.len()..HEX_VIEW_BYTES_PER_ROW {
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
    out
}

pub(crate) fn hex_dump_bytes(base_offset: u64, bytes: &[u8]) -> String {
    let mut out = String::new();
    for (line_index, chunk) in bytes.chunks(HEX_VIEW_BYTES_PER_ROW).enumerate() {
        let line_offset = base_offset
            .saturating_add((line_index as u64).saturating_mul(HEX_VIEW_BYTES_PER_ROW as u64));
        out.push_str(&hex_dump_row(line_offset, chunk));
        out.push('\n');
    }
    out
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RealStreamPreset {
    Nice,
    Raw,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RealStreamKey {
    pub(crate) object: ObjectId,
    pub(crate) mode: StreamMode,
    pub(crate) offset: u64,
    pub(crate) limit: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct RealStreamLoadedWindow {
    pub(crate) key: RealStreamKey,
    pub(crate) chunk: StreamChunk,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NiceStreamLine {
    pub(crate) indent: usize,
    pub(crate) text: String,
    pub(crate) block_open: bool,
    pub(crate) block_close: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NiceStreamRenderLine {
    pub(crate) line: NiceStreamLine,
    pub(crate) line_key: String,
    pub(crate) block_key: Option<String>,
    pub(crate) guide_blocks: Vec<(usize, String)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InspectorTab {
    Object,
    Stream,
    Hex,
    Xref,
    Diagnostics,
}
