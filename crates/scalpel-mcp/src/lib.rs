use std::collections::{HashMap, VecDeque};
use std::fmt;
#[cfg(unix)]
use std::fs::File;
use std::io;
#[cfg(unix)]
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use scalpel_core::{
    CapabilityDecision, CapabilityFeature, ChildRange, DocumentId, MuPdfCapabilities, ObjectId,
    RenderRequest,
};

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
    let text = path.as_os_str().to_string_lossy();
    let lower = text.to_ascii_lowercase();
    if lower.contains("://")
        || lower.starts_with("file:")
        || text.starts_with("//")
        || text.starts_with("\\\\")
    {
        Err(AllowlistError::UrlLikePath(path.to_path_buf()))
    } else {
        Ok(())
    }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum McpTool {
    InspectStructure,
    GetChildren,
    GetObjectDetail,
    LoadRawStream,
    LoadDecodedStream,
    RenderPage,
    ExtractText,
    GetArtifact,
}

impl McpTool {
    pub fn required_feature(self) -> Option<CapabilityFeature> {
        match self {
            Self::InspectStructure | Self::GetChildren | Self::GetObjectDetail => {
                Some(CapabilityFeature::InspectStructure)
            }
            Self::LoadRawStream => Some(CapabilityFeature::RawStreams),
            Self::LoadDecodedStream => Some(CapabilityFeature::DecodedStreams),
            Self::RenderPage => Some(CapabilityFeature::RenderPages),
            Self::ExtractText => Some(CapabilityFeature::ExtractText),
            Self::GetArtifact => None,
        }
    }
}

pub fn gate_mcp_tool(capabilities: &MuPdfCapabilities, tool: McpTool) -> CapabilityDecision {
    match tool.required_feature() {
        Some(feature) => capabilities.gate(feature),
        None => CapabilityDecision::Enabled,
    }
}

