#[cfg(test)]
mod tests {
    use pdbg_core::{
        escape_pdf_text, CapabilityDecision, CapabilityFeature, DiagnosticCode, DocumentId,
        EgressFormat, FakeShim, MuPdfCapabilities, NodeId, ObjectId, ResourceGroup, SafeModeConfig,
        Shim,
    };

    #[test]
    fn workspace_contract_smoke() {
        let shim = FakeShim::new().unwrap();
        let summary = shim.open_document_summary("fake.pdf").unwrap();
        assert_eq!(summary.file_path, "fake.pdf");
        assert_eq!(
            DiagnosticCode::JavaScriptDisabled.as_public_str(),
            "javascript_disabled"
        );
        assert_eq!(ResourceGroup::XObjects.as_public_str(), "xobjects");
    }

    #[test]
    fn serialized_node_id_contract_is_stable_and_token_free() {
        let node = NodeId::Stream {
            doc: DocumentId(3),
            object: ObjectId { num: 9, gen: 2 },
            decoded: true,
        };

        let json = node.to_serialized().to_json_string();
        assert_eq!(
            json,
            "{\"schema_version\":1,\"doc\":3,\"segments\":[{\"tag\":\"stream\",\"object\":{\"num\":9,\"gen\":2},\"decoded\":true}],\"object\":{\"num\":9,\"gen\":2}}"
        );
        assert!(!json.contains("path_token"));
    }

    #[test]
    fn egress_contract_escapes_pdf_controlled_text() {
        assert_eq!(
            escape_pdf_text("<b>*PDF*</b>", EgressFormat::Html, 100).text,
            "&lt;b&gt;*PDF*&lt;/b&gt;"
        );
        assert_eq!(
            escape_pdf_text("<b>*PDF*</b>", EgressFormat::Markdown, 100).text,
            "<b\\>\\*PDF\\*</b\\>"
        );
    }

    #[test]
    fn safe_mode_and_capability_contracts_are_pinned() {
        let safe_mode = SafeModeConfig::default();
        assert!(safe_mode.safe_mode);
        assert!(safe_mode.disable_javascript);
        assert!(!safe_mode.enable_ocr);
        assert!(!safe_mode.allow_external_references);

        let capabilities = MuPdfCapabilities::mupdf_only_default();
        assert_eq!(
            capabilities.gate(CapabilityFeature::InspectStructure),
            CapabilityDecision::Enabled
        );
        assert_eq!(
            capabilities.gate(CapabilityFeature::Ocr),
            CapabilityDecision::Unsupported {
                reason: "OCR is disabled or unavailable"
            }
        );
    }
}
