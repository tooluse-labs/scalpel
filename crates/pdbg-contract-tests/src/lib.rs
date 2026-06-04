#[cfg(test)]
mod tests {
    use pdbg_core::{DiagnosticCode, FakeShim, ResourceGroup, Shim};

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
}
