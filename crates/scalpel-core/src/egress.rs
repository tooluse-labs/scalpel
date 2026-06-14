#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EgressFormat {
    PlainText,
    Markdown,
    Html,
    JsonString,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscapedText {
    pub text: String,
    pub truncated: bool,
}

pub fn escape_pdf_text(input: &str, format: EgressFormat, max_bytes: usize) -> EscapedText {
    let (bounded, truncated) = bounded_prefix(input, max_bytes);
    let text = match format {
        EgressFormat::PlainText => escape_plain_text(bounded),
        EgressFormat::Markdown => escape_markdown(bounded),
        EgressFormat::Html => escape_html(bounded),
        EgressFormat::JsonString => escape_json_string(bounded),
    };
    EscapedText { text, truncated }
}

fn bounded_prefix(input: &str, max_bytes: usize) -> (&str, bool) {
    if input.len() <= max_bytes {
        return (input, false);
    }

    let mut end = 0;
    for (index, ch) in input.char_indices() {
        let next = index + ch.len_utf8();
        if next > max_bytes {
            break;
        }
        end = next;
    }
    (&input[..end], true)
}

pub(crate) fn append_json_string(out: &mut String, input: &str) {
    out.push('"');
    append_json_string_content(out, input);
    out.push('"');
}

pub(crate) fn append_bounded_json_string(out: &mut String, input: &str, max_bytes: usize) -> bool {
    let (bounded, truncated) = bounded_prefix(input, max_bytes);
    append_json_string(out, bounded);
    truncated
}

fn escape_plain_text(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            '\n' | '\r' | '\t' => ch,
            ch if should_neutralize_for_display(ch) => '\u{fffd}',
            ch => ch,
        })
        .collect()
}

fn escape_markdown(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.'
            | '!' | '|' | '>' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' | '\r' | '\t' => out.push(ch),
            ch if should_neutralize_for_display(ch) => out.push('\u{fffd}'),
            ch => out.push(ch),
        }
    }
    out
}

fn escape_html(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\n' | '\r' | '\t' => out.push(ch),
            ch if should_neutralize_for_display(ch) => out.push('\u{fffd}'),
            ch => out.push(ch),
        }
    }
    out
}

fn escape_json_string(input: &str) -> String {
    let mut out = String::new();
    append_json_string(&mut out, input);
    out
}

fn append_json_string_content(out: &mut String, input: &str) {
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if should_escape_json_control(ch) => {
                out.push_str("\\u");
                out.push_str(&format!("{:04x}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
}

fn should_neutralize_for_display(ch: char) -> bool {
    matches!(
        ch,
        '\u{007f}'
            | '\u{0080}'..='\u{009f}'
            | '\u{061c}'
            | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{feff}'
    ) || ch < ' '
}

fn should_escape_json_control(ch: char) -> bool {
    should_neutralize_for_display(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markdown_html_and_json_outputs() {
        assert_eq!(
            escape_pdf_text("A* [B](x)", EgressFormat::Markdown, 100).text,
            "A\\* \\[B\\]\\(x\\)"
        );
        assert_eq!(
            escape_pdf_text("<script>&\"'", EgressFormat::Html, 100).text,
            "&lt;script&gt;&amp;&quot;&#39;"
        );
        assert_eq!(
            escape_pdf_text("A\"B\n", EgressFormat::JsonString, 100).text,
            "\"A\\\"B\\n\""
        );
    }

    #[test]
    fn bounds_without_splitting_utf8() {
        let out = escape_pdf_text("abé", EgressFormat::PlainText, 3);
        assert_eq!(out.text, "ab");
        assert!(out.truncated);
    }

    #[test]
    fn neutralizes_bidi_zero_width_and_line_separator_controls() {
        assert_eq!(
            escape_pdf_text("A\u{202e}B\u{200b}C\u{2028}", EgressFormat::Markdown, 100).text,
            format!("A{}B{}C{}", '\u{fffd}', '\u{fffd}', '\u{fffd}')
        );
        assert_eq!(
            escape_pdf_text("A\u{202e}\u{2028}", EgressFormat::JsonString, 100).text,
            "\"A\\u202e\\u2028\""
        );
        assert_eq!(
            escape_pdf_text("A\u{0085}B", EgressFormat::PlainText, 100).text,
            format!("A{}B", '\u{fffd}')
        );
        assert_eq!(
            escape_pdf_text("A\u{0085}B", EgressFormat::JsonString, 100).text,
            "\"A\\u0085B\""
        );
    }
}
