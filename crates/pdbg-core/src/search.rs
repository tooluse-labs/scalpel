use crate::{
    ChildContainer, ChildPage, ChildRange, NodeId, ObjectDetail, ObjectId, ObjectKind,
    ObjectSummary, ObjectValue, PageRect, ShimDocument, ShimError, TextPage, TextRequest,
};
use pdbg_shim::raw;
use std::collections::{HashSet, VecDeque};

#[derive(Clone, Debug)]
pub struct ObjectSearchRequest {
    pub query: String,
    pub root: Option<NodeId>,
    pub child_page_size: usize,
    pub max_child_pages_per_node: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_results: usize,
    pub inspect_details: bool,
}

impl ObjectSearchRequest {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            root: None,
            child_page_size: 64,
            max_child_pages_per_node: 1,
            max_depth: 4,
            max_nodes: 512,
            max_results: 100,
            inspect_details: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectSearchField {
    ObjectNumber,
    DictionaryKey,
    NameObject,
    ScalarPreview,
    Label,
}

#[derive(Clone, Debug)]
pub struct ObjectSearchHit {
    pub label: String,
    pub matched_field: ObjectSearchField,
    pub excerpt: String,
    pub object: Option<ObjectId>,
    pub node: Option<NodeId>,
    pub depth: usize,
}

#[derive(Clone, Debug)]
pub struct ObjectSearchResult {
    pub hits: Vec<ObjectSearchHit>,
    pub searched_nodes: usize,
    pub truncated: bool,
}

#[derive(Clone, Debug)]
pub struct TextSearchRequest {
    pub query: String,
    pub start_page: usize,
    pub max_pages: usize,
    pub max_results: usize,
    pub max_excerpt_chars: usize,
    pub max_chars_per_page: usize,
    pub max_blocks_per_page: usize,
}

impl TextSearchRequest {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            start_page: 0,
            max_pages: 64,
            max_results: 100,
            max_excerpt_chars: 160,
            max_chars_per_page: 512_000,
            max_blocks_per_page: 50_000,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TextSearchHit {
    pub page_index: usize,
    pub span_index: usize,
    pub excerpt: String,
    pub bbox: Option<PageRect>,
    pub untrusted: bool,
}

#[derive(Clone, Debug)]
pub struct TextSearchPageError {
    pub page_index: usize,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct TextSearchResult {
    pub hits: Vec<TextSearchHit>,
    pub searched_pages: usize,
    pub cache_hits: usize,
    pub page_errors: Vec<TextSearchPageError>,
    pub truncated: bool,
}

impl TextSearchResult {
    fn empty() -> Self {
        Self {
            hits: Vec::new(),
            searched_pages: 0,
            cache_hits: 0,
            page_errors: Vec::new(),
            truncated: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TextPageCache {
    entries: VecDeque<TextPageCacheEntry>,
    max_pages: usize,
    max_bytes: usize,
    current_bytes: usize,
}

#[derive(Clone, Debug)]
struct TextPageCacheEntry {
    page: TextPage,
    bytes: usize,
}

impl TextPageCache {
    pub fn new(max_pages: usize, max_bytes: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_pages: max_pages.max(1),
            max_bytes: max_bytes.max(1),
            current_bytes: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn current_bytes(&self) -> usize {
        self.current_bytes
    }

    pub fn get(&mut self, page_index: usize) -> Option<TextPage> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.page.page_index == page_index)?;
        let entry = self.entries.remove(index)?;
        let page = entry.page.clone();
        self.entries.push_back(entry);
        Some(page)
    }

    pub fn insert(&mut self, page: TextPage) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.page.page_index == page.page_index)
        {
            if let Some(entry) = self.entries.remove(index) {
                self.current_bytes = self.current_bytes.saturating_sub(entry.bytes);
            }
        }

        let bytes = estimate_text_page_bytes(&page);
        if bytes > self.max_bytes {
            return;
        }
        self.current_bytes += bytes;
        self.entries.push_back(TextPageCacheEntry { page, bytes });
        self.evict_to_budget();
    }

