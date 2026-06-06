use crate::{
    ChildContainer, ChildPage, ChildRange, NodeId, ObjectDetail, ObjectId, ObjectKind,
    ObjectSummary, ObjectValue, ShimDocument, ShimError,
};
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

            consume_child_page(
                document,
                &context,
                pending.depth + 1,
                page,
                &mut state,
            )?;

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
        if !exhausted_node
            && state.searched_nodes < max_nodes
            && state.hits.len() < max_results
        {
            state.truncated = true;
        }
    }

    Ok(ObjectSearchResult {
        hits: state.hits,
        searched_nodes: state.searched_nodes,
        truncated: state.truncated,
    })
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

fn match_summary(
    query: &Query,
    summary: &ObjectSummary,
    depth: usize,
) -> Option<ObjectSearchHit> {
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
        ObjectValue::Name(name) if contains_folded(name, &query.folded) => {
            Some(hit(summary, ObjectSearchField::NameObject, name.clone(), depth))
        }
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
        } if contains_folded(text, &query.folded) => {
            Some(hit(summary, ObjectSearchField::ScalarPreview, text.clone(), depth))
        }
        _ if contains_folded(&detail.preview, &query.folded) => Some(hit(
            summary,
            ObjectSearchField::ScalarPreview,
            detail.preview.clone(),
            depth,
        )),
        _ => None,
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
            hit.matched_field == ObjectSearchField::ScalarPreview
                && hit.excerpt == "fake object"
        }));
    }
}
