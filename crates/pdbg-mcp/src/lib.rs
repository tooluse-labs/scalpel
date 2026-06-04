use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Allowlist {
    roots: Vec<PathBuf>,
}

impl Allowlist {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    pub fn contains_canonical_path(&self, path: &Path) -> bool {
        self.roots
            .iter()
            .any(|root| path.starts_with(root) && path != root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_requires_descendant() {
        let allowlist = Allowlist::new(vec![PathBuf::from("/tmp/root")]);
        assert!(allowlist.contains_canonical_path(Path::new("/tmp/root/a.pdf")));
        assert!(!allowlist.contains_canonical_path(Path::new("/tmp/root")));
        assert!(!allowlist.contains_canonical_path(Path::new("/tmp/root-evil/a.pdf")));
    }
}