    fn evict_to_budget(&mut self) {
        while self.entries.len() > self.max_pages || self.current_bytes > self.max_bytes {
            let Some(entry) = self.entries.pop_front() else {
                break;
            };
            self.current_bytes = self.current_bytes.saturating_sub(entry.bytes);
        }
    }
}

struct PendingNode {
    node: NodeId,
    depth: usize,
    container: ChildContainer,
}

struct SearchState {
    queue: VecDeque<PendingNode>,
    hits: Vec<ObjectSearchHit>,
    searched_nodes: usize,
    truncated: bool,
}

struct SearchContext<'a> {
    query: &'a Query,
    request: &'a ObjectSearchRequest,
    detail_range: ChildRange,
    max_nodes: usize,
    max_results: usize,
}

struct Query {
    folded: String,
    object_ref: Option<ParsedObjectRef>,
}

#[derive(Clone, Copy)]
struct ParsedObjectRef {
    num: i32,
    gen: Option<i32>,
}

pub fn search_objects<D>(
    document: &mut D,
    request: &ObjectSearchRequest,
) -> Result<ObjectSearchResult, ShimError>
where
    D: ShimDocument,
{
    let query = Query::new(&request.query);
    if query.folded.is_empty() {
        return Ok(ObjectSearchResult {
            hits: Vec::new(),
            searched_nodes: 0,
            truncated: false,
        });
    }

    let root = match &request.root {
        Some(root) => root.clone(),
        None => {
            let summary = document.summary()?;
            NodeId::DocumentRoot { doc: summary.doc }
        }
    };

    let child_page_size = request.child_page_size.max(1);
    let max_child_pages_per_node = request.max_child_pages_per_node.max(1);
    let max_nodes = request.max_nodes.max(1);
    let max_results = request.max_results.max(1);
    let detail_range = ChildRange {
        offset: 0,
        limit: child_page_size,
    };
    let context = SearchContext {
        query: &query,
        request,
        detail_range,
        max_nodes,
        max_results,
    };

    let root_container = child_container_for_node(&root);
    let mut queue = VecDeque::new();
    queue.push_back(PendingNode {
        node: root,
        depth: 0,
        container: root_container,
    });
    let mut state = SearchState {
        queue,
        hits: Vec::new(),
        searched_nodes: 0,
        truncated: false,
    };
    let mut visited = HashSet::new();

    while let Some(pending) = state.queue.pop_front() {
        if !visited.insert(pending.node.clone()) {
            continue;
        }
        if pending.depth >= request.max_depth {
            continue;
        }

        let mut offset = 0;
        let mut exhausted_node = false;
        for _ in 0..max_child_pages_per_node {
            if state.searched_nodes >= max_nodes || state.hits.len() >= max_results {
                state.truncated = true;
                break;
            }

            let page = document.children(
                &pending.node,
                ChildRange {
                    offset,
                    limit: child_page_size,
                },
                pending.container,
            )?;
            let page_len = page.items.len();
            let total = page.total;
            if page_len == 0 {
                exhausted_node = true;
                break;
            }

            consume_child_page(document, &context, pending.depth + 1, page, &mut state)?;

            offset += page_len;
            if state.searched_nodes >= max_nodes || state.hits.len() >= max_results {
                state.truncated = true;
                break;
            }
            if page_is_exhausted(offset, page_len, child_page_size, total) {
                exhausted_node = true;
                break;
            }
        }
        if !exhausted_node && state.searched_nodes < max_nodes && state.hits.len() < max_results {
            state.truncated = true;
        }
    }

    Ok(ObjectSearchResult {
        hits: state.hits,
        searched_nodes: state.searched_nodes,
        truncated: state.truncated,
    })
}

pub fn search_text<D>(
    document: &mut D,
    cache: &mut TextPageCache,
    request: &TextSearchRequest,
) -> Result<TextSearchResult, ShimError>
where
    D: ShimDocument,
{
    let page_count = document.summary()?.page_count;
    search_text_with_cache(page_count, cache, request, |text_request| {
        document.extract_text(text_request)
    })
}

pub fn search_text_with_cache<F>(
    page_count: usize,
    cache: &mut TextPageCache,
    request: &TextSearchRequest,
    mut extract: F,
) -> Result<TextSearchResult, ShimError>
where
    F: FnMut(&TextRequest) -> Result<TextPage, ShimError>,
{
    let query = Query::new(&request.query);
    if query.folded.is_empty() || page_count == 0 {
        return Ok(TextSearchResult::empty());
    }

    let mut result = TextSearchResult::empty();
    let max_results = request.max_results.max(1);
    let start_page = request.start_page.min(page_count);
    let max_pages = request.max_pages.max(1);
    let end_page = start_page.saturating_add(max_pages).min(page_count);

    for page_index in start_page..end_page {
        if result.hits.len() >= max_results {
            result.truncated = true;
            break;
        }

        let page = if let Some(page) = cache.get(page_index) {
            result.cache_hits += 1;
            page
        } else {
            let mut text_request = TextRequest::page(page_index);
            text_request.max_chars = request.max_chars_per_page.max(1);
            text_request.max_blocks = request.max_blocks_per_page.max(1);
            match extract(&text_request) {
                Ok(page) => {
                    cache.insert(page.clone());
                    page
                }
                Err(err) if err.status == raw::pdbg_status::PDBG_ERROR_CANCELLED => {
                    return Err(err);
                }
                Err(err) => {
                    result.searched_pages += 1;
                    result.page_errors.push(TextSearchPageError {
                        page_index,
                        message: err.message,
                    });
                    continue;
                }
            }
        };

        result.searched_pages += 1;
        append_text_hits(&page, &query.folded, request, max_results, &mut result);
    }

    if end_page < page_count {
        result.truncated = true;
    }

    Ok(result)
}

fn consume_child_page<D>(
    document: &mut D,
    context: &SearchContext<'_>,
    depth: usize,
    page: ChildPage,
    state: &mut SearchState,
) -> Result<(), ShimError>
where
    D: ShimDocument,
{
    for summary in page.items {
        if state.searched_nodes >= context.max_nodes || state.hits.len() >= context.max_results {
            state.truncated = true;
            break;
        }

        state.searched_nodes += 1;
        if let Some(hit) = match_summary(context.query, &summary, depth) {
            state.hits.push(hit);
            if state.hits.len() >= context.max_results {
                state.truncated = true;
                break;
            }
        } else if context.request.inspect_details {
            let detail = document.object_detail(&summary.id, context.detail_range)?;
            if let Some(hit) = match_detail(context.query, &summary, &detail, depth) {
                state.hits.push(hit);
                if state.hits.len() >= context.max_results {
                    state.truncated = true;
                    break;
                }
            }
        }

        if summary.has_children {
            if depth < context.request.max_depth {
                state.queue.push_back(PendingNode {
                    container: child_container_for_summary(&summary),
                    node: summary.id.clone(),
                    depth,
                });
            } else {
                state.truncated = true;
            }
        }
    }

    Ok(())
}

fn page_is_exhausted(
    offset_after_page: usize,
    page_len: usize,
    child_page_size: usize,
    total: Option<usize>,
) -> bool {
    if let Some(total) = total {
        return offset_after_page >= total;
    }
    page_len < child_page_size
}

fn match_summary(query: &Query, summary: &ObjectSummary, depth: usize) -> Option<ObjectSearchHit> {
    if object_ref_matches(query.object_ref, summary.object) {
        return Some(hit(
            summary,
            ObjectSearchField::ObjectNumber,
            summary
                .object
                .map(|object| format!("{} {} R", object.num, object.gen))
                .unwrap_or_default(),
            depth,
        ));
    }

    if let Some(key) = dict_key(&summary.id) {
        if contains_folded(key, &query.folded) {
            return Some(hit(
                summary,
                ObjectSearchField::DictionaryKey,
                key.to_string(),
                depth,
            ));
        }
    }

    if preview_is_name(&summary.preview) && contains_folded(&summary.preview, &query.folded) {
        return Some(hit(
            summary,
            ObjectSearchField::NameObject,
            summary.preview.clone(),
            depth,
        ));
    }

    if contains_folded(&summary.preview, &query.folded) {
        return Some(hit(
            summary,
            ObjectSearchField::ScalarPreview,
            summary.preview.clone(),
            depth,
        ));
    }

    if contains_folded(&summary.label, &query.folded) {
        return Some(hit(
            summary,
            ObjectSearchField::Label,
            summary.label.clone(),
            depth,
        ));
    }

    None
}

fn match_detail(
    query: &Query,
    summary: &ObjectSummary,
    detail: &ObjectDetail,
    depth: usize,
) -> Option<ObjectSearchHit> {
    match &detail.value {
        ObjectValue::Name(name) if contains_folded(name, &query.folded) => Some(hit(
            summary,
            ObjectSearchField::NameObject,
            name.clone(),
            depth,
        )),
        ObjectValue::Bool(value) if contains_folded(&value.to_string(), &query.folded) => {
            Some(hit(
                summary,
                ObjectSearchField::ScalarPreview,
                value.to_string(),
                depth,
            ))
        }
        ObjectValue::Int(value) if contains_folded(&value.to_string(), &query.folded) => Some(hit(
            summary,
            ObjectSearchField::ScalarPreview,
            value.to_string(),
            depth,
        )),
        ObjectValue::Real(value) if contains_folded(&value.to_string(), &query.folded) => {
            Some(hit(
                summary,
                ObjectSearchField::ScalarPreview,
                value.to_string(),
                depth,
            ))
        }
        ObjectValue::StringBytes {
            decoded_text: Some(text),
            ..
        } if contains_folded(text, &query.folded) => Some(hit(
            summary,
            ObjectSearchField::ScalarPreview,
            text.clone(),
            depth,
        )),
        _ if contains_folded(&detail.preview, &query.folded) => Some(hit(
            summary,
            ObjectSearchField::ScalarPreview,
            detail.preview.clone(),
            depth,
        )),
        _ => None,
    }
}

fn append_text_hits(
    page: &TextPage,
    folded_query: &str,
    request: &TextSearchRequest,
    max_results: usize,
    result: &mut TextSearchResult,
) {
    for (span_index, span) in page.spans.iter().enumerate() {
        if result.hits.len() >= max_results {
            result.truncated = true;
            return;
        }

        let folded_text = span.text.to_lowercase();
        let mut offset = 0;
        while offset < folded_text.len() {
            let Some(relative_match) = folded_text[offset..].find(folded_query) else {
                break;
            };
            let match_start = offset + relative_match;
            let match_end = match_start + folded_query.len();
            result.hits.push(TextSearchHit {
                page_index: page.page_index,
                span_index,
                excerpt: bounded_text_excerpt(
                    &span.text,
                    match_start,
                    match_end,
                    request.max_excerpt_chars,
                ),
                bbox: Some(span.bbox.clone()),
                untrusted: span.untrusted,
            });
            if result.hits.len() >= max_results {
                result.truncated = true;
                return;
            }
            offset = match_end.max(match_start + 1);
        }
    }
}

fn hit(
    summary: &ObjectSummary,
    matched_field: ObjectSearchField,
    excerpt: String,
    depth: usize,
) -> ObjectSearchHit {
    ObjectSearchHit {
        label: summary.label.clone(),
        matched_field,
        excerpt: truncate_excerpt(&excerpt),
        object: summary.object,
        node: Some(summary.id.clone()),
        depth,
    }
}

fn object_ref_matches(query: Option<ParsedObjectRef>, object: Option<ObjectId>) -> bool {
    match (query, object) {
        (Some(query), Some(object)) => {
            query.num == object.num
                && match query.gen {
                    Some(gen) => gen == object.gen,
                    None => true,
                }
        }
        _ => false,
    }
}

fn dict_key(node: &NodeId) -> Option<&str> {
    match node {
        NodeId::DictEntry { key, .. } => Some(key),
        _ => None,
    }
}

fn child_container_for_node(node: &NodeId) -> ChildContainer {
    match node {
        NodeId::PageRoot { .. } | NodeId::XrefRoot { .. } | NodeId::ArrayEntry { .. } => {
            ChildContainer::Array
        }
        _ => ChildContainer::Dictionary,
    }
}

fn child_container_for_summary(summary: &ObjectSummary) -> ChildContainer {
    if summary.kind == ObjectKind::Array {
        ChildContainer::Array
    } else {
        child_container_for_node(&summary.id)
    }
}

fn preview_is_name(preview: &str) -> bool {
    preview.trim_start().starts_with('/')
}

fn contains_folded(value: &str, folded_query: &str) -> bool {
    value.to_lowercase().contains(folded_query)
}

fn truncate_excerpt(value: &str) -> String {
    const LIMIT: usize = 160;
    if value.len() <= LIMIT {
        return value.to_string();
    }

    let mut end = LIMIT;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &value[..end])
}

fn bounded_text_excerpt(
    text: &str,
    match_start: usize,
    match_end: usize,
    max_chars: usize,
) -> String {
    let total_chars = text.chars().count();
    let max_chars = max_chars.max(1);
    if total_chars <= max_chars {
        return text.to_string();
    }

    let start = clamp_char_boundary(text, match_start.min(text.len()));
    let end = clamp_char_boundary(text, match_end.min(text.len())).max(start);
    let match_start_char = text[..start].chars().count();
    let match_chars = text[start..end].chars().count().max(1);
    let context_chars = max_chars.saturating_sub(match_chars) / 2;
    let excerpt_start_char = match_start_char.saturating_sub(context_chars);
    let excerpt_end_char = excerpt_start_char
        .saturating_add(max_chars)
        .min(total_chars);

    let mut excerpt = String::new();
    if excerpt_start_char > 0 {
        excerpt.push_str("...");
    }
    excerpt.push_str(&slice_by_char_range(
        text,
        excerpt_start_char,
        excerpt_end_char,
    ));
    if excerpt_end_char < total_chars {
        excerpt.push_str("...");
    }
    excerpt
}

fn clamp_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn slice_by_char_range(text: &str, start_char: usize, end_char: usize) -> String {
    text.chars()
        .skip(start_char)
        .take(end_char.saturating_sub(start_char))
        .collect()
}

fn estimate_text_page_bytes(page: &TextPage) -> usize {
    page.spans
        .iter()
        .map(|span| span.text.len() + std::mem::size_of_val(span))
        .sum()
}

impl Query {
    fn new(query: &str) -> Self {
        let trimmed = query.trim().to_string();
        Self {
            folded: trimmed.to_lowercase(),
            object_ref: parse_object_ref(&trimmed),
        }
    }
}

fn parse_object_ref(query: &str) -> Option<ParsedObjectRef> {
    let mut parts = query.split_whitespace();
    let first = parts.next()?;
    match (parts.next(), parts.next(), parts.next()) {
        (None, None, None) => first
            .parse::<i32>()
            .ok()
            .map(|num| ParsedObjectRef { num, gen: None }),
        (Some(gen), Some(r), None) if r.eq_ignore_ascii_case("R") => {
            let num = first.parse::<i32>().ok()?;
            let gen = gen.parse::<i32>().ok()?;
            Some(ParsedObjectRef {
                num,
                gen: Some(gen),
            })
        }
        _ => None,
    }
}

#[cfg(all(test, feature = "fake"))]
mod tests {
    use super::*;
    use crate::{FakeShim, Shim};