pub fn visible_mcp_tools(capabilities: &MuPdfCapabilities) -> Vec<McpTool> {
    [
        McpTool::InspectStructure,
        McpTool::GetChildren,
        McpTool::GetObjectDetail,
        McpTool::LoadRawStream,
        McpTool::LoadDecodedStream,
        McpTool::RenderPage,
        McpTool::ExtractText,
        McpTool::GetArtifact,
    ]
    .into_iter()
    .filter(|tool| gate_mcp_tool(capabilities, *tool) == CapabilityDecision::Enabled)
    .collect()
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactScope {
    pub session_id: String,
    pub client_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactInput {
    pub media_type: String,
    pub bytes: Vec<u8>,
    pub dimensions: Option<ImageDimensions>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactReference {
    pub id: String,
    pub media_type: String,
    pub byte_len: usize,
    pub dimensions: Option<ImageDimensions>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactBytes {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub dimensions: Option<ImageDimensions>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactStoreConfig {
    pub max_total_bytes: usize,
    pub ttl: Duration,
}

impl Default for ArtifactStoreConfig {
    fn default() -> Self {
        Self {
            max_total_bytes: 64 * 1024 * 1024,
            ttl: Duration::from_secs(10 * 60),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ArtifactStoreError {
    EmptyScope,
    EmptyMediaType,
    EmptyArtifact,
    ArtifactTooLarge { size: usize, max: usize },
    EntropyUnavailable,
    NotFound,
    InvalidByteLimit { requested: usize },
}

impl fmt::Display for ArtifactStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyScope => write!(f, "artifact scope requires session and client ids"),
            Self::EmptyMediaType => write!(f, "artifact media type must not be empty"),
            Self::EmptyArtifact => write!(f, "artifact bytes must not be empty"),
            Self::ArtifactTooLarge { size, max } => {
                write!(f, "artifact size {size} exceeds store limit {max}")
            }
            Self::EntropyUnavailable => write!(f, "secure artifact id entropy is unavailable"),
            Self::NotFound => write!(f, "artifact not found"),
            Self::InvalidByteLimit { requested } => {
                write!(f, "artifact byte limit must be non-zero: {requested}")
            }
        }
    }
}

impl std::error::Error for ArtifactStoreError {}

pub struct ArtifactStore {
    config: ArtifactStoreConfig,
    entries: HashMap<String, StoredArtifact>,
    lru: VecDeque<String>,
    total_bytes: usize,
}

struct StoredArtifact {
    scope: ArtifactScope,
    media_type: String,
    bytes: Vec<u8>,
    dimensions: Option<ImageDimensions>,
    created_at: Instant,
}

impl ArtifactStore {
    pub fn new(config: ArtifactStoreConfig) -> Self {
        Self {
            config,
            entries: HashMap::new(),
            lru: VecDeque::new(),
            total_bytes: 0,
        }
    }

    pub fn insert(
        &mut self,
        scope: ArtifactScope,
        artifact: ArtifactInput,
    ) -> Result<ArtifactReference, ArtifactStoreError> {
        self.insert_at(scope, artifact, Instant::now())
    }

    pub fn get(
        &mut self,
        scope: &ArtifactScope,
        id: &str,
        max_output_bytes: usize,
    ) -> Result<ArtifactBytes, ArtifactStoreError> {
        self.get_at(scope, id, max_output_bytes, Instant::now())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    fn insert_at(
        &mut self,
        scope: ArtifactScope,
        artifact: ArtifactInput,
        now: Instant,
    ) -> Result<ArtifactReference, ArtifactStoreError> {
        validate_scope(&scope)?;
        if artifact.media_type.trim().is_empty() {
            return Err(ArtifactStoreError::EmptyMediaType);
        }
        if artifact.bytes.is_empty() {
            return Err(ArtifactStoreError::EmptyArtifact);
        }
        if artifact.bytes.len() > self.config.max_total_bytes {
            return Err(ArtifactStoreError::ArtifactTooLarge {
                size: artifact.bytes.len(),
                max: self.config.max_total_bytes,
            });
        }

        self.evict_expired(now);
        let id = self.generate_unused_artifact_id()?;
        let byte_len = artifact.bytes.len();
        let reference = ArtifactReference {
            id: id.clone(),
            media_type: artifact.media_type.clone(),
            byte_len,
            dimensions: artifact.dimensions.clone(),
        };

        self.total_bytes += byte_len;
        self.lru.push_back(id.clone());
        self.entries.insert(
            id,
            StoredArtifact {
                scope,
                media_type: artifact.media_type,
                bytes: artifact.bytes,
                dimensions: artifact.dimensions,
                created_at: now,
            },
        );
        self.evict_lru();

        Ok(reference)
    }

    fn get_at(
        &mut self,
        scope: &ArtifactScope,
        id: &str,
        max_output_bytes: usize,
        now: Instant,
    ) -> Result<ArtifactBytes, ArtifactStoreError> {
        if max_output_bytes == 0 {
            return Err(ArtifactStoreError::InvalidByteLimit {
                requested: max_output_bytes,
            });
        }
        validate_scope(scope)?;
        self.evict_expired(now);

        let entry = self
            .entries
            .get_mut(id)
            .filter(|entry| &entry.scope == scope)
            .ok_or(ArtifactStoreError::NotFound)?;

        move_lru_to_back(&mut self.lru, id);
        let len = entry.bytes.len().min(max_output_bytes);
        Ok(ArtifactBytes {
            bytes: entry.bytes[..len].to_vec(),
            media_type: entry.media_type.clone(),
            dimensions: entry.dimensions.clone(),
            truncated: len < entry.bytes.len(),
        })
    }

    fn evict_expired(&mut self, now: Instant) {
        let ttl = self.config.ttl;
        let expired: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, entry)| now.saturating_duration_since(entry.created_at) >= ttl)
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            self.remove(&id);
        }
    }

    fn generate_unused_artifact_id(&self) -> Result<String, ArtifactStoreError> {
        loop {
            let id = generate_artifact_id()?;
            if !self.entries.contains_key(&id) {
                return Ok(id);
            }
        }
    }

    fn evict_lru(&mut self) {
        while self.total_bytes > self.config.max_total_bytes {
            if let Some(id) = self.lru.pop_front() {
                if let Some(entry) = self.entries.remove(&id) {
                    self.total_bytes -= entry.bytes.len();
                }
            } else {
                break;
            }
        }
    }

    fn remove(&mut self, id: &str) {
        if let Some(entry) = self.entries.remove(id) {
            self.total_bytes -= entry.bytes.len();
        }
        self.lru.retain(|candidate| candidate != id);
    }
}

fn validate_scope(scope: &ArtifactScope) -> Result<(), ArtifactStoreError> {
    if scope.session_id.trim().is_empty() || scope.client_id.trim().is_empty() {
        Err(ArtifactStoreError::EmptyScope)
    } else {
        Ok(())
    }
}

fn move_lru_to_back(lru: &mut VecDeque<String>, id: &str) {
    lru.retain(|candidate| candidate != id);
    lru.push_back(id.to_string());
}

fn generate_artifact_id() -> Result<String, ArtifactStoreError> {
    let mut bytes = [0_u8; 16];
    fill_secure_random(&mut bytes)?;
    let mut id = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        id.push(hex_digit(byte >> 4));
        id.push(hex_digit(byte & 0x0f));
    }
    Ok(id)
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => unreachable!("hex nibble must be in 0..16"),
    }
}

#[cfg(unix)]
fn fill_secure_random(bytes: &mut [u8]) -> Result<(), ArtifactStoreError> {
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(bytes))
        .map_err(|_| ArtifactStoreError::EntropyUnavailable)
}

