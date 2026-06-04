use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use pdbg_core::{ChildRange, DocumentId, ObjectId, RenderRequest};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpInputLimits {
    pub max_child_limit: usize,
    pub max_stream_limit: usize,
    pub max_output_bytes: u64,
    pub max_page_index: usize,
    pub max_render_dimension: u32,
    pub max_render_pixels: u64,
}

impl Default for McpInputLimits {
    fn default() -> Self {
        Self {
            max_child_limit: 200,
            max_stream_limit: 1024 * 1024,
            max_output_bytes: 4 * 1024 * 1024,
            max_page_index: u32::MAX as usize,
            max_render_dimension: 4096,
            max_render_pixels: 16_777_216,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputValidationError {
    InvalidDocumentId(u64),
    InvalidObjectId(ObjectId),
    InvalidRange {
        offset: usize,
        limit: usize,
        max_limit: usize,
    },
    RangeOverflow {
        offset: usize,
        limit: usize,
    },
    InvalidByteLimit {
        requested: u64,
        max: u64,
    },
    InvalidPageIndex {
        requested: usize,
        max: usize,
    },
    InvalidRenderDimension {
        width: u32,
        height: u32,
        max_dimension: u32,
    },
    InvalidRenderPixels {
        requested: u64,
        max: u64,
    },
    InvalidRotation(i32),
}

impl fmt::Display for InputValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDocumentId(value) => write!(f, "invalid document id: {value}"),
            Self::InvalidObjectId(object) => {
                write!(f, "invalid object id: {} {}", object.num, object.gen)
            }
            Self::InvalidRange {
                offset,
                limit,
                max_limit,
            } => write!(
                f,
                "invalid range offset={offset} limit={limit}; max limit is {max_limit}"
            ),
            Self::RangeOverflow { offset, limit } => {
                write!(f, "range overflows usize: offset={offset} limit={limit}")
            }
            Self::InvalidByteLimit { requested, max } => {
                write!(f, "invalid byte limit {requested}; max is {max}")
            }
            Self::InvalidPageIndex { requested, max } => {
                write!(f, "invalid page index {requested}; max is {max}")
            }
            Self::InvalidRenderDimension {
                width,
                height,
                max_dimension,
            } => write!(
                f,
                "invalid render dimensions {width}x{height}; max dimension is {max_dimension}"
            ),
            Self::InvalidRenderPixels { requested, max } => {
                write!(f, "invalid render pixel count {requested}; max is {max}")
            }
            Self::InvalidRotation(rotation) => write!(f, "invalid rotation: {rotation}"),
        }
    }
}

impl std::error::Error for InputValidationError {}

pub fn validate_document_id(id: DocumentId) -> Result<DocumentId, InputValidationError> {
    if id.0 == 0 {
        Err(InputValidationError::InvalidDocumentId(id.0))
    } else {
        Ok(id)
    }
}

pub fn validate_object_id(id: ObjectId) -> Result<ObjectId, InputValidationError> {
    if id.num <= 0 || id.gen < 0 {
        Err(InputValidationError::InvalidObjectId(id))
    } else {
        Ok(id)
    }
}

pub fn validate_child_range(
    range: ChildRange,
    limits: &McpInputLimits,
) -> Result<ChildRange, InputValidationError> {
    if range.limit == 0 || range.limit > limits.max_child_limit {
        return Err(InputValidationError::InvalidRange {
            offset: range.offset,
            limit: range.limit,
            max_limit: limits.max_child_limit,
        });
    }
    if range.offset.checked_add(range.limit).is_none() {
        return Err(InputValidationError::RangeOverflow {
            offset: range.offset,
            limit: range.limit,
        });
    }
    Ok(range)
}

pub fn validate_stream_limit(
    limit: usize,
    limits: &McpInputLimits,
) -> Result<usize, InputValidationError> {
    if limit == 0 || limit > limits.max_stream_limit {
        Err(InputValidationError::InvalidRange {
            offset: 0,
            limit,
            max_limit: limits.max_stream_limit,
        })
    } else {
        Ok(limit)
    }
}

pub fn validate_output_limit(
    requested: u64,
    limits: &McpInputLimits,
) -> Result<u64, InputValidationError> {
    if requested == 0 || requested > limits.max_output_bytes {
        Err(InputValidationError::InvalidByteLimit {
            requested,
            max: limits.max_output_bytes,
        })
    } else {
        Ok(requested)
    }
}

pub fn validate_page_index(
    page_index: usize,
    limits: &McpInputLimits,
) -> Result<usize, InputValidationError> {
    if page_index > limits.max_page_index {
        Err(InputValidationError::InvalidPageIndex {
            requested: page_index,
            max: limits.max_page_index,
        })
    } else {
        Ok(page_index)
    }
}

