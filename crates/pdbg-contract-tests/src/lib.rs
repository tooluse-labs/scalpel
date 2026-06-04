#[cfg(test)]
mod tests {
    use pdbg_core::{
        escape_pdf_text, CapabilityDecision, CapabilityFeature, ChildRange, DiagnosticCode,
        DocumentId, EgressFormat, FakeShim, MuPdfCapabilities, NodeId, ObjectId, RenderRequest,
        ResourceGroup, SafeModeConfig, Shim, ShimDocument, StreamMode, TextRequest,
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

    #[test]
    fn fake_shim_operation_surface_uses_c_accessors_and_registry() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();
        let summary = doc.summary().unwrap();
        let root = NodeId::DocumentRoot {
            doc: summary.doc.clone(),
        };
        let range = ChildRange {
            offset: 0,
            limit: 2,
        };

        let children = doc
            .children(&root, range, pdbg_core::ChildContainer::Dictionary)
            .unwrap();
        assert_eq!(children.total, Some(3));
        assert_eq!(children.items[0].object, Some(ObjectId { num: 1, gen: 0 }));

        let detail = doc.object_detail(&children.items[0].id, range).unwrap();
        assert_eq!(detail.stream.unwrap().filters, vec!["FlateDecode"]);
        assert_eq!(detail.dictionary_entries.unwrap().items.len(), 2);

        let stream = doc
            .stream_load(ObjectId { num: 1, gen: 0 }, StreamMode::Raw, 0, 4)
            .unwrap();
        assert_eq!(stream.bytes, b"fake");
        assert!(stream.truncated);

        let render = doc.render_page(&RenderRequest::page(0)).unwrap();
        assert_eq!(render.pixels_rgba, vec![255, 255, 255, 255]);

        let text = doc.extract_text(&TextRequest::page(0)).unwrap();
        assert_eq!(text.spans[0].text.as_bytes(), b"A\0B");
    }

    #[test]
    fn text_coordinate_normalization_golden_is_top_left_page_space() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();
        let text = doc.extract_text(&TextRequest::page(0)).unwrap();
        let span = &text.spans[0];

        assert_eq!(text.page_index, 0);
        assert_eq!(span.bbox.x, 5.0);
        assert_eq!(span.bbox.y, 7.0);
        assert_eq!(span.bbox.width, 10.0);
        assert_eq!(span.bbox.height, 12.0);
        assert!(span.untrusted);
    }
}