#[cfg(windows)]
fn fill_secure_random(bytes: &mut [u8]) -> Result<(), ArtifactStoreError> {
    use std::ffi::c_void;

    #[link(name = "advapi32")]
    extern "system" {
        fn SystemFunction036(random_buffer: *mut c_void, random_buffer_length: u32) -> u8;
    }

    let len = u32::try_from(bytes.len()).map_err(|_| ArtifactStoreError::EntropyUnavailable)?;
    let ok = unsafe { SystemFunction036(bytes.as_mut_ptr().cast::<c_void>(), len) };
    if ok == 0 {
        Err(ArtifactStoreError::EntropyUnavailable)
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn fill_secure_random(_bytes: &mut [u8]) -> Result<(), ArtifactStoreError> {
    Err(ArtifactStoreError::EntropyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scalpel_core::RenderColorMode;
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
            allowlist.validate_request_path(Path::new("file:/tmp/a.pdf")),
            Err(AllowlistError::UrlLikePath(_))
        ));
        assert!(matches!(
            allowlist.validate_request_path(Path::new("//server/share/a.pdf")),
            Err(AllowlistError::UrlLikePath(_))
        ));
        assert!(matches!(
            allowlist.validate_request_path(Path::new("\\\\server\\share\\a.pdf")),
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

    #[test]
    fn mcp_tools_are_hidden_or_unsupported_by_capability() {
        let mut capabilities = MuPdfCapabilities::mupdf_only_default();
        assert_eq!(
            gate_mcp_tool(&capabilities, McpTool::RenderPage),
            CapabilityDecision::Enabled
        );
        assert!(visible_mcp_tools(&capabilities).contains(&McpTool::RenderPage));

        capabilities.can_render_pages = false;
        assert_eq!(
            gate_mcp_tool(&capabilities, McpTool::RenderPage),
            CapabilityDecision::Unsupported {
                reason: "page rendering is unavailable"
            }
        );
        assert!(!visible_mcp_tools(&capabilities).contains(&McpTool::RenderPage));
        assert_eq!(
            gate_mcp_tool(&capabilities, McpTool::GetArtifact),
            CapabilityDecision::Enabled
        );
    }

    fn valid_render_request(limits: &McpInputLimits) -> RenderRequest {
        RenderRequest {
            max_output_bytes: limits.max_output_bytes,
            max_pixels: limits.max_render_pixels,
            ..RenderRequest::page(0)
        }
    }

    #[test]
    fn artifact_store_returns_unguessable_scoped_references() {
        let mut store = ArtifactStore::new(ArtifactStoreConfig::default());
        let scope = artifact_scope("session-a", "client-a");

        let first = store
            .insert(scope.clone(), fake_image_artifact(&[1, 2, 3, 4]))
            .unwrap();
        let second = store
            .insert(scope.clone(), fake_image_artifact(&[5, 6, 7, 8]))
            .unwrap();

        assert_eq!(first.id.len(), 32);
        assert!(first.id.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_ne!(first.id, second.id);
        assert_eq!(first.media_type, "image/png");
        assert_eq!(
            first.dimensions,
            Some(ImageDimensions {
                width: 1,
                height: 1
            })
        );

        assert!(matches!(
            store.get(&artifact_scope("session-b", "client-a"), &first.id, 4),
            Err(ArtifactStoreError::NotFound)
        ));
        assert!(matches!(
            store.get(&artifact_scope("session-a", "client-b"), &first.id, 4),
            Err(ArtifactStoreError::NotFound)
        ));
    }

    #[test]
    fn artifact_store_get_truncates_by_output_limit() {
        let mut store = ArtifactStore::new(ArtifactStoreConfig::default());
        let scope = artifact_scope("session", "client");
        let reference = store
            .insert(scope.clone(), fake_image_artifact(&[1, 2, 3, 4, 5]))
            .unwrap();

        let bytes = store.get(&scope, &reference.id, 3).unwrap();
        assert_eq!(bytes.bytes, vec![1, 2, 3]);
        assert_eq!(bytes.media_type, "image/png");
        assert!(bytes.truncated);
        assert_eq!(
            bytes.dimensions,
            Some(ImageDimensions {
                width: 1,
                height: 1
            })
        );

        assert!(matches!(
            store.get(&scope, &reference.id, 0),
            Err(ArtifactStoreError::InvalidByteLimit { requested: 0 })
        ));
    }

    #[test]
    fn artifact_store_expires_entries_by_ttl() {
        let mut store = ArtifactStore::new(ArtifactStoreConfig {
            max_total_bytes: 1024,
            ttl: Duration::from_secs(5),
        });
        let scope = artifact_scope("session", "client");
        let now = Instant::now();
        let reference = store
            .insert_at(scope.clone(), fake_image_artifact(&[1, 2, 3]), now)
            .unwrap();

        assert!(store
            .get_at(&scope, &reference.id, 3, now + Duration::from_secs(4))
            .is_ok());
        assert!(matches!(
            store.get_at(&scope, &reference.id, 3, now + Duration::from_secs(5)),
            Err(ArtifactStoreError::NotFound)
        ));
        assert!(store.is_empty());
    }

    #[test]
    fn artifact_store_evicts_least_recently_used_entries() {
        let mut store = ArtifactStore::new(ArtifactStoreConfig {
            max_total_bytes: 6,
            ttl: Duration::from_secs(60),
        });
        let scope = artifact_scope("session", "client");

        let first = store
            .insert(
                scope.clone(),
                artifact_bytes("application/octet-stream", &[1, 1, 1]),
            )
            .unwrap();
        let second = store
            .insert(
                scope.clone(),
                artifact_bytes("application/octet-stream", &[2, 2, 2]),
            )
            .unwrap();
        store.get(&scope, &first.id, 3).unwrap();
        let third = store
            .insert(
                scope.clone(),
                artifact_bytes("application/octet-stream", &[3, 3, 3]),
            )
            .unwrap();

        assert!(store.get(&scope, &first.id, 3).is_ok());
        assert!(matches!(
            store.get(&scope, &second.id, 3),
            Err(ArtifactStoreError::NotFound)
        ));
        assert!(store.get(&scope, &third.id, 3).is_ok());
        assert_eq!(store.total_bytes(), 6);
    }

    #[test]
    fn artifact_store_rejects_invalid_inputs() {
        let mut store = ArtifactStore::new(ArtifactStoreConfig {
            max_total_bytes: 4,
            ttl: Duration::from_secs(60),
        });

        assert!(matches!(
            store.insert(artifact_scope("", "client"), fake_image_artifact(&[1])),
            Err(ArtifactStoreError::EmptyScope)
        ));
        assert!(matches!(
            store.insert(
                artifact_scope("session", "client"),
                artifact_bytes("", &[1])
            ),
            Err(ArtifactStoreError::EmptyMediaType)
        ));
        assert!(matches!(
            store.insert(
                artifact_scope("session", "client"),
                artifact_bytes("application/octet-stream", &[])
            ),
            Err(ArtifactStoreError::EmptyArtifact)
        ));
        assert!(matches!(
            store.insert(
                artifact_scope("session", "client"),
                artifact_bytes("application/octet-stream", &[1, 2, 3, 4, 5])
            ),
            Err(ArtifactStoreError::ArtifactTooLarge { size: 5, max: 4 })
        ));
    }

    fn artifact_scope(session_id: &str, client_id: &str) -> ArtifactScope {
        ArtifactScope {
            session_id: session_id.to_string(),
            client_id: client_id.to_string(),
        }
    }

    fn fake_image_artifact(bytes: &[u8]) -> ArtifactInput {
        ArtifactInput {
            media_type: "image/png".to_string(),
            bytes: bytes.to_vec(),
            dimensions: Some(ImageDimensions {
                width: 1,
                height: 1,
            }),
        }
    }

    fn artifact_bytes(media_type: &str, bytes: &[u8]) -> ArtifactInput {
        ArtifactInput {
            media_type: media_type.to_string(),
            bytes: bytes.to_vec(),
            dimensions: None,
        }
    }

    struct TempFixture {
        root: PathBuf,
    }

    impl TempFixture {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "scalpel-mcp-{}-{}-{}",
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