    #[test]
    fn object_search_finds_dictionary_keys_across_bounded_child_pages() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();
        let request = ObjectSearchRequest {
            child_page_size: 1,
            max_child_pages_per_node: 3,
            max_depth: 1,
            ..ObjectSearchRequest::new("Key2")
        };

        let result = search_objects(&mut doc, &request).unwrap();

        assert_eq!(result.searched_nodes, 3);
        assert!(result.hits.iter().any(|hit| {
            hit.matched_field == ObjectSearchField::DictionaryKey
                && hit.excerpt == "Key2"
                && hit.object == Some(ObjectId { num: 3, gen: 0 })
        }));
    }

    #[test]
    fn object_search_finds_object_numbers() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();
        let request = ObjectSearchRequest {
            child_page_size: 3,
            max_depth: 1,
            ..ObjectSearchRequest::new("2 0 R")
        };

        let result = search_objects(&mut doc, &request).unwrap();

        assert!(result.hits.iter().any(|hit| {
            hit.matched_field == ObjectSearchField::ObjectNumber
                && hit.object == Some(ObjectId { num: 2, gen: 0 })
        }));
    }

    #[test]
    fn object_search_reports_truncation_when_page_bound_stops_before_match() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();
        let request = ObjectSearchRequest {
            child_page_size: 1,
            max_child_pages_per_node: 1,
            max_depth: 1,
            ..ObjectSearchRequest::new("Key2")
        };

        let result = search_objects(&mut doc, &request).unwrap();

        assert!(result.truncated);
        assert!(result.hits.is_empty());
    }

    #[test]
    fn object_search_matches_scalar_previews_without_detail_fetch() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();
        let request = ObjectSearchRequest {
            child_page_size: 3,
            max_depth: 1,
            ..ObjectSearchRequest::new("fake object")
        };

        let result = search_objects(&mut doc, &request).unwrap();

        assert!(result.hits.iter().any(|hit| {
            hit.matched_field == ObjectSearchField::ScalarPreview && hit.excerpt == "fake object"
        }));
    }

    #[test]
    fn text_search_finds_untrusted_fake_text_and_caches_page() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();
        let mut cache = TextPageCache::new(4, 64 * 1024);
        let request = TextSearchRequest {
            max_pages: 1,
            ..TextSearchRequest::new("A")
        };

        let first = search_text(&mut doc, &mut cache, &request).unwrap();

        assert_eq!(first.searched_pages, 1);
        assert_eq!(first.cache_hits, 0);
        assert!(first.page_errors.is_empty());
        assert!(first.hits.iter().any(|hit| {
            hit.page_index == 0
                && hit.span_index == 0
                && hit.excerpt.as_bytes() == b"A\0B"
                && hit.bbox.is_some()
                && hit.untrusted
        }));
        assert_eq!(cache.len(), 1);

        let second = search_text(&mut doc, &mut cache, &request).unwrap();
        assert_eq!(second.cache_hits, 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn text_search_reports_page_limit_truncation() {
        let mut cache = TextPageCache::new(4, 64 * 1024);
        let request = TextSearchRequest {
            max_pages: 1,
            ..TextSearchRequest::new("needle")
        };

        let result = search_text_with_cache(3, &mut cache, &request, |text_request| {
            Ok(text_page(text_request.page_index, "needle"))
        })
        .unwrap();

        assert_eq!(result.searched_pages, 1);
        assert!(result.truncated);
        assert_eq!(result.hits.len(), 1);
    }

    #[test]
    fn text_page_cache_evicts_to_page_and_byte_budgets() {
        let mut cache = TextPageCache::new(1, 128);
        cache.insert(text_page(0, "first"));
        cache.insert(text_page(1, "second"));

        assert_eq!(cache.len(), 1);
        assert!(cache.get(0).is_none());
        assert!(cache.get(1).is_some());
        assert!(cache.current_bytes() <= 128);
    }

    fn text_page(page_index: usize, text: &str) -> TextPage {
        TextPage {
            page_index,
            spans: vec![crate::TextSpan {
                text: text.to_string(),
                bbox: PageRect {
                    x: 1.0,
                    y: 2.0,
                    width: 3.0,
                    height: 4.0,
                },
                untrusted: true,
            }],
        }
    }
}
