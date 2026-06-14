use crate::{
    diagnostics_payload_to_json_string, escape_pdf_text, DiagnosticSeverity, DiagnosticSummary,
    DocumentSummary, EgressFormat, EscapedText, ObjectDetail, ObjectSearchResult, TextSearchResult,
};

const REPORT_FIELD_LIMIT_BYTES: usize = 4096;

#[derive(Clone, Debug, Default)]
pub struct DiagnosticFilter {
    pub min_severity: Option<DiagnosticSeverity>,
    pub code_query: Option<String>,
}

impl DiagnosticFilter {
    pub fn matches(&self, diagnostic: &DiagnosticSummary) -> bool {
        if let Some(min_severity) = &self.min_severity {
            if severity_rank(&diagnostic.severity) < severity_rank(min_severity) {
                return false;
            }
        }
        if let Some(query) = self
            .code_query
            .as_ref()
            .map(|query| query.trim().to_ascii_lowercase())
            .filter(|query| !query.is_empty())
        {
            if !diagnostic.code.as_public_str().contains(&query) {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Debug, Default)]
pub struct DocumentDiagnostics {
    diagnostics: Vec<DiagnosticSummary>,
}

impl DocumentDiagnostics {
    pub fn new(diagnostics: Vec<DiagnosticSummary>) -> Self {
        Self { diagnostics }
    }

    pub fn all(&self) -> &[DiagnosticSummary] {
        &self.diagnostics
    }

    pub fn filtered(&self, filter: &DiagnosticFilter) -> Vec<DiagnosticSummary> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| filter.matches(diagnostic))
            .cloned()
            .collect()
    }

    pub fn filtered_json(&self, filter: &DiagnosticFilter) -> String {
        diagnostics_payload_to_json_string(&self.filtered(filter))
    }
}

pub struct MarkdownReportInput<'a> {
    pub document: Option<&'a DocumentSummary>,
    pub selected_object: Option<&'a ObjectDetail>,
    pub diagnostics: &'a [DiagnosticSummary],
    pub object_search: Option<&'a ObjectSearchResult>,
    pub text_search: Option<&'a TextSearchResult>,
    pub max_diagnostics: usize,
    pub max_object_hits: usize,
    pub max_text_hits: usize,
    pub max_bytes: usize,
}

pub fn build_markdown_report(input: &MarkdownReportInput<'_>) -> EscapedText {
    let mut out = String::new();
    out.push_str("# PDF Diagnostic Report\n\n");
    push_document_summary(&mut out, input.document);
    push_selected_object(&mut out, input.selected_object);
    push_diagnostics(&mut out, input.diagnostics, input.max_diagnostics);
    push_object_search(&mut out, input.object_search, input.max_object_hits);
    push_text_search(&mut out, input.text_search, input.max_text_hits);
    bounded_markdown(out, input.max_bytes)
}

fn push_document_summary(out: &mut String, document: Option<&DocumentSummary>) {
    out.push_str("## Summary\n\n");
    let Some(document) = document else {
        out.push_str("- document: unavailable\n\n");
        return;
    };

    out.push_str("- file: ");
    out.push_str(&markdown_field(&document.file_path));
    out.push('\n');
    out.push_str("- version: ");
    out.push_str(&markdown_field(
        document.pdf_version.as_deref().unwrap_or("-"),
    ));
    out.push('\n');
    out.push_str(&format!(
        "- pages: {}\n- xref size: {}\n- encrypted: {}\n- repaired_or_damaged: {}\n\n",
        document.page_count,
        document.xref_size,
        document.encrypted,
        document.safety.repaired_or_damaged
    ));
}

