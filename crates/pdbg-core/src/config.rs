use pdbg_shim::raw;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairPolicy {
    Default,
    Never,
    Allow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafeModeConfig {
    pub safe_mode: bool,
    pub disable_javascript: bool,
    pub enable_ocr: bool,
    pub allow_external_references: bool,
    pub max_store_bytes: u64,
    pub max_decoded_stream_bytes: u64,
    pub max_filter_expansion_ratio: u32,
    pub max_object_depth: u32,
    pub repair_policy: RepairPolicy,
}

impl Default for SafeModeConfig {
    fn default() -> Self {
        Self {
            safe_mode: true,
            disable_javascript: true,
            enable_ocr: false,
            allow_external_references: false,
            max_store_bytes: 256 * 1024 * 1024,
            max_decoded_stream_bytes: 64 * 1024 * 1024,
            max_filter_expansion_ratio: 64,
            max_object_depth: 128,
            repair_policy: RepairPolicy::Default,
        }
    }
}

impl SafeModeConfig {
    pub fn to_raw_open_options(&self) -> raw::pdbg_open_options {
        raw::pdbg_open_options {
            safe_mode: self.safe_mode as i32,
            disable_javascript: self.disable_javascript as i32,
            enable_ocr: self.enable_ocr as i32,
            max_store_bytes: self.max_store_bytes,
            max_decoded_stream_bytes: self.max_decoded_stream_bytes,
            max_filter_expansion_ratio: self.max_filter_expansion_ratio,
            max_object_depth: self.max_object_depth,
            repair_policy: match self.repair_policy {
                RepairPolicy::Default => raw::pdbg_repair_policy::PDBG_REPAIR_DEFAULT,
                RepairPolicy::Never => raw::pdbg_repair_policy::PDBG_REPAIR_NEVER,
                RepairPolicy::Allow => raw::pdbg_repair_policy::PDBG_REPAIR_ALLOW,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_mode_defaults_match_m0_contract() {
        let config = SafeModeConfig::default();
        assert!(config.safe_mode);
        assert!(config.disable_javascript);
        assert!(!config.enable_ocr);
        assert!(!config.allow_external_references);
        assert!(config.max_decoded_stream_bytes > 0);
        assert!(config.max_filter_expansion_ratio > 0);
        assert!(config.max_object_depth > 0);

        let raw = config.to_raw_open_options();
        assert_eq!(raw.safe_mode, 1);
        assert_eq!(raw.disable_javascript, 1);
        assert_eq!(raw.enable_ocr, 0);
        assert_eq!(
            raw.repair_policy,
            raw::pdbg_repair_policy::PDBG_REPAIR_DEFAULT
        );
    }
}
