use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Allowlist {
    roots: Vec<PathBuf>,
}

#[derive(Debug)]
pub enum AllowlistError {
    EmptyRoots,
    UrlLikePath(PathBuf),
    CanonicalizeRoot { root: PathBuf, source: io::Error },
    CanonicalizeRequest { path: PathBuf, source: io::Error },
    OutsideRoots { path: PathBuf },
}

impl fmt::Display for AllowlistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRoots => write!(f, "allowlist must contain at least one root"),
            Self::UrlLikePath(path) => {
                write!(f, "URL-like paths are not allowed: {}", path.display())
            }
            Self::CanonicalizeRoot { root, source } => {
                write!(
                    f,
                    "failed to canonicalize root {}: {source}",
                    root.display()
                )
            }
            Self::CanonicalizeRequest { path, source } => {
                write!(
                    f,
                    "failed to canonicalize request {}: {source}",
                    path.display()
                )
            }
            Self::OutsideRoots { path } => {
                write!(
                    f,
                    "path is outside configured allowlist roots: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for AllowlistError {}

impl Allowlist {
    pub fn try_new(roots: impl IntoIterator<Item = PathBuf>) -> Result<Self, AllowlistError> {
        let mut canonical_roots = Vec::new();
        for root in roots {
            reject_url_like(&root)?;
            let canonical = root
                .canonicalize()
                .map_err(|source| AllowlistError::CanonicalizeRoot { root, source })?;
            canonical_roots.push(canonical);
        }

        canonical_roots.sort();
        canonical_roots.dedup();

        if canonical_roots.is_empty() {
            return Err(AllowlistError::EmptyRoots);
        }

        Ok(Self {
            roots: canonical_roots,
        })
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn validate_request_path(&self, path: &Path) -> Result<PathBuf, AllowlistError> {
        reject_url_like(path)?;
        let canonical =
            path.canonicalize()
                .map_err(|source| AllowlistError::CanonicalizeRequest {
                    path: path.to_path_buf(),
                    source,
                })?;

        if self.contains_canonical_path(&canonical) {
            Ok(canonical)
        } else {
            Err(AllowlistError::OutsideRoots { path: canonical })
        }
    }

    pub fn contains_canonical_path(&self, path: &Path) -> bool {
        self.roots
            .iter()
            .any(|root| path_component_descendant(path, root))
    }
}

fn path_component_descendant(path: &Path, root: &Path) -> bool {
    match path.strip_prefix(root) {
        Ok(suffix) => suffix.components().next().is_some(),
        Err(_) => false,
    }
}

fn reject_url_like(path: &Path) -> Result<(), AllowlistError> {
    if os_str_contains(path.as_os_str(), "://") {
        Err(AllowlistError::UrlLikePath(path.to_path_buf()))
    } else {
        Ok(())
    }
}

fn os_str_contains(value: &OsStr, needle: &str) -> bool {
    value.to_string_lossy().contains(needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;

    #[test]
    fn allowlist_requires_canonical_descendant() {
        let fixture = TempFixture::new("descendant");
        let root = fixture.mkdir("root");
        let allowed = fixture.file("root/a.pdf");
        fixture.mkdir("root-evil");
        let evil_prefix_file = fixture.file("root-evil/a.pdf");

        let allowlist = Allowlist::try_new(vec![root.clone()]).unwrap();
        assert_eq!(allowlist.roots(), &[root.canonicalize().unwrap()]);
        assert_eq!(
            allowlist.validate_request_path(&allowed).unwrap(),
            allowed.canonicalize().unwrap()
        );
        assert!(matches!(
            allowlist.validate_request_path(&root),
            Err(AllowlistError::OutsideRoots { .. })
        ));
        assert!(matches!(
            allowlist.validate_request_path(&evil_prefix_file),
            Err(AllowlistError::OutsideRoots { .. })
        ));
    }

    #[test]
    fn allowlist_rejects_missing_paths_and_urls() {
        let fixture = TempFixture::new("rejects");
        let root = fixture.mkdir("root");
        let allowlist = Allowlist::try_new(vec![root]).unwrap();

        assert!(matches!(
            Allowlist::try_new(vec![PathBuf::from("https://example.test/a.pdf")]),
            Err(AllowlistError::UrlLikePath(_))
        ));
        assert!(matches!(
            allowlist.validate_request_path(Path::new("https://example.test/a.pdf")),
            Err(AllowlistError::UrlLikePath(_))
        ));
        assert!(matches!(
            allowlist.validate_request_path(&fixture.path("root/missing.pdf")),
            Err(AllowlistError::CanonicalizeRequest { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn allowlist_rejects_symlink_escape_and_parent_escape() {
        use std::os::unix::fs::symlink;

        let fixture = TempFixture::new("escape");
        let root = fixture.mkdir("root");
        fixture.mkdir("outside");
        let outside_file = fixture.file("outside/secret.pdf");
        symlink(&outside_file, fixture.path("root/link.pdf")).unwrap();

        let allowlist = Allowlist::try_new(vec![root]).unwrap();
        assert!(matches!(
            allowlist.validate_request_path(&fixture.path("root/link.pdf")),
            Err(AllowlistError::OutsideRoots { .. })
        ));
        assert!(matches!(
            allowlist.validate_request_path(&fixture.path("root/../outside/secret.pdf")),
            Err(AllowlistError::OutsideRoots { .. })
        ));
    }

    struct TempFixture {
        root: PathBuf,
    }

    impl TempFixture {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "pdbg-mcp-{}-{}-{}",
                name,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&root).unwrap();
            Self { root }
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.root.join(relative)
        }

        fn mkdir(&self, relative: &str) -> PathBuf {
            let path = self.path(relative);
            fs::create_dir_all(&path).unwrap();
            path
        }

        fn file(&self, relative: &str) -> PathBuf {
            let path = self.path(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            let mut file = File::create(&path).unwrap();
            file.write_all(b"%PDF fake").unwrap();
            path
        }
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
