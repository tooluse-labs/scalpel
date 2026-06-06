use crate::dto::{CancellationCapability, MuPdfCapabilities};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityFeature {
    InspectStructure,
    RawStreams,
    DecodedStreams,
    RenderPages,
    ExtractText,
    ExtractPositionedText,
    Ocr,
    IncrementalSections,
    RepairDiagnostics,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityDecision {
    Enabled,
    Unsupported { reason: &'static str },
}

impl MuPdfCapabilities {
    pub fn mupdf_only_default() -> Self {
        Self {
            can_inspect_structure: true,
            can_load_raw_streams: true,
            can_load_decoded_streams: true,
            can_render_pages: true,
            can_extract_text: true,
            can_extract_positioned_text: true,
            can_ocr: false,
            can_list_incremental_sections: true,
            can_report_repair_diagnostics: true,
            cancellation: CancellationCapability::CooperativeDuringOperation,
        }
    }

    pub fn gate(&self, feature: CapabilityFeature) -> CapabilityDecision {
        let (enabled, reason) = match feature {
            CapabilityFeature::InspectStructure => (
                self.can_inspect_structure,
                "structure inspection is unavailable",
            ),
            CapabilityFeature::RawStreams => (
                self.can_load_raw_streams,
                "raw stream loading is unavailable",
            ),
            CapabilityFeature::DecodedStreams => (
                self.can_load_decoded_streams,
                "decoded stream loading is unavailable",
            ),
            CapabilityFeature::RenderPages => {
                (self.can_render_pages, "page rendering is unavailable")
            }
            CapabilityFeature::ExtractText => {
                (self.can_extract_text, "text extraction is unavailable")
            }
            CapabilityFeature::ExtractPositionedText => (
                self.can_extract_positioned_text,
                "positioned text extraction is unavailable",
            ),
            CapabilityFeature::Ocr => (self.can_ocr, "OCR is disabled or unavailable"),
            CapabilityFeature::IncrementalSections => (
                self.can_list_incremental_sections,
                "incremental section listing is unavailable",
            ),
            CapabilityFeature::RepairDiagnostics => (
                self.can_report_repair_diagnostics,
                "repair diagnostics are unavailable",
            ),
        };

        if enabled {
            CapabilityDecision::Enabled
        } else {
            CapabilityDecision::Unsupported { reason }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mupdf_default_exposes_debugger_surface_but_not_ocr_by_default() {
        let capabilities = MuPdfCapabilities::mupdf_only_default();
        assert_eq!(
            capabilities.gate(CapabilityFeature::RenderPages),
            CapabilityDecision::Enabled
        );
        assert_eq!(
            capabilities.gate(CapabilityFeature::ExtractText),
            CapabilityDecision::Enabled
        );
        assert_eq!(
            capabilities.gate(CapabilityFeature::ExtractPositionedText),
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