fn push_selected_object(out: &mut String, selected_object: Option<&ObjectDetail>) {
    out.push_str("## Selected Object\n\n");
    let Some(object) = selected_object else {
        out.push_str("- object: none\n\n");
        return;
    };

    out.push_str("- label: ");
    out.push_str(&markdown_field(&object.label));
    out.push('\n');
    if let Some(object_id) = object.object {
        out.push_str(&format!("- ref: {} {} R\n", object_id.num, object_id.gen));
    }
    out.push_str("- preview: ");
    out.push_str(&markdown_field(&object.preview));
    out.push_str("\n\n");
}

fn push_diagnostics(out: &mut String, diagnostics: &[DiagnosticSummary], max_diagnostics: usize) {
    out.push_str("## Diagnostics\n\n");
    if diagnostics.is_empty() {
        out.push_str("- none\n\n");
        return;
    }

    let limit = max_diagnostics.max(1);
    for diagnostic in diagnostics.iter().take(limit) {
        out.push_str(&format!(
            "- `{}` `{}` ",
            diagnostic.severity.as_public_str(),
            diagnostic.code.as_public_str()
        ));
        out.push_str(&markdown_field(&diagnostic.message));
        if let Some(page_index) = diagnostic.page_index {
            out.push_str(&format!(" page={}", page_index + 1));
        }
        if let Some(object) = diagnostic.object {
            out.push_str(&format!(" object={} {} R", object.num, object.gen));
        }
        out.push('\n');
    }
    if diagnostics.len() > limit {
        out.push_str(&format!(
            "- truncated: {} more diagnostics\n",
            diagnostics.len() - limit
        ));
    }
    out.push('\n');
}

fn push_object_search(
    out: &mut String,
    object_search: Option<&ObjectSearchResult>,
    max_object_hits: usize,
) {
    out.push_str("## Object Search Hits\n\n");
    let Some(result) = object_search else {
        out.push_str("- none\n\n");
        return;
    };
    if result.hits.is_empty() {
        out.push_str("- none\n\n");
        return;
    }

    let limit = max_object_hits.max(1);
    for hit in result.hits.iter().take(limit) {
        out.push_str("- ");
        if let Some(object) = hit.object {
            out.push_str(&format!("{} {} R ", object.num, object.gen));
        }
        out.push('`');
        out.push_str(object_search_field_label(hit.matched_field));
        out.push_str("` ");
        out.push_str(&markdown_field(&hit.excerpt));
        out.push('\n');
    }
    if result.hits.len() > limit {
        out.push_str(&format!(
            "- truncated: {} more hits\n",
            result.hits.len() - limit
        ));
    }
    out.push('\n');
}

fn push_text_search(
    out: &mut String,
    text_search: Option<&TextSearchResult>,
    max_text_hits: usize,
) {
    out.push_str("## Text Search Hits\n\n");
    let Some(result) = text_search else {
        out.push_str("- none\n\n");
        return;
    };
    if result.hits.is_empty() {
        out.push_str("- none\n\n");
        return;
    }

    let limit = max_text_hits.max(1);
    for hit in result.hits.iter().take(limit) {
        out.push_str(&format!(
            "- page={} span={} ",
            hit.page_index + 1,
            hit.span_index
        ));
        out.push_str(&markdown_field(&hit.excerpt));
        if hit.untrusted {
            out.push_str(" `untrusted`");
        }
        out.push('\n');
    }
    if result.hits.len() > limit {
        out.push_str(&format!(
            "- truncated: {} more hits\n",
            result.hits.len() - limit
        ));
    }
    out.push('\n');
}

fn markdown_field(value: &str) -> String {
    escape_pdf_text(value, EgressFormat::Markdown, REPORT_FIELD_LIMIT_BYTES).text
}

fn bounded_markdown(markdown: String, max_bytes: usize) -> EscapedText {
    let max_bytes = max_bytes.max(1);
    if markdown.len() <= max_bytes {
        return EscapedText {
            text: markdown,
            truncated: false,
        };
    }

    let mut end = max_bytes;
    while end > 0 && !markdown.is_char_boundary(end) {
        end -= 1;
    }
    EscapedText {
        text: markdown[..end].to_string(),
        truncated: true,
    }
}