pub fn validate_render_request(
    request: &RenderRequest,
    limits: &McpInputLimits,
) -> Result<(), InputValidationError> {
    validate_page_index(request.page_index, limits)?;
    validate_output_limit(request.max_output_bytes, limits)?;
    if request.max_width == 0
        || request.max_height == 0
        || request.max_width > limits.max_render_dimension
        || request.max_height > limits.max_render_dimension
    {
        return Err(InputValidationError::InvalidRenderDimension {
            width: request.max_width,
            height: request.max_height,
            max_dimension: limits.max_render_dimension,
        });
    }
    if request.max_pixels == 0 || request.max_pixels > limits.max_render_pixels {
        return Err(InputValidationError::InvalidRenderPixels {
            requested: request.max_pixels,
            max: limits.max_render_pixels,
        });
    }
    if !matches!(request.rotation_degrees, 0 | 90 | 180 | 270) {
        return Err(InputValidationError::InvalidRotation(
            request.rotation_degrees,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdbg_core::RenderColorMode;
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

    #[test]
    fn input_validation_accepts_normal_ids_ranges_and_limits() {
        let limits = McpInputLimits::default();
        assert_eq!(validate_document_id(DocumentId(1)).unwrap(), DocumentId(1));
        assert_eq!(
            validate_object_id(ObjectId { num: 12, gen: 0 }).unwrap(),
            ObjectId { num: 12, gen: 0 }
        );
        assert_eq!(
            validate_child_range(
                ChildRange {
                    offset: 10,
                    limit: 20
                },
                &limits
            )
            .unwrap(),
            ChildRange {
                offset: 10,
                limit: 20
            }
        );
        assert_eq!(validate_stream_limit(4096, &limits).unwrap(), 4096);
        assert_eq!(validate_output_limit(4096, &limits).unwrap(), 4096);
    }

    #[test]
    fn input_validation_rejects_bad_ids_and_bounds() {
        let limits = McpInputLimits::default();
        assert_eq!(
            validate_document_id(DocumentId(0)).unwrap_err(),
            InputValidationError::InvalidDocumentId(0)
        );
        assert_eq!(
            validate_object_id(ObjectId { num: 0, gen: 0 }).unwrap_err(),
            InputValidationError::InvalidObjectId(ObjectId { num: 0, gen: 0 })
        );
        assert!(matches!(
            validate_child_range(
                ChildRange {
                    offset: usize::MAX,
                    limit: 1
                },
                &limits
            ),
            Err(InputValidationError::RangeOverflow { .. })
        ));
        assert!(matches!(
            validate_child_range(
                ChildRange {
                    offset: 0,
                    limit: limits.max_child_limit + 1
                },
                &limits
            ),
            Err(InputValidationError::InvalidRange { .. })
        ));
        assert!(matches!(
            validate_stream_limit(limits.max_stream_limit + 1, &limits),
            Err(InputValidationError::InvalidRange { .. })
        ));
        assert_eq!(
            validate_output_limit(0, &limits).unwrap_err(),
            InputValidationError::InvalidByteLimit {
                requested: 0,
                max: limits.max_output_bytes
            }
        );
    }

    #[test]
    fn render_request_validation_rejects_excessive_or_invalid_values() {
        let limits = McpInputLimits::default();
        validate_render_request(&valid_render_request(&limits), &limits).unwrap();

        let too_wide = RenderRequest {
            max_width: limits.max_render_dimension + 1,
            ..valid_render_request(&limits)
        };
        assert!(matches!(
            validate_render_request(&too_wide, &limits),
            Err(InputValidationError::InvalidRenderDimension { .. })
        ));

        let too_many_pixels = RenderRequest {
            max_pixels: limits.max_render_pixels + 1,
            ..valid_render_request(&limits)
        };
        assert!(matches!(
            validate_render_request(&too_many_pixels, &limits),
            Err(InputValidationError::InvalidRenderPixels { .. })
        ));

        let bad_rotation = RenderRequest {
            rotation_degrees: 45,
            ..valid_render_request(&limits)
        };
        assert_eq!(
            validate_render_request(&bad_rotation, &limits).unwrap_err(),
            InputValidationError::InvalidRotation(45)
        );

        let valid_inverted = RenderRequest {
            color_mode: RenderColorMode::Inverted,
            rotation_degrees: 270,
            ..valid_render_request(&limits)
        };
        validate_render_request(&valid_inverted, &limits).unwrap();
    }

    fn valid_render_request(limits: &McpInputLimits) -> RenderRequest {
        RenderRequest {
            max_output_bytes: limits.max_output_bytes,
            max_pixels: limits.max_render_pixels,
            ..RenderRequest::page(0)
        }
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
