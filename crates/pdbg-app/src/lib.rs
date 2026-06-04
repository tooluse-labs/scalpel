use pdbg_core::{DocumentSummary, FakeShim, Shim, ShimError};

pub struct AppState {
    pub summary: Option<DocumentSummary>,
}

impl AppState {
    pub fn new_headless() -> Result<Self, ShimError> {
        let shim = FakeShim::new()?;
        Ok(Self {
            summary: Some(shim.open_document_summary("fake.pdf")?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_app_state_smoke() {
        let state = AppState::new_headless().unwrap();
        assert!(state.summary.is_some());
    }
}