fn severity_rank(severity: &DiagnosticSeverity) -> u8 {
    match severity {
        DiagnosticSeverity::Info => 0,
        DiagnosticSeverity::Warning => 1,
        DiagnosticSeverity::Error => 2,
    }
}

fn object_search_field_label(field: crate::ObjectSearchField) -> &'static str {
    match field {
        crate::ObjectSearchField::ObjectNumber => "object",
        crate::ObjectSearchField::DictionaryKey => "key",
        crate::ObjectSearchField::NameObject => "name",
        crate::ObjectSearchField::ScalarPreview => "scalar",
        crate::ObjectSearchField::Label => "label",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DiagnosticCode, DocumentId, DocumentPermissions, DocumentSafetyState, NodeId, ObjectId,
    };

    #[test]
    fn diagnostic_filter_applies_severity_and_code_query() {
        let diagnostics = DocumentDiagnostics::new(vec![
            diagnostic(DiagnosticSeverity::Info, DiagnosticCode::JavaScriptDisabled),
            diagnostic(DiagnosticSeverity::Warning, DiagnosticCode::RepairWarning),
            diagnostic(
                DiagnosticSeverity::Error,
                DiagnosticCode::StreamDecodeFailure,
            ),
        ]);
        let filter = DiagnosticFilter {
            min_severity: Some(DiagnosticSeverity::Warning),
            code_query: Some("stream".to_string()),
        };

        let filtered = diagnostics.filtered(&filter);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].code, DiagnosticCode::StreamDecodeFailure);
        assert!(diagnostics
            .filtered_json(&filter)
            .contains("\"diagnostic_schema_version\":1"));
    }

    #[test]
    fn markdown_report_escapes_pdf_text_and_bounds_output() {
        let summary = DocumentSummary {
            doc: DocumentId(1),
            file_path: "bad *file*.pdf".to_string(),
            file_hash: None,
            pdf_version: Some("1.7".to_string()),
            page_count: 2,
            xref_size: 8,
            parsed_object_count: None,
            encrypted: false,
            needs_password: false,
            permissions: DocumentPermissions {
                print: true,
                modify: false,
                copy: true,
                annotate: false,
                fill_forms: false,
                extract_accessibility: true,
                assemble: false,
                high_quality_print: true,
            },
            metadata_summary: Vec::new(),
            safety: DocumentSafetyState {
                safe_mode: true,
                javascript_disabled: true,
                repaired_or_damaged: true,
                embedded_files_detected: false,
                external_references_detected: false,
                ocr_enabled: false,
            },
            diagnostics: Vec::new(),
        };
        let diagnostics = vec![DiagnosticSummary {
            severity: DiagnosticSeverity::Warning,
            code: DiagnosticCode::RepairWarning,
            message: "xref *repaired*".to_string(),
            node: Some(NodeId::Page {
                doc: DocumentId(1),
                index: 0,
            }),
            page_index: Some(0),
            object: Some(ObjectId { num: 4, gen: 0 }),
        }];

        let report = build_markdown_report(&MarkdownReportInput {
            document: Some(&summary),
            selected_object: None,
            diagnostics: &diagnostics,
            object_search: None,
            text_search: None,
            max_diagnostics: 8,
            max_object_hits: 8,
            max_text_hits: 8,
            max_bytes: 256,
        });

        assert!(report.text.contains("bad \\*file\\*\\.pdf"));
        assert!(report.text.contains("xref \\*repaired\\*"));
        assert!(report.truncated);
    }

    fn diagnostic(severity: DiagnosticSeverity, code: DiagnosticCode) -> DiagnosticSummary {
        DiagnosticSummary {
            severity,
            code,
            message: code.as_public_str().to_string(),
            node: None,
            page_index: None,
            object: None,
        }
    }
}
