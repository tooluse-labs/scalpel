#[cfg(feature = "real-mupdf")]
use pdbg_core::RealMuPdfShim;
use pdbg_core::{
    escape_pdf_text, CapabilityDecision, CapabilityFeature, ChildContainer, ChildPage, ChildRange,
    DocumentSession, DocumentSummary, EgressFormat, EscapedText, FakeShim, MuPdfCapabilities,
    NodeId, ObjectSummary, OpenDocument, SafeModeConfig, ShimDocument, ShimError,
};

#[cfg(feature = "gui")]
pub mod gui;

pub struct AppState {
    session: DocumentSession<OpenDocument>,
    pub safe_mode: SafeModeConfig,
    pub capabilities: MuPdfCapabilities,
    pub feature_gates: Vec<FeatureGateState>,
    pub panels: PanelState,
    pub command_log: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PanelKind {
    Summary,
    StructureTree,
    ObjectDetail,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeatureGateState {
    pub feature: CapabilityFeature,
    pub decision: CapabilityDecision,
}

#[derive(Clone, Debug)]
pub struct PanelState {
    pub visible: Vec<PanelKind>,
    pub summary: Option<DocumentSummary>,
    pub tree: Option<ChildPage<ObjectSummary>>,
    pub detail_preview: Option<String>,
    pub escaped_output: Option<EscapedText>,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            visible: vec![
                PanelKind::Summary,
                PanelKind::StructureTree,
                PanelKind::ObjectDetail,
                PanelKind::Output,
            ],
            summary: None,
            tree: None,
            detail_preview: None,
            escaped_output: None,
        }
    }
}

impl AppState {
    pub fn new_headless() -> Result<Self, ShimError> {
        let safe_mode = SafeModeConfig::default();
        let capabilities = MuPdfCapabilities::mupdf_only_default();
        let shim = FakeShim::new()?;
        let doc = shim.open_document_with_config("fake.pdf", &safe_mode)?;
        let session = DocumentSession::with_shared_store(doc, shim.shared_store());
        Self::from_session(session, safe_mode, capabilities)
    }

    #[cfg(feature = "real-mupdf")]
    pub fn new_real_path(path: &str) -> Result<Self, ShimError> {
        let safe_mode = SafeModeConfig::default();
        let capabilities = MuPdfCapabilities::mupdf_only_default();
        let shim = RealMuPdfShim::new()?;
        let doc = shim.open_document_with_config(path, &safe_mode)?;
        let session = DocumentSession::new(doc);
        Self::from_session(session, safe_mode, capabilities)
    }

    fn from_session(
        session: DocumentSession<OpenDocument>,
        safe_mode: SafeModeConfig,
        capabilities: MuPdfCapabilities,
    ) -> Result<Self, ShimError> {
        let mut state = Self {
            session,
            safe_mode,
            feature_gates: feature_gates(&capabilities),
            capabilities,
            panels: PanelState::default(),
            command_log: Vec::new(),
        };
        state.run_headless_command_loop()?;
        Ok(state)
    }

    pub fn run_headless_command_loop(&mut self) -> Result<(), ShimError> {
        self.command_log.push("refresh_summary".to_string());
        let summary = self.session.summary()?;

        self.command_log.push("refresh_tree".to_string());
        let tree = if self.capabilities.gate(CapabilityFeature::InspectStructure)
            == CapabilityDecision::Enabled
        {
            let root = NodeId::DocumentRoot {
                doc: summary.doc.clone(),
            };
            Some(self.session.run_task(|document| {
                document.children(
                    &root,
                    ChildRange {
                        offset: 0,
                        limit: 16,
                    },
                    ChildContainer::Dictionary,
                )
            })?)
        } else {
            None
        };

        self.command_log.push("refresh_detail".to_string());
        let detail_preview = if let Some(first) = tree.as_ref().and_then(|tree| tree.items.first())
        {
            let detail = self.session.run_task(|document| {
                document.object_detail(
                    &first.id,
                    ChildRange {
                        offset: 0,
                        limit: 16,
                    },
                )
            })?;
            Some(detail.preview)
        } else {
            None
        };

        self.command_log.push("escape_output".to_string());
        let escaped_output = detail_preview
            .as_deref()
            .map(|preview| escape_pdf_text(preview, EgressFormat::Markdown, 4096));

        self.panels.summary = Some(summary);
        self.panels.tree = tree;
        self.panels.detail_preview = detail_preview;
        self.panels.escaped_output = escaped_output;
        Ok(())
    }
}

fn feature_gates(capabilities: &MuPdfCapabilities) -> Vec<FeatureGateState> {
    [
        CapabilityFeature::InspectStructure,
        CapabilityFeature::RenderPages,
        CapabilityFeature::ExtractText,
        CapabilityFeature::ExtractPositionedText,
        CapabilityFeature::Ocr,
    ]
    .into_iter()
    .map(|feature| FeatureGateState {
        feature,
        decision: capabilities.gate(feature),
    })
    .collect()
}

#[cfg(all(test, not(feature = "real-mupdf")))]
mod tests {
    use super::*;

    #[test]
    fn headless_app_state_smoke() {
        let state = AppState::new_headless().unwrap();
        assert_eq!(
            state.panels.visible,
            vec![
                PanelKind::Summary,
                PanelKind::StructureTree,
                PanelKind::ObjectDetail,
                PanelKind::Output,
            ]
        );
        assert!(state.safe_mode.safe_mode);
        assert!(state.safe_mode.disable_javascript);
        assert_eq!(
            state
                .feature_gates
                .iter()
                .find(|gate| gate.feature == CapabilityFeature::RenderPages)
                .unwrap()
                .decision,
            CapabilityDecision::Enabled
        );
        assert_eq!(
            state
                .feature_gates
                .iter()
                .find(|gate| gate.feature == CapabilityFeature::Ocr)
                .unwrap()
                .decision,
            CapabilityDecision::Unsupported {
                reason: "OCR is disabled or unavailable"
            }
        );
        assert_eq!(state.panels.summary.as_ref().unwrap().file_path, "fake.pdf");
        assert!(!state.panels.tree.as_ref().unwrap().items.is_empty());
        assert_eq!(
            state.panels.detail_preview.as_deref(),
            Some("<< /Type /Fake >>")
        );
        assert_eq!(
            state.panels.escaped_output.as_ref().unwrap().text,
            "<< /Type /Fake \\>\\>"
        );
        assert_eq!(
            state.command_log,
            vec![
                "refresh_summary",
                "refresh_tree",
                "refresh_detail",
                "escape_output"
            ]
        );
    }
}
