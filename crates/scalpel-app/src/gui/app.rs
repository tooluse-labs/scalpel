use super::*;

/// Shared by the Stream tab and the Hex tab, which load windows of the same
/// stream bytes with different keys.
pub(crate) fn spawn_stream_load_job(state: &AppState, key: RealStreamKey) -> RealStreamJob {
    let session = state.session.clone();
    BackgroundJob::spawn(key, move |cancel| {
        session
            .run_task(|document| {
                document.stream_load_with_cancel_token(
                    key.object, key.mode, key.offset, key.limit, cancel,
                )
            })
            .map_err(|err| err.message)
    })
}

/// Single-pass save through the shim: the C side streams to a sibling temp
/// file (O(n) regardless of stream size) and renames it into place on
/// success, so failures never clobber an existing destination.
pub(crate) fn stream_export_worker(
    session: DocumentSession<OpenDocument>,
    key: &StreamExportKey,
    cancel: &CancelToken,
) -> Result<StreamSaveOutcome, String> {
    session
        .run_task(|document| {
            document.stream_save_with_cancel_token(
                key.object,
                key.mode,
                &key.path,
                STREAM_EXPORT_MAX_BYTES,
                cancel,
            )
        })
        .map_err(|err| err.message)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedXObject {
    pub(crate) object: ObjectId,
    pub(crate) subtype: Option<String>,
    pub(crate) is_image: bool,
}

impl ResolvedXObject {
    pub(crate) fn type_label(&self) -> String {
        match self.subtype.as_deref() {
            Some("Image") => "Image XObject".to_string(),
            Some("Form") => "Form XObject".to_string(),
            Some("PS") => "PostScript XObject".to_string(),
            Some(subtype) => format!("{subtype} XObject"),
            None => "XObject".to_string(),
        }
    }
}

pub(crate) fn dict_entry_summary<'a>(
    detail: &'a ObjectDetail,
    key: &str,
) -> Option<&'a ObjectSummary> {
    detail
        .dictionary_entries
        .as_ref()?
        .items
        .iter()
        .find(|entry| entry.key == key)
        .map(|entry| &entry.value)
}

pub(crate) fn dict_entry_name_value(
    state: &AppState,
    detail: &ObjectDetail,
    key: &str,
) -> Option<String> {
    let summary = dict_entry_summary(detail, key)?;
    let detail = load_object_detail(state, &summary.id).ok()?;
    match detail.value {
        ObjectValue::Name(name) => Some(name),
        _ => None,
    }
}

pub(crate) fn xobject_subtype(detail_state: &AppState, detail: &ObjectDetail) -> Option<String> {
    dict_entry_name_value(detail_state, detail, "Subtype")
}

pub(crate) fn resolve_xobject_resource_from_detail(
    state: &AppState,
    detail: &ObjectDetail,
    resource_name: &str,
) -> Result<ResolvedXObject, String> {
    let resources = dict_entry_summary(detail, "Resources")
        .ok_or_else(|| "has no /Resources dictionary".to_string())?;
    let resources_detail = load_object_detail(state, &resources.id)?;
    let xobjects = dict_entry_summary(&resources_detail, "XObject")
        .ok_or_else(|| "resources have no /XObject dictionary".to_string())?;
    let xobjects_detail = load_object_detail(state, &xobjects.id)?;
    let resource = dict_entry_summary(&xobjects_detail, resource_name)
        .ok_or_else(|| format!("no /{resource_name} entry in /XObject resources"))?;
    let object = resource
        .object
        .or_else(|| resource.id.object_id())
        .ok_or_else(|| format!("/{resource_name} is not an indirect object"))?;
    let detail = load_object_detail(state, &resource.id)?;
    let subtype = xobject_subtype(state, &detail);
    let is_image = detail
        .stream
        .as_ref()
        .is_some_and(|stream| stream.image_preview_available);
    Ok(ResolvedXObject {
        object,
        subtype,
        is_image,
    })
}

impl GuiShellApp {
    pub fn new() -> Self {
        Self::new_with_options(GuiRunOptions::default())
    }

    pub(crate) fn current_ui_settings(&self) -> UiSettings {
        UiSettings {
            dark_mode: dark_mode_enabled(),
            left_panel_width: self.left_panel_width,
            right_panel_width: self.right_panel_width,
            render_zoom: Some(self.render_zoom),
        }
    }

    pub(crate) fn save_ui_settings(&mut self) {
        if let Err(err) = save_ui_settings_to(&self.ui_settings_path, &self.current_ui_settings()) {
            self.status_log
                .push(format!("ui settings save failed: {err}"));
        }
    }

    pub fn new_with_options(options: GuiRunOptions) -> Self {
        let start_empty = options.start_empty_when_no_pdf && options.pdf_path.is_none();
        let state = if start_empty {
            Err("No PDF open".to_string())
        } else {
            open_app_state(options.pdf_path.as_deref(), None)
        };
        let tree = TreeModel::from_state(&state, options.pdf_path.is_some());
        let (real_detail, real_detail_error) = load_initial_real_detail(&state, &tree);
        let (real_pages, real_pages_error) = load_initial_real_pages(&state, &tree);
        let recent_files_path = options
            .recent_files_path
            .clone()
            .unwrap_or_else(default_recent_files_path);
        let mut recent_pdf_paths = load_recent_pdf_paths_from(&recent_files_path);
        let ui_settings_path = ui_settings_path_for(&recent_files_path);
        let ui_settings = load_ui_settings_from(&ui_settings_path);
        set_dark_mode(ui_settings.dark_mode);
        let render_page_index = 0;
        let render_zoom = ui_settings.render_zoom.unwrap_or(DEFAULT_RENDER_ZOOM);
        let render_rotation_degrees = 0;
        let render_max_dimension = render_max_dimension_or_default(options.render_max_dimension);
        let mut status_log = if start_empty {
            empty_status_log()
        } else {
            initial_status_log(&state, &tree, options.pdf_path.as_deref())
        };
        if let Some(pages) = &real_pages {
            status_log.push(format!(
                "loaded page list {}",
                child_page_detail(pages.total, pages.items.len())
            ));
        } else if let Some(err) = &real_pages_error {
            status_log.push(format!("page list load failed: {err}"));
        }
        if state.is_ok() {
            if let Some(path) = options.pdf_path.as_deref() {
                if record_recent_pdf_path(&mut recent_pdf_paths, path) {
                    if let Err(err) =
                        save_recent_pdf_paths_to(&recent_files_path, &recent_pdf_paths)
                    {
                        status_log.push(format!("recent file save failed: {err}"));
                    }
                }
            }
        }
        let smoke_exit_after = options.smoke_exit_after;
        let selected_row = tree.initial_selected_row();
        let mut app = Self {
            state,
            empty_workspace: start_empty,
            launched_at: Instant::now(),
            smoke_exit_after,
            recent_files_path,
            recent_pdf_paths,
            ui_settings_path,
            open_pdf_dialog_open: false,
            open_pdf_path_input: options.pdf_path.unwrap_or_default(),
            open_pdf_password_input: String::new(),
            open_pdf_error: None,
            open_pdf_job: None,
            about_dialog_open: false,
            about_logo_texture: None,
            left_panel_width: ui_settings.left_panel_width,
            right_panel_width: ui_settings.right_panel_width,
            tree,
            stream: LargeStreamModel::default(),
            real_stream_preset: RealStreamPreset::Nice,
            real_stream_mode: StreamMode::Decoded,
            real_stream_view_mode: StreamViewMode::Text,
            real_stream_offset: 0,
            real_stream_limit: REAL_STREAM_DEFAULT_VIEW_LIMIT_BYTES,
            real_stream_key: None,
            real_stream_job: None,
            real_stream_chunk: None,
            real_stream_windows: VecDeque::new(),
            real_stream_collapsed_blocks: HashSet::new(),
            real_stream_selected_block: None,
            scroll_selected_nice_stream_row: false,
            real_stream_error: None,
            decoded_stream_cache: StreamChunkCache::new(
                DECODED_STREAM_CACHE_MAX_ITEMS,
                DECODED_STREAM_CACHE_MAX_BYTES,
            ),
            selected_row,
            scroll_selected_tree_row: false,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            selected_tab: InspectorTab::Object,
            real_detail,
            real_detail_error,
            real_pages,
            real_pages_error,
            render_page_index,
            preview_page_scroll_accumulator: egui::Vec2::ZERO,
            render_zoom,
            render_rotation_degrees,
            render_max_dimension,
            render_limit_dialog_open: false,
            render_limit_gib_input: String::new(),
            render_limit_dialog_error: None,
            real_render_key: None,
            real_render_job: None,
            real_render: None,
            real_render_error: None,
            real_render_texture: None,
            render_cache: RenderResultCache::new(RENDER_CACHE_MAX_ITEMS, RENDER_CACHE_MAX_BYTES),
            object_search_query: String::new(),
            object_search_result: None,
            object_search_error: None,
            object_search_job: None,
            text_search_query: String::new(),
            text_search_result: None,
            text_search_error: None,
            text_search_job: None,
            text_search_cache: TextPageCache::new(
                TEXT_SEARCH_CACHE_MAX_PAGES,
                TEXT_SEARCH_CACHE_MAX_BYTES,
            ),
            visual_page_cache: VisualPageCache::new(
                VISUAL_CLICK_CACHE_MAX_PAGES,
                VISUAL_CLICK_CACHE_MAX_ELEMENTS,
            ),
            selected_text_hit: None,
            selected_visual_hit: None,
            pending_preview_stream_selection: None,
            preview_click: None,
            xref_slice: None,
            xref_error: None,
            xref_offset: 0,
            hex_mode: StreamMode::Raw,
            hex_offset: 0,
            hex_jump_input: String::new(),
            hex_key: None,
            hex_job: None,
            hex_chunk: None,
            hex_error: None,
            image_preview_job: None,
            image_preview_result: None,
            image_preview_error: None,
            image_preview_texture: None,
            stream_export_job: None,
            diagnostic_min_severity: None,
            diagnostic_code_filter: String::new(),
            copied_excerpt: None,
            status_log,
        };
        app.refresh_real_render();
        app
    }

    pub(crate) fn selected_object_label(&self) -> String {
        self.tree.row_label(self.selected_row)
    }

    pub(crate) fn clear_preview_selection(&mut self) {
        self.preview_click = None;
        self.selected_text_hit = None;
        self.selected_visual_hit = None;
        self.pending_preview_stream_selection = None;
    }

    pub(crate) fn select_row_from_tree(&mut self, row: usize) {
        if self.selected_row == row {
            self.clear_preview_selection();
            self.expand_selected_real_tree_path();
            self.sync_render_page_for_tree_row(self.selected_row);
            self.select_visual_bbox_for_tree_row(self.selected_row);
            return;
        }
        self.back_stack.push(self.selected_row);
        self.selected_row = row;
        self.clear_preview_selection();
        self.forward_stack.clear();
        self.refresh_real_detail_for_selection();
        self.expand_selected_real_tree_path();
        self.sync_render_page_for_tree_row(self.selected_row);
        self.select_visual_bbox_for_tree_row(self.selected_row);
        self.status_log.push(format!(
            "selected {}",
            self.tree.row_label(self.selected_row)
        ));
    }

    pub(crate) fn follow_reference(&mut self, row: usize) {
        if self.selected_row == row || row >= self.tree.row_count() {
            return;
        }
        self.back_stack.push(self.selected_row);
        self.forward_stack.clear();
        self.selected_row = row;
        self.clear_preview_selection();
        self.selected_tab = InspectorTab::Object;
        self.refresh_real_detail_for_selection();
        self.expand_selected_real_tree_path();
        self.sync_render_page_for_tree_row(self.selected_row);
        self.select_visual_bbox_for_tree_row(self.selected_row);
        self.status_log.push(format!(
            "resolved reference to {}",
            self.tree.row_label(self.selected_row)
        ));
    }

    pub(crate) fn go_back(&mut self) {
        if let Some(row) = self.back_stack.pop() {
            self.forward_stack.push(self.selected_row);
            self.selected_row = row;
            self.clear_preview_selection();
            self.refresh_real_detail_for_selection();
            self.expand_selected_real_tree_path();
            self.sync_render_page_for_tree_row(self.selected_row);
            self.select_visual_bbox_for_tree_row(self.selected_row);
            self.status_log.push(format!(
                "back to {}",
                self.tree.row_label(self.selected_row)
            ));
        }
    }

    pub(crate) fn go_forward(&mut self) {
        if let Some(row) = self.forward_stack.pop() {
            self.back_stack.push(self.selected_row);
            self.selected_row = row;
            self.clear_preview_selection();
            self.refresh_real_detail_for_selection();
            self.expand_selected_real_tree_path();
            self.sync_render_page_for_tree_row(self.selected_row);
            self.select_visual_bbox_for_tree_row(self.selected_row);
            self.status_log.push(format!(
                "forward to {}",
                self.tree.row_label(self.selected_row)
            ));
        }
    }

    pub(crate) fn follow_real_reference(&mut self, object: ObjectId) {
        let Some(summary) = self
            .state
            .as_ref()
            .ok()
            .and_then(|state| state.panels.summary.as_ref())
        else {
            return;
        };
        let row = self
            .tree
            .ensure_real_object_row(summary.doc.clone(), object);
        self.follow_reference(row);
    }

    pub(crate) fn follow_object_search_hit(&mut self, hit: &ObjectSearchHit) {
        if self.tree.is_real() {
            if let Some(doc) = self
                .state
                .as_ref()
                .ok()
                .and_then(|state| state.panels.summary.as_ref())
                .map(|summary| summary.doc.clone())
            {
                if let Some(row) = self.tree.ensure_real_search_hit_row(doc, hit) {
                    self.follow_reference(row);
                    self.status_log.push(format!(
                        "opened object search hit {}",
                        object_search_hit_summary(hit)
                    ));
                    return;
                }
            }
        } else if let Some(row) = virtual_search_hit_row(hit, self.tree.row_count()) {
            self.follow_reference(row);
            self.status_log.push(format!(
                "opened object search hit {}",
                object_search_hit_summary(hit)
            ));
            return;
        }

        self.status_log.push(format!(
            "object search hit is not navigable: {}",
            object_search_hit_summary(hit)
        ));
    }

    pub(crate) fn run_object_search(&mut self) {
        let query = self.object_search_query.trim().to_string();
        if query.is_empty() {
            self.cancel_object_search_job();
            self.object_search_result = None;
            self.object_search_error = None;
            return;
        }

        self.object_search_job = None;

        let Ok(state) = self.state.as_ref() else {
            self.object_search_result = None;
            self.object_search_error = Some("document is not open".to_string());
            return;
        };
        let request = ObjectSearchRequest {
            query: query.clone(),
            root: None,
            child_page_size: OBJECT_SEARCH_CHILD_PAGE_SIZE,
            max_child_pages_per_node: OBJECT_SEARCH_MAX_CHILD_PAGES,
            max_depth: OBJECT_SEARCH_MAX_DEPTH,
            max_nodes: OBJECT_SEARCH_MAX_NODES,
            max_results: OBJECT_SEARCH_MAX_RESULTS,
            inspect_details: false,
        };
        let session = state.session.clone();
        self.object_search_job = Some(BackgroundJob::spawn(query.clone(), move |cancel| {
            session
                .run_task(|document| search_objects_with_cancel(document, &request, cancel))
                .map_err(|err| err.message)
        }));
        self.object_search_result = None;
        self.object_search_error = None;
        self.status_log
            .push(format!("queued object search {query:?}"));
    }

    pub(crate) fn cancel_object_search_job(&mut self) {
        if let Some(job) = self.object_search_job.take() {
            self.object_search_error = Some("object search cancelled".to_string());
            self.status_log
                .push(format!("cancelled object search {:?}", job.key()));
        }
    }

    pub(crate) fn poll_object_search_job(&mut self) {
        match self.object_search_job.as_ref().map(BackgroundJob::poll) {
            None | Some(JobPoll::Pending) => {}
            Some(JobPoll::Finished(output)) => {
                self.object_search_job = None;
                match output.result {
                    Ok(result) => {
                        self.status_log.push(format!(
                            "object search {:?}: {} hits across {} nodes{}",
                            output.key,
                            result.hits.len(),
                            result.searched_nodes,
                            if result.truncated { " (truncated)" } else { "" }
                        ));
                        self.object_search_result = Some(result);
                        self.object_search_error = None;
                    }
                    Err(err) => {
                        self.object_search_result = None;
                        self.object_search_error = Some(err.clone());
                        self.status_log
                            .push(format!("object search {:?} failed: {err}", output.key));
                    }
                }
            }
            Some(JobPoll::Disconnected(query)) => {
                self.object_search_job = None;
                self.object_search_result = None;
                self.object_search_error = Some("object search worker disconnected".to_string());
                self.status_log
                    .push(format!("object search {query:?} worker disconnected"));
            }
        }
    }

    pub(crate) fn start_text_search(&mut self) {
        let query = self.text_search_query.trim().to_string();
        if query.is_empty() {
            self.cancel_text_search_job();
            self.text_search_result = None;
            self.text_search_error = None;
            self.selected_text_hit = None;
            self.selected_visual_hit = None;
            return;
        }

        self.text_search_job = None;

        let page_count = self.page_count();
        if page_count == 0 {
            self.text_search_result = None;
            self.text_search_error = Some("document has no pages".to_string());
            return;
        }

        let Ok(state) = self.state.as_ref() else {
            self.text_search_result = None;
            self.text_search_error = Some("document is not open".to_string());
            return;
        };

        let request = TextSearchRequest {
            query: query.clone(),
            max_pages: TEXT_SEARCH_MAX_PAGES,
            max_results: TEXT_SEARCH_MAX_RESULTS,
            max_chars_per_page: TEXT_SEARCH_MAX_CHARS_PER_PAGE,
            max_blocks_per_page: TEXT_SEARCH_MAX_BLOCKS_PER_PAGE,
            ..TextSearchRequest::new(query.clone())
        };
        let session = state.session.clone();
        let mut cache = self.text_search_cache.clone();
        self.text_search_job = Some(BackgroundJob::spawn(query.clone(), move |cancel| {
            session
                .run_task(|document| {
                    search_text_with_cache(page_count, &mut cache, &request, |text_request| {
                        document.extract_text_with_cancel_token(text_request, cancel)
                    })
                })
                .map(|result| (result, cache))
                .map_err(|err| err.message)
        }));
        self.text_search_result = None;
        self.text_search_error = None;
        self.selected_text_hit = None;
        self.selected_visual_hit = None;
        self.status_log.push(format!(
            "queued text search {query:?} across up to {} pages",
            page_count.min(TEXT_SEARCH_MAX_PAGES)
        ));
    }

    pub(crate) fn cancel_text_search_job(&mut self) {
        if let Some(job) = self.text_search_job.take() {
            self.text_search_error = Some("text search cancelled".to_string());
            self.status_log
                .push(format!("cancelled text search {:?}", job.key()));
        }
    }

    pub(crate) fn poll_text_search_job(&mut self) {
        match self.text_search_job.as_ref().map(BackgroundJob::poll) {
            None | Some(JobPoll::Pending) => {}
            Some(JobPoll::Finished(output)) => {
                self.text_search_job = None;
                match output.result {
                    Ok((result, cache)) => {
                        self.status_log.push(format!(
                            "text search {:?}: {} hits across {} pages{}",
                            output.key,
                            result.hits.len(),
                            result.searched_pages,
                            if result.truncated { " (truncated)" } else { "" }
                        ));
                        self.text_search_cache = cache;
                        self.text_search_result = Some(result);
                        self.text_search_error = None;
                    }
                    Err(err) => {
                        self.text_search_result = None;
                        self.text_search_error = Some(err.clone());
                        self.status_log
                            .push(format!("text search {:?} failed: {err}", output.key));
                    }
                }
            }
            Some(JobPoll::Disconnected(query)) => {
                self.text_search_job = None;
                self.text_search_result = None;
                self.text_search_error = Some("text search worker disconnected".to_string());
                self.status_log
                    .push(format!("text search {query:?} worker disconnected"));
            }
        }
    }

    pub(crate) fn follow_text_search_hit(&mut self, hit: &TextSearchHit) {
        self.set_render_page(hit.page_index);
        self.selected_text_hit = Some(hit.clone());
        self.selected_visual_hit = None;
        self.status_log.push(format!(
            "opened text search hit page {} span {}",
            hit.page_index + 1,
            hit.span_index
        ));
    }

    pub(crate) fn expand_selected_real_row(&mut self) -> usize {
        let inserted = self.expand_real_tree_row(self.selected_row);
        if inserted > 0 {
            self.status_log.push(format!(
                "expanded {} bounded children under {}",
                inserted,
                self.tree.row_label(self.selected_row)
            ));
        }
        inserted
    }

    pub(crate) fn expand_real_tree_row(&mut self, row: usize) -> usize {
        let root_inserted = match &mut self.tree {
            TreeModel::Real(tree) => tree.expand_cached_document_root(row),
            TreeModel::Virtual(_) => 0,
        };
        if root_inserted > 0 {
            return root_inserted;
        }

        let Some(id) = (match &self.tree {
            TreeModel::Real(tree) => tree.summary(row).map(|summary| summary.id.clone()),
            TreeModel::Virtual(_) => None,
        }) else {
            return 0;
        };
        let Ok(state) = &self.state else {
            return 0;
        };
        let detail = match load_object_detail(state, &id) {
            Ok(detail) => detail,
            Err(err) => {
                if self.selected_row == row {
                    self.real_detail = None;
                    self.real_detail_error = Some(err.clone());
                }
                self.status_log
                    .push(format!("expand {} failed: {err}", self.tree.row_label(row)));
                return 0;
            }
        };
        let inserted = match &mut self.tree {
            TreeModel::Real(tree) => {
                tree.update_row_from_detail(row, &detail);
                tree.expand_row_from_detail(row, &detail)
            }
            TreeModel::Virtual(_) => 0,
        };
        if self.selected_row == row {
            self.real_detail = Some(detail);
            self.real_detail_error = None;
        }
        inserted
    }

    pub(crate) fn expand_selected_real_tree_path(&mut self) {
        let selected_id = match &self.tree {
            TreeModel::Real(tree) => tree
                .summary(self.selected_row)
                .map(|summary| summary.id.clone()),
            TreeModel::Virtual(_) => None,
        };
        let Some(selected_id) = selected_id else {
            return;
        };

        self.expand_real_tree_row(self.selected_row);
        if let TreeModel::Real(tree) = &mut self.tree {
            if let Some(row) = tree.collapse_expanded_rows_except_selected_path(&selected_id) {
                self.selected_row = row;
            }
        }
    }

    pub(crate) fn refresh_real_detail_for_selection(&mut self) {
        self.clear_real_stream_chunk();
        self.reset_hex_state();
        let TreeModel::Real(tree) = &self.tree else {
            return;
        };
        let Some(summary) = tree.summary(self.selected_row) else {
            self.real_detail = None;
            self.real_detail_error = None;
            return;
        };
        match self.state.as_ref() {
            Ok(state) => match load_object_detail(state, &summary.id) {
                Ok(detail) => {
                    if let TreeModel::Real(tree) = &mut self.tree {
                        tree.update_row_from_detail(self.selected_row, &detail);
                    }
                    self.real_detail = Some(detail);
                    self.real_detail_error = None;
                }
                Err(err) => {
                    self.real_detail = None;
                    self.real_detail_error = Some(err);
                }
            },
            Err(err) => {
                self.real_detail = None;
                self.real_detail_error = Some(err.clone());
            }
        }
    }

    pub(crate) fn clear_real_stream_chunk(&mut self) {
        self.real_stream_job = None;
        self.real_stream_key = None;
        self.real_stream_chunk = None;
        self.real_stream_windows.clear();
        self.real_stream_collapsed_blocks.clear();
        self.real_stream_selected_block = None;
        self.scroll_selected_nice_stream_row = false;
        self.real_stream_error = None;
    }

    pub(crate) fn refresh_real_stream_chunk(&mut self, object: ObjectId) {
        let key = RealStreamKey {
            object,
            mode: self.real_stream_mode,
            offset: self.real_stream_offset,
            limit: self.real_stream_limit,
        };
        if self
            .real_stream_job
            .as_ref()
            .is_some_and(|job| *job.key() == key)
        {
            return;
        }
        if let Some(chunk) = self.real_stream_cached_window(key) {
            self.real_stream_key = Some(key);
            self.real_stream_chunk = Some(chunk);
            self.real_stream_error = None;
            return;
        }
        if self.real_stream_key == Some(key) && self.real_stream_job.is_some() {
            return;
        }
        self.real_stream_job = None;
        self.real_stream_key = Some(key);
        if self.real_stream_windows.is_empty() {
            self.real_stream_chunk = None;
        }
        self.real_stream_error = None;

        if key.mode == StreamMode::Decoded {
            if let Some(chunk) = self.decoded_stream_cache.get(&key) {
                self.insert_real_stream_window(key, chunk.clone());
                self.real_stream_chunk = Some(chunk);
                self.status_log.push(format!(
                    "reused cached decoded stream chunk {} {} R @ {}",
                    key.object.num, key.object.gen, key.offset
                ));
                return;
            }
        }

        let Ok(state) = self.state.as_ref() else {
            self.real_stream_error = Some("document is not open".to_string());
            return;
        };
        self.real_stream_job = Some(spawn_stream_load_job(state, key));
        self.status_log.push(format!(
            "queued {} stream chunk {} {} R @ {}",
            stream_mode_label(key.mode),
            object.num,
            object.gen,
            key.offset
        ));
    }

    pub(crate) fn cancel_real_stream_job(&mut self) {
        if let Some(job) = self.real_stream_job.take() {
            let key = *job.key();
            self.real_stream_chunk = None;
            self.real_stream_error = Some("stream chunk load cancelled".to_string());
            self.status_log.push(format!(
                "cancelled {} stream chunk {} {} R @ {}",
                stream_mode_label(key.mode),
                key.object.num,
                key.object.gen,
                key.offset
            ));
        }
    }

    pub(crate) fn real_stream_cached_window(&self, key: RealStreamKey) -> Option<StreamChunk> {
        self.real_stream_windows
            .iter()
            .find(|window| window.key == key)
            .map(|window| window.chunk.clone())
    }

    pub(crate) fn insert_real_stream_window(&mut self, key: RealStreamKey, chunk: StreamChunk) {
        if let Some(window) = self
            .real_stream_windows
            .iter_mut()
            .find(|window| window.key == key)
        {
            window.chunk = chunk;
        } else {
            self.real_stream_windows
                .push_back(RealStreamLoadedWindow { key, chunk });
        }
        self.real_stream_windows
            .make_contiguous()
            .sort_by_key(|window| window.key.offset);
        while self.real_stream_windows.len() > REAL_STREAM_MAX_LOADED_WINDOWS {
            let front_distance = self
                .real_stream_windows
                .front()
                .map(|window| key.offset.saturating_sub(window.key.offset))
                .unwrap_or(0);
            let back_distance = self
                .real_stream_windows
                .back()
                .map(|window| window.key.offset.saturating_sub(key.offset))
                .unwrap_or(0);
            if front_distance > back_distance {
                self.real_stream_windows.pop_front();
            } else {
                self.real_stream_windows.pop_back();
            }
        }
    }

    pub(crate) fn apply_real_stream_preset(&mut self, stream: &StreamSummary) -> bool {
        let (mode, view_mode) =
            real_stream_preset_defaults(self.real_stream_preset, stream.can_decode);
        let limit = real_stream_default_limit(stream, mode);
        let mut changed = false;
        if self.real_stream_mode != mode {
            self.real_stream_mode = mode;
            changed = true;
        }
        if self.real_stream_view_mode != view_mode {
            self.real_stream_view_mode = view_mode;
            changed = true;
        }
        if self.real_stream_offset != 0 {
            self.real_stream_offset = 0;
            changed = true;
        }
        if self.real_stream_limit != limit {
            self.real_stream_limit = limit;
            changed = true;
        }
        changed
    }

    pub(crate) fn poll_real_stream_job(&mut self) {
        match self.real_stream_job.as_ref().map(BackgroundJob::poll) {
            None | Some(JobPoll::Pending) => {}
            Some(JobPoll::Finished(output)) => {
                self.real_stream_job = None;
                if self.real_stream_key != Some(output.key) {
                    self.status_log.push(format!(
                        "discarded stale {} stream chunk {} {} R @ {}",
                        stream_mode_label(output.key.mode),
                        output.key.object.num,
                        output.key.object.gen,
                        output.key.offset
                    ));
                    return;
                }
                match output.result {
                    Ok(chunk) => {
                        self.status_log.push(format!(
                            "loaded {} stream chunk {} {} R @ {} ({} bytes{})",
                            stream_mode_label(chunk.mode),
                            output.key.object.num,
                            output.key.object.gen,
                            chunk.offset,
                            chunk.bytes.len(),
                            if chunk.truncated { ", truncated" } else { "" }
                        ));
                        if output.key.mode == StreamMode::Decoded {
                            self.decoded_stream_cache.insert(output.key, chunk.clone());
                        }
                        self.insert_real_stream_window(output.key, chunk.clone());
                        self.real_stream_chunk = Some(chunk);
                        self.real_stream_error = None;
                        if self.pending_preview_stream_selection.is_some() {
                            self.select_nice_stream_code_for_preview_selection();
                        }
                    }
                    Err(err) => {
                        self.real_stream_chunk = None;
                        self.real_stream_error = Some(err.clone());
                        self.status_log.push(format!(
                            "{} stream chunk {} {} R failed: {err}",
                            stream_mode_label(output.key.mode),
                            output.key.object.num,
                            output.key.object.gen
                        ));
                    }
                }
            }
            Some(JobPoll::Disconnected(key)) => {
                self.real_stream_job = None;
                if self.real_stream_key == Some(key) {
                    self.real_stream_chunk = None;
                    self.real_stream_error = Some("stream worker disconnected".to_string());
                }
            }
        }
    }

    pub(crate) fn ensure_hex_chunk(&mut self, stream: &StreamSummary) {
        let object = stream.object;
        if self.hex_key.is_some_and(|key| key.object != object) {
            self.reset_hex_state();
        }
        if !stream.can_decode && self.hex_mode == StreamMode::Decoded {
            self.hex_mode = StreamMode::Raw;
            self.clear_hex_chunk();
        }
        let key = RealStreamKey {
            object,
            mode: self.hex_mode,
            offset: self.hex_offset,
            limit: HEX_VIEW_WINDOW_BYTES,
        };
        if self.hex_key == Some(key)
            && (self.hex_chunk.is_some() || self.hex_job.is_some() || self.hex_error.is_some())
        {
            return;
        }
        self.hex_key = Some(key);
        self.hex_chunk = None;
        self.hex_error = None;
        self.hex_job = None;

        if key.mode == StreamMode::Decoded {
            if let Some(chunk) = self.decoded_stream_cache.get(&key) {
                self.hex_chunk = Some(chunk);
                return;
            }
        }

        let Ok(state) = self.state.as_ref() else {
            self.hex_error = Some("document is not open".to_string());
            return;
        };
        self.hex_job = Some(spawn_stream_load_job(state, key));
    }

    pub(crate) fn poll_hex_job(&mut self) {
        match self.hex_job.as_ref().map(BackgroundJob::poll) {
            None | Some(JobPoll::Pending) => {}
            Some(JobPoll::Finished(output)) => {
                self.hex_job = None;
                if self.hex_key != Some(output.key) {
                    return;
                }
                match output.result {
                    Ok(chunk) => {
                        if output.key.mode == StreamMode::Decoded {
                            self.decoded_stream_cache.insert(output.key, chunk.clone());
                        }
                        self.hex_chunk = Some(chunk);
                        self.hex_error = None;
                    }
                    Err(err) => {
                        self.hex_chunk = None;
                        self.hex_error = Some(err);
                    }
                }
            }
            Some(JobPoll::Disconnected(key)) => {
                self.hex_job = None;
                if self.hex_key == Some(key) {
                    self.hex_chunk = None;
                    self.hex_error = Some("hex view worker disconnected".to_string());
                }
            }
        }
    }

    pub(crate) fn clear_hex_chunk(&mut self) {
        self.hex_key = None;
        self.hex_chunk = None;
        self.hex_error = None;
        self.hex_job = None;
    }

    pub(crate) fn reset_hex_state(&mut self) {
        self.hex_mode = StreamMode::Raw;
        self.hex_offset = 0;
        self.hex_jump_input.clear();
        self.clear_hex_chunk();
    }

    pub(crate) fn set_hex_offset(&mut self, offset: u64) {
        let aligned = offset - offset % HEX_VIEW_BYTES_PER_ROW as u64;
        self.hex_offset = aligned;
        self.clear_hex_chunk();
    }

    pub(crate) fn ensure_image_preview(&mut self, object: ObjectId) {
        let current = self
            .image_preview_result
            .as_ref()
            .map(|(result_object, _)| *result_object)
            .or_else(|| {
                self.image_preview_error
                    .as_ref()
                    .map(|(error_object, _)| *error_object)
            })
            .or_else(|| self.image_preview_job.as_ref().map(|job| *job.key()));
        if current == Some(object) {
            return;
        }

        self.image_preview_job = None;
        self.image_preview_result = None;
        self.image_preview_error = None;
        self.image_preview_texture = None;

        let Ok(state) = self.state.as_ref() else {
            self.image_preview_error = Some((object, "document is not open".to_string()));
            return;
        };
        let session = state.session.clone();
        self.image_preview_job = Some(BackgroundJob::spawn(object, move |cancel| {
            session
                .run_task(|document| {
                    document.image_preview_with_cancel_token(
                        object,
                        IMAGE_PREVIEW_MAX_DIMENSION,
                        cancel,
                    )
                })
                .map_err(|err| err.message)
        }));
    }

    pub(crate) fn poll_image_preview_job(&mut self) {
        match self.image_preview_job.as_ref().map(BackgroundJob::poll) {
            None | Some(JobPoll::Pending) => {}
            Some(JobPoll::Finished(output)) => {
                self.image_preview_job = None;
                match output.result {
                    Ok(preview) => {
                        self.image_preview_result = Some((output.key, preview));
                        self.image_preview_error = None;
                    }
                    Err(err) => {
                        self.image_preview_result = None;
                        self.image_preview_error = Some((output.key, err));
                    }
                }
            }
            Some(JobPoll::Disconnected(object)) => {
                self.image_preview_job = None;
                self.image_preview_error =
                    Some((object, "image preview worker disconnected".to_string()));
            }
        }
    }

    pub(crate) fn clear_image_preview(&mut self) {
        self.image_preview_job = None;
        self.image_preview_result = None;
        self.image_preview_error = None;
        self.image_preview_texture = None;
    }

    pub(crate) fn start_stream_export(&mut self, object: ObjectId, mode: StreamMode, path: String) {
        if let Some(job) = &self.stream_export_job {
            self.status_log.push(format!(
                "export already running for {} {} R",
                job.key().object.num,
                job.key().object.gen
            ));
            return;
        }
        let Ok(state) = self.state.as_ref() else {
            self.status_log
                .push("export failed: document is not open".to_string());
            return;
        };
        let key = StreamExportKey { object, mode, path };
        let session = state.session.clone();
        let worker_key = key.clone();
        self.stream_export_job = Some(BackgroundJob::spawn(key.clone(), move |cancel| {
            stream_export_worker(session, &worker_key, cancel)
        }));
        self.status_log.push(format!(
            "exporting {} stream {} {} R to {}",
            stream_mode_label(mode),
            object.num,
            object.gen,
            display_file_chip_label(&key.path)
        ));
    }

    pub(crate) fn cancel_stream_export_job(&mut self) {
        if let Some(job) = self.stream_export_job.take() {
            self.status_log.push(format!(
                "cancelled export of {} {} R",
                job.key().object.num,
                job.key().object.gen
            ));
        }
    }

    pub(crate) fn poll_stream_export_job(&mut self) {
        match self.stream_export_job.as_ref().map(BackgroundJob::poll) {
            None | Some(JobPoll::Pending) => {}
            Some(JobPoll::Finished(output)) => {
                self.stream_export_job = None;
                match output.result {
                    Ok(outcome) => {
                        self.status_log.push(format!(
                            "exported {} bytes of {} {} R to {}{}",
                            outcome.bytes_written,
                            output.key.object.num,
                            output.key.object.gen,
                            display_file_chip_label(&output.key.path),
                            if outcome.capped {
                                " (stopped at size cap)"
                            } else {
                                ""
                            }
                        ));
                    }
                    Err(err) => {
                        self.status_log.push(format!(
                            "export of {} {} R failed: {err} (destination left unchanged)",
                            output.key.object.num, output.key.object.gen
                        ));
                    }
                }
            }
            Some(JobPoll::Disconnected(key)) => {
                self.stream_export_job = None;
                self.status_log.push(format!(
                    "export of {} {} R worker disconnected",
                    key.object.num, key.object.gen
                ));
            }
        }
    }

    pub(crate) fn ensure_xref_slice(&mut self) {
        if self.xref_slice.is_some() || self.xref_error.is_some() {
            return;
        }
        let offset = self.xref_offset;
        let result = match &self.state {
            Ok(state) => state
                .session
                .run_task(|document| {
                    document.xref_table(ChildRange {
                        offset,
                        limit: XREF_PAGE_SIZE,
                    })
                })
                .map_err(|err| err.message),
            Err(err) => Err(err.clone()),
        };
        match result {
            Ok(slice) => {
                self.xref_slice = Some(slice);
                self.xref_error = None;
            }
            Err(err) => {
                self.xref_error = Some(err);
            }
        }
    }

    pub(crate) fn set_xref_offset(&mut self, offset: usize) {
        self.xref_offset = offset;
        self.xref_slice = None;
        self.xref_error = None;
    }

    pub(crate) fn clear_xref_state(&mut self) {
        self.xref_slice = None;
        self.xref_error = None;
        self.xref_offset = 0;
    }

    pub(crate) fn page_count(&self) -> usize {
        self.state
            .as_ref()
            .ok()
            .and_then(|state| state.panels.summary.as_ref())
            .map(|summary| summary.page_count)
            .unwrap_or(0)
    }

    pub(crate) fn sync_render_page_for_tree_row(&mut self, row: usize) {
        let Some(page_index) = self.tree.real_row_page_index(row) else {
            return;
        };
        self.set_render_page(page_index);
    }

    pub(crate) fn current_render_key(&self) -> Option<RealRenderKey> {
        if !self.tree.is_real() || self.page_count() == 0 {
            return None;
        }
        Some(RealRenderKey::new(
            self.render_page_index,
            self.render_zoom,
            self.render_rotation_degrees,
            self.render_max_dimension,
        ))
    }

    pub(crate) fn current_render_page_height(&self, page_index: usize) -> Option<f32> {
        let render = self.real_render.as_ref()?;
        if render.page_index != page_index || self.render_zoom <= 0.0 {
            return None;
        }
        Some(render.height as f32 / self.render_zoom)
    }

    pub(crate) fn set_render_page(&mut self, page_index: usize) {
        let page_count = self.page_count();
        if page_count == 0 {
            return;
        }
        let page_index = page_index.min(page_count - 1);
        if self.render_page_index == page_index {
            return;
        }
        self.render_page_index = page_index;
        self.clear_preview_selection();
        self.refresh_real_render();
    }

    pub(crate) fn set_render_page_from_pager(&mut self, page_index: usize) {
        self.set_render_page(page_index);
        let page_index = self.render_page_index;
        self.sync_tree_to_render_page(page_index);
    }

    pub(crate) fn handle_page_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        let page_count = self.page_count();
        if page_count == 0 || ctx.egui_wants_keyboard_input() {
            return;
        }

        let shortcut = ctx.input(|input| {
            if input.modifiers.any() {
                return None;
            }
            if input.key_pressed(egui::Key::ArrowLeft) {
                Some(PageKeyboardShortcut::Previous)
            } else if input.key_pressed(egui::Key::ArrowRight) {
                Some(PageKeyboardShortcut::Next)
            } else if input.key_pressed(egui::Key::ArrowUp) {
                Some(PageKeyboardShortcut::First)
            } else if input.key_pressed(egui::Key::ArrowDown) {
                Some(PageKeyboardShortcut::Last)
            } else if input.key_pressed(egui::Key::PageUp) {
                Some(PageKeyboardShortcut::Previous)
            } else if input.key_pressed(egui::Key::PageDown) {
                Some(PageKeyboardShortcut::Next)
            } else if input.key_pressed(egui::Key::Home) {
                Some(PageKeyboardShortcut::First)
            } else if input.key_pressed(egui::Key::End) {
                Some(PageKeyboardShortcut::Last)
            } else {
                None
            }
        });

        if let Some(page_index) = shortcut
            .and_then(|shortcut| {
                page_keyboard_target_page(self.render_page_index, page_count, shortcut)
            })
            .filter(|page_index| *page_index != self.render_page_index)
        {
            self.set_render_page_from_pager(page_index);
            self.status_log.push(format!(
                "keyboard switched preview to page {}",
                self.render_page_index + 1
            ));
        }
    }

    pub(crate) fn handle_preview_page_scroll_delta(&mut self, delta: egui::Vec2) {
        let page_count = self.page_count();
        if page_count == 0 {
            self.preview_page_scroll_accumulator = egui::Vec2::ZERO;
            return;
        }

        let Some(page_index) = preview_scroll_target_page(
            self.render_page_index,
            page_count,
            &mut self.preview_page_scroll_accumulator,
            delta,
        )
        .filter(|page_index| *page_index != self.render_page_index) else {
            return;
        };

        self.set_render_page_from_pager(page_index);
        self.status_log.push(format!(
            "scroll gesture switched preview to page {}",
            self.render_page_index + 1
        ));
    }

    pub(crate) fn sync_tree_to_render_page(&mut self, page_index: usize) -> Option<usize> {
        if !self.tree.is_real() {
            return None;
        }
        let row = self.ensure_tree_page_row(page_index)?;
        self.select_row_from_tree(row);
        self.scroll_selected_tree_row = true;
        self.status_log
            .push(format!("synced document tree to page {}", page_index + 1));
        Some(row)
    }

    pub(crate) fn ensure_tree_page_row(&mut self, page_index: usize) -> Option<usize> {
        if let Some(row) = self.tree.real_row_for_page_index(page_index) {
            return Some(row);
        }

        let page_root_row = self.tree.real_page_root_row()?;
        self.expand_real_tree_row(page_root_row);
        if let Some(row) = self.tree.real_row_for_page_index(page_index) {
            return Some(row);
        }

        let summary = self.load_page_summary_for_tree(page_root_row, page_index)?;
        self.tree.ensure_real_page_child_row(page_root_row, summary)
    }

    pub(crate) fn ensure_page_content_stream_row(&mut self, page_index: usize) -> Option<usize> {
        let page_row = self.ensure_tree_page_row(page_index)?;
        self.expand_real_tree_row(page_row);
        for _ in 0..4 {
            if let Some(row) = self.tree.real_first_page_content_stream_row(page_row) {
                return Some(row);
            }
            let candidates = self.tree.real_page_content_candidate_rows(page_row);
            let mut expanded_any = false;
            for row in candidates {
                if self.expand_real_tree_row(row) > 0 {
                    expanded_any = true;
                    break;
                }
            }
            if !expanded_any {
                break;
            }
        }
        None
    }

    pub(crate) fn load_page_summary_for_tree(
        &mut self,
        page_root_row: usize,
        page_index: usize,
    ) -> Option<ObjectSummary> {
        let page_root_id = match &self.tree {
            TreeModel::Real(tree) => tree
                .summary(page_root_row)
                .map(|summary| summary.id.clone()),
            TreeModel::Virtual(_) => None,
        }?;
        let Ok(state) = self.state.as_ref() else {
            return None;
        };
        match state.session.run_task(|document| {
            document.children(
                &page_root_id,
                ChildRange {
                    offset: page_index,
                    limit: 1,
                },
                ChildContainer::Array,
            )
        }) {
            Ok(page) => page.items.into_iter().next(),
            Err(err) => {
                self.status_log.push(format!(
                    "page {} tree row load failed: {}",
                    page_index + 1,
                    err.message
                ));
                None
            }
        }
    }

    pub(crate) fn select_xobject_resource_from_stream_context(
        &mut self,
        stream_object: ObjectId,
        resource_name: &str,
    ) {
        let page_index = self
            .tree
            .real_row_page_index(self.selected_row)
            .unwrap_or(self.render_page_index);
        match self.resolve_stream_xobject_resource(stream_object, Some(page_index), resource_name) {
            Ok((resolved, source)) => {
                let Some(doc) = self
                    .state
                    .as_ref()
                    .ok()
                    .and_then(|state| state.panels.summary.as_ref())
                    .map(|summary| summary.doc.clone())
                else {
                    self.status_log.push(format!(
                        "resolved /{resource_name} but document is unavailable"
                    ));
                    return;
                };
                let row = self.tree.ensure_real_object_row(doc, resolved.object);
                self.select_row_from_tree(row);
                self.scroll_selected_tree_row = true;
                self.selected_tab = InspectorTab::Stream;
                self.status_log.push(format!(
                    "selected /{} {} {} from {}",
                    resource_name,
                    resolved.type_label(),
                    object_ref_text(resolved.object),
                    source,
                ));
            }
            Err(err) => {
                self.status_log.push(format!(
                    "cannot resolve /{} near stream {}: {err}",
                    resource_name,
                    object_ref_text(stream_object)
                ));
            }
        }
    }

    pub(crate) fn resolve_stream_xobject_resource(
        &mut self,
        stream_object: ObjectId,
        fallback_page_index: Option<usize>,
        resource_name: &str,
    ) -> Result<(ResolvedXObject, String), String> {
        let state = self
            .state
            .as_ref()
            .map_err(|err| format!("document is not open: {err}"))?;
        let doc = state
            .panels
            .summary
            .as_ref()
            .map(|summary| summary.doc.clone())
            .ok_or_else(|| "document summary is unavailable".to_string())?;

        let stream_id = NodeId::XrefObject {
            doc: doc.clone(),
            object: stream_object,
        };
        let stream_result = load_object_detail(state, &stream_id)
            .map_err(|err| err.to_string())
            .and_then(|detail| resolve_xobject_resource_from_detail(state, &detail, resource_name));
        if let Ok(resolved) = stream_result {
            return Ok((
                resolved,
                format!("stream {}", object_ref_text(stream_object)),
            ));
        }

        if let Some(page_index) = fallback_page_index {
            match self.resolve_page_xobject_resource(page_index, resource_name) {
                Ok(resolved) => {
                    return Ok((resolved, format!("page {}", page_index + 1)));
                }
                Err(page_err) => {
                    return Err(format!(
                        "stream resources: {}; page {} resources: {}",
                        stream_result.unwrap_err(),
                        page_index + 1,
                        page_err
                    ));
                }
            }
        }

        Err(format!("stream resources: {}", stream_result.unwrap_err()))
    }

    pub(crate) fn resolve_page_xobject_resource(
        &mut self,
        page_index: usize,
        resource_name: &str,
    ) -> Result<ResolvedXObject, String> {
        let page_row = self
            .ensure_tree_page_row(page_index)
            .ok_or_else(|| format!("page {} tree row is unavailable", page_index + 1))?;
        let page_id = match &self.tree {
            TreeModel::Real(tree) => tree
                .summary(page_row)
                .map(|summary| summary.id.clone())
                .ok_or_else(|| format!("page {} tree summary is unavailable", page_index + 1))?,
            TreeModel::Virtual(_) => {
                return Err("document tree is not backed by real PDF data".to_string());
            }
        };
        let state = self
            .state
            .as_ref()
            .map_err(|err| format!("document is not open: {err}"))?;
        let page_detail = load_object_detail(state, &page_id)?;
        resolve_xobject_resource_from_detail(state, &page_detail, resource_name)
            .map_err(|err| format!("page {page_index} {err}"))
    }

    pub(crate) fn select_text_hit_for_preview_click(&mut self, click: PagePreviewClick) {
        self.selected_text_hit = None;
        if self.render_rotation_degrees != 0 {
            self.status_log
                .push("preview text bbox hit-test is only available at 0 deg rotation".to_string());
            return;
        }

        let page = match self.text_page_for_preview(click.page_index) {
            Ok(page) => page,
            Err(err) => {
                self.status_log.push(format!(
                    "preview text bbox hit-test page {} failed: {err}",
                    click.page_index + 1
                ));
                return;
            }
        };

        if let Some(hit) = text_hit_from_page_click(&page, click, self.render_zoom) {
            self.status_log.push(format!(
                "selected text bbox page {} span {}",
                hit.page_index + 1,
                hit.span_index
            ));
            self.selected_text_hit = Some(hit);
        } else {
            self.status_log.push(format!(
                "no text bbox at page {} preview click",
                click.page_index + 1
            ));
        }
    }

    pub(crate) fn select_nice_stream_selection_for_preview(
        &mut self,
        object: ObjectId,
        selection_key: &str,
        rows: &[NiceStreamRenderLine],
    ) {
        self.selected_text_hit = None;
        self.selected_visual_hit = None;
        if self.render_rotation_degrees != 0 {
            self.status_log
                .push("nice stream bbox highlight is only available at 0 deg rotation".to_string());
            return;
        }

        let page_index = self.render_page_index;
        let fragments = nice_stream_text_fragments_for_selection(rows, selection_key);
        let mut highlighted_text = false;
        if !fragments.is_empty() {
            match self.text_page_for_preview(page_index) {
                Ok(page) => {
                    if let Some(hit) = text_hit_for_text_fragments(&page, &fragments) {
                        self.status_log.push(format!(
                            "highlighted page {} text bbox from nice stream",
                            page_index + 1
                        ));
                        self.selected_text_hit = Some(hit);
                        highlighted_text = true;
                    } else {
                        self.status_log.push(format!(
                            "no positioned text bbox for selected nice stream text on page {}",
                            page_index + 1
                        ));
                    }
                }
                Err(err) => {
                    self.status_log.push(format!(
                        "nice stream text bbox page {} failed: {err}",
                        page_index + 1
                    ));
                }
            }
        }

        let needs_visual_highlight =
            !highlighted_text || nice_stream_selection_has_non_text_visual_ops(rows, selection_key);
        if needs_visual_highlight {
            let page_height = self.current_render_page_height(page_index);
            match self.visual_page_for_preview(page_index) {
                Ok(page) => {
                    if let Some(hit) = nice_stream_visual_hit_for_selection(
                        &page,
                        rows,
                        selection_key,
                        object,
                        page_height,
                    ) {
                        self.status_log.push(format!(
                            "highlighted page {} visual bbox from nice stream",
                            page_index + 1
                        ));
                        self.selected_visual_hit = Some(hit);
                    } else if !highlighted_text {
                        self.status_log.push(format!(
                            "no visual bbox for selected nice stream object on page {}",
                            page_index + 1
                        ));
                    }
                }
                Err(err) => {
                    self.status_log.push(format!(
                        "nice stream visual bbox page {} failed: {err}",
                        page_index + 1
                    ));
                }
            }
        }

        if !highlighted_text && self.selected_visual_hit.is_none() && fragments.is_empty() {
            self.status_log
                .push("selected nice stream line has no previewable drawing operation".to_string());
        }
    }

    pub(crate) fn select_visual_hit_for_preview_click(&mut self, click: PagePreviewClick) {
        self.selected_visual_hit = None;
        if self.render_rotation_degrees != 0 {
            self.status_log.push(
                "preview visual bbox hit-test is only available at 0 deg rotation".to_string(),
            );
            return;
        }

        let page = match self.visual_page_for_preview(click.page_index) {
            Ok(page) => page,
            Err(err) => {
                self.status_log.push(format!(
                    "preview visual bbox hit-test page {} failed: {err}",
                    click.page_index + 1
                ));
                return;
            }
        };

        if let Some(hit) = visual_hit_from_page_click(&page, click, self.render_zoom) {
            self.status_log.push(format!(
                "selected visual bbox page {} element {}",
                hit.page_index + 1,
                hit.element_index
            ));
            self.selected_visual_hit = Some(hit);
        } else {
            self.status_log.push(format!(
                "no visual bbox at page {} preview click",
                click.page_index + 1
            ));
        }
    }

    pub(crate) fn select_nice_stream_code_for_preview_selection(&mut self) -> bool {
        let Some(stream) = self
            .real_detail
            .as_ref()
            .and_then(|detail| detail.stream.clone())
        else {
            return false;
        };
        let Some(chunks) = self.loaded_nice_stream_chunks(stream.object) else {
            return false;
        };
        let rows = real_stream_nice_render_lines(stream.object, &chunks);
        let selection = self.current_preview_stream_selection();
        let text_hit = selection.text_hit;
        let visual_hit = selection.visual_hit;
        let page_index = selection.page_index;

        let selection_key = text_hit
            .as_ref()
            .filter(|hit| hit.page_index == page_index)
            .and_then(|hit| nice_stream_selection_key_for_text_hit(&rows, hit))
            .or_else(|| {
                let hit = visual_hit
                    .as_ref()
                    .filter(|hit| hit.page_index == page_index)?;
                let page_height = self.current_render_page_height(hit.page_index);
                let page = self.visual_page_for_preview(hit.page_index).ok()?;
                nice_stream_selection_key_for_visual_hit(&page, &rows, hit, page_height)
            });

        let Some(selection_key) = selection_key else {
            return false;
        };

        self.expand_nice_stream_selection_path(&rows, &selection_key);
        self.real_stream_selected_block = Some(selection_key);
        self.scroll_selected_nice_stream_row = true;
        self.pending_preview_stream_selection = None;
        self.status_log
            .push("highlighted nice stream code from preview click".to_string());
        true
    }

    pub(crate) fn current_preview_stream_selection(&self) -> PendingPreviewStreamSelection {
        self.pending_preview_stream_selection
            .clone()
            .unwrap_or_else(|| PendingPreviewStreamSelection {
                page_index: self.render_page_index,
                text_hit: self.selected_text_hit.clone(),
                visual_hit: self.selected_visual_hit.clone(),
            })
    }

    pub(crate) fn open_nice_stream_for_preview_selection(&mut self, page_index: usize) -> bool {
        if self.selected_text_hit.is_none() && self.selected_visual_hit.is_none() {
            self.pending_preview_stream_selection = None;
            return false;
        }

        self.pending_preview_stream_selection = Some(PendingPreviewStreamSelection {
            page_index,
            text_hit: self.selected_text_hit.clone(),
            visual_hit: self.selected_visual_hit.clone(),
        });

        if self.select_nice_stream_code_for_preview_selection() {
            self.selected_tab = InspectorTab::Stream;
            return true;
        }

        let Some(row) = self.ensure_page_content_stream_row(page_index) else {
            self.status_log.push(format!(
                "no content stream tree row available for page {} preview selection",
                page_index + 1
            ));
            self.pending_preview_stream_selection = None;
            return false;
        };

        self.select_stream_row_from_preview(row);
        self.selected_tab = InspectorTab::Stream;
        self.prepare_real_stream_for_preview_selection()
    }

    pub(crate) fn select_stream_row_from_preview(&mut self, row: usize) {
        if self.selected_row != row {
            self.back_stack.push(self.selected_row);
            self.selected_row = row;
            self.forward_stack.clear();
        }
        self.refresh_real_detail_for_selection();
        self.expand_selected_real_tree_path();
        self.scroll_selected_tree_row = true;
    }

    pub(crate) fn prepare_real_stream_for_preview_selection(&mut self) -> bool {
        let Some(stream) = self
            .real_detail
            .as_ref()
            .and_then(|detail| detail.stream.clone())
        else {
            self.pending_preview_stream_selection = None;
            return false;
        };
        if !stream.can_decode {
            self.status_log.push(format!(
                "stream {} cannot decode for preview-to-formatted selection",
                object_ref_text(stream.object)
            ));
            self.pending_preview_stream_selection = None;
            return false;
        }

        let limit = real_stream_default_limit(&stream, StreamMode::Decoded);
        let request_changed = self.real_stream_preset != RealStreamPreset::Nice
            || self.real_stream_mode != StreamMode::Decoded
            || self.real_stream_view_mode != StreamViewMode::Text
            || self.real_stream_offset != 0
            || self.real_stream_limit != limit
            || self
                .real_stream_key
                .is_some_and(|key| key.object != stream.object || key.mode != StreamMode::Decoded);

        self.real_stream_preset = RealStreamPreset::Nice;
        self.real_stream_mode = StreamMode::Decoded;
        self.real_stream_view_mode = StreamViewMode::Text;
        self.real_stream_offset = 0;
        self.real_stream_limit = limit;
        if request_changed {
            self.clear_real_stream_chunk();
        }

        self.refresh_real_stream_chunk(stream.object);
        self.select_nice_stream_code_for_preview_selection();
        true
    }

    pub(crate) fn loaded_nice_stream_chunks(&self, object: ObjectId) -> Option<Vec<StreamChunk>> {
        if self.real_stream_preset != RealStreamPreset::Nice
            || self.real_stream_mode != StreamMode::Decoded
            || self.real_stream_view_mode != StreamViewMode::Text
        {
            return None;
        }
        let key = self.real_stream_key?;
        if key.object != object || key.mode != StreamMode::Decoded {
            return None;
        }

        if !self.real_stream_windows.is_empty() {
            let chunks = self
                .real_stream_windows
                .iter()
                .filter(|window| {
                    window.key.object == object && window.key.mode == StreamMode::Decoded
                })
                .map(|window| window.chunk.clone())
                .collect::<Vec<_>>();
            return (!chunks.is_empty()).then_some(chunks);
        }

        self.real_stream_chunk.clone().map(|chunk| vec![chunk])
    }

    pub(crate) fn expand_nice_stream_selection_path(
        &mut self,
        rows: &[NiceStreamRenderLine],
        key: &str,
    ) {
        let Some(row) = rows
            .iter()
            .find(|row| row.line_key == key || row.block_key.as_deref() == Some(key))
        else {
            return;
        };
        for (_, block_key) in &row.guide_blocks {
            self.real_stream_collapsed_blocks.remove(block_key);
        }
        if let Some(block_key) = &row.block_key {
            self.real_stream_collapsed_blocks.remove(block_key);
        }
        if row.line.block_open {
            self.real_stream_collapsed_blocks.remove(&row.line_key);
        }
    }

    pub(crate) fn select_visual_bbox_for_tree_row(&mut self, row: usize) {
        let Some(target) = self.tree.real_row_visual_target(row) else {
            return;
        };
        if self.render_rotation_degrees != 0 {
            self.status_log
                .push("tree visual bbox highlight is only available at 0 deg rotation".to_string());
            return;
        }

        let page = match self.visual_page_for_preview(target.page_index) {
            Ok(page) => page,
            Err(err) => {
                self.status_log.push(format!(
                    "tree visual bbox page {} failed: {err}",
                    target.page_index + 1
                ));
                return;
            }
        };

        if let Some(hit) = visual_hit_for_object(&page, target.object) {
            self.selected_text_hit = None;
            self.selected_visual_hit = Some(hit);
            self.status_log.push(format!(
                "highlighted page {} visual bbox for {}",
                target.page_index + 1,
                object_ref_text(target.object)
            ));
            return;
        }

        if target.allow_page_union {
            if let Some(hit) = visual_hit_for_page_visual_union(&page, target.object) {
                self.selected_text_hit = None;
                self.selected_visual_hit = Some(hit);
                self.status_log.push(format!(
                    "highlighted page {} content stream bbox for {}",
                    target.page_index + 1,
                    object_ref_text(target.object)
                ));
            }
        }
    }

    pub(crate) fn visual_page_for_preview(
        &mut self,
        page_index: usize,
    ) -> Result<VisualPage, String> {
        if let Some(page) = self.visual_page_cache.get(page_index) {
            return Ok(page);
        }
        let state = self
            .state
            .as_ref()
            .map_err(|_| "document is not open".to_string())?;
        let mut request = VisualRequest::page(page_index);
        request.max_elements = VISUAL_CLICK_MAX_ELEMENTS_PER_PAGE;
        let page = state
            .session
            .run_task(|document| document.extract_visuals(&request))
            .map_err(|err| err.message)?;
        self.visual_page_cache.insert(page.clone());
        Ok(page)
    }

    pub(crate) fn text_page_for_preview(&mut self, page_index: usize) -> Result<TextPage, String> {
        if let Some(page) = self.text_search_cache.get(page_index) {
            return Ok(page);
        }
        let state = self
            .state
            .as_ref()
            .map_err(|_| "document is not open".to_string())?;
        let mut request = TextRequest::page(page_index);
        request.max_chars = TEXT_SEARCH_MAX_CHARS_PER_PAGE;
        request.max_blocks = TEXT_SEARCH_MAX_BLOCKS_PER_PAGE;
        let page = state
            .session
            .run_task(|document| document.extract_text(&request))
            .map_err(|err| err.message)?;
        self.text_search_cache.insert(page.clone());
        Ok(page)
    }

    pub(crate) fn handle_render_limit_error(&mut self, key: RealRenderKey, err: String) {
        self.real_render = None;
        self.real_render_texture = None;
        self.real_render_error = Some(err.clone());
        self.real_render_key = Some(key);
        self.render_limit_dialog_open = true;
        self.render_limit_gib_input = next_render_limit_gib_input(self.render_max_dimension);
        self.render_limit_dialog_error = None;
        self.status_log.push(format!(
            "page {} render exceeded {}: {err}",
            key.page_index + 1,
            render_limit_label(self.render_max_dimension)
        ));
    }

    pub(crate) fn apply_render_limit_upgrade(&mut self) {
        let input = self.render_limit_gib_input.trim();
        let Ok(gib) = input.parse::<u64>() else {
            self.render_limit_dialog_error = Some("Enter a whole number of GiB.".to_string());
            return;
        };
        let Some(max_dimension) = render_max_dimension_for_gib(gib) else {
            self.render_limit_dialog_error =
                Some("Enter a positive GiB value within the renderer range.".to_string());
            return;
        };
        let current_limit = render_max_output_bytes(self.render_max_dimension);
        let new_limit = render_max_output_bytes(max_dimension);
        if new_limit <= current_limit {
            self.render_limit_dialog_error = Some(format!(
                "Enter a value above {}.",
                render_limit_label(self.render_max_dimension)
            ));
            return;
        }

        self.render_max_dimension = max_dimension;
        self.render_limit_dialog_open = false;
        self.render_limit_dialog_error = None;
        self.real_render_job = None;
        self.real_render_key = None;
        self.real_render_texture = None;
        self.real_render = None;
        self.real_render_error = None;
        self.status_log.push(format!(
            "raised render limit to {}",
            render_limit_label(max_dimension)
        ));
        self.refresh_real_render();
    }

    pub(crate) fn refresh_real_render(&mut self) {
        let Some(key) = self.current_render_key() else {
            return;
        };
        if self.real_render_key == Some(key)
            && (self.real_render.is_some() || self.real_render_job.is_some())
        {
            return;
        }
        self.real_render_job = None;
        self.real_render_texture = None;
        self.real_render = None;
        self.real_render_error = None;
        self.real_render_key = Some(key);

        if let Some(render) = self.render_cache.get(&key) {
            self.real_render_texture = None;
            self.real_render = Some(render);
            self.status_log.push(format!(
                "reused cached page {} @ {:.0}% rot {} render",
                key.page_index + 1,
                key.zoom() * 100.0,
                key.rotation_degrees
            ));
            return;
        }

        let Ok(state) = self.state.as_ref() else {
            self.real_render_error = Some("document is not open".to_string());
            return;
        };
        let session = state.session.clone();
        self.real_render_job = Some(BackgroundJob::spawn(key, move |cancel| {
            let request = key.request();
            session
                .run_task(|document| document.render_page_with_cancel_token(&request, cancel))
                .map_err(|err| err.message)
        }));
        self.status_log.push(format!(
            "queued page {} @ {:.0}% rot {} render",
            key.page_index + 1,
            key.zoom() * 100.0,
            key.rotation_degrees
        ));
    }

    pub(crate) fn poll_real_render_job(&mut self) {
        match self.real_render_job.as_ref().map(BackgroundJob::poll) {
            None | Some(JobPoll::Pending) => {}
            Some(JobPoll::Finished(output)) => {
                self.real_render_job = None;
                if self.current_render_key() != Some(output.key) {
                    self.status_log.push(format!(
                        "discarded stale page {} render",
                        output.key.page_index + 1
                    ));
                    return;
                }
                match output.result {
                    Ok(render) => {
                        self.real_render_texture = None;
                        self.real_render_error = None;
                        self.status_log.push(format!(
                            "rendered page {} @ {:.0}% rot {} -> {}x{}",
                            render.page_index + 1,
                            output.key.zoom() * 100.0,
                            output.key.rotation_degrees,
                            render.width,
                            render.height
                        ));
                        self.render_cache.insert(output.key, render.clone());
                        self.real_render = Some(render);
                    }
                    Err(err) => {
                        if is_render_limit_error(&err) {
                            self.handle_render_limit_error(output.key, err);
                            return;
                        }
                        self.real_render = None;
                        self.real_render_error = Some(err.clone());
                        self.status_log.push(format!(
                            "page {} render failed: {err}",
                            output.key.page_index + 1
                        ));
                    }
                }
            }
            Some(JobPoll::Disconnected(key)) => {
                self.real_render_job = None;
                if self.current_render_key() == Some(key) {
                    self.real_render = None;
                    self.real_render_error = Some("render worker disconnected".to_string());
                }
            }
        }
    }

    pub(crate) fn safe_mode_active(&self) -> bool {
        self.state
            .as_ref()
            .ok()
            .and_then(|state| state.panels.summary.as_ref())
            .map(|summary| summary.safety.safe_mode)
            .unwrap_or(!self.empty_workspace)
    }

    pub(crate) fn window_title(&self) -> String {
        if let Some(job) = &self.open_pdf_job {
            return format!(
                "Opening {} - {APP_TITLE}",
                display_file_chip_label(job.key())
            );
        }
        if self.empty_workspace {
            return APP_TITLE.to_string();
        }
        if let Ok(state) = &self.state {
            if let Some(summary) = &state.panels.summary {
                return format!(
                    "{} - {APP_TITLE}",
                    display_file_chip_label(&summary.file_path)
                );
            }
        }
        format!("fake.pdf - {APP_TITLE}")
    }

    pub(crate) fn breadcrumb_label(&self) -> String {
        if self.empty_workspace {
            return "No document".to_string();
        }
        if let Some(detail) = &self.real_detail {
            return node_breadcrumb(&detail.id);
        }
        match &self.tree {
            TreeModel::Real(tree) => tree
                .summary(self.selected_row)
                .map(|summary| node_breadcrumb(&summary.id))
                .unwrap_or_else(|| "Document".to_string()),
            TreeModel::Virtual(_) => "Document/FakeNode".to_string(),
        }
    }

    pub(crate) fn diagnostics_filter(&self) -> DiagnosticFilter {
        DiagnosticFilter {
            min_severity: self.diagnostic_min_severity.clone(),
            code_query: Some(self.diagnostic_code_filter.clone()),
        }
    }

    pub(crate) fn diagnostics_model(&self) -> DocumentDiagnostics {
        DocumentDiagnostics::new(self.collected_diagnostics())
    }

    pub(crate) fn filtered_diagnostics(&self) -> Vec<DiagnosticSummary> {
        self.diagnostics_model()
            .filtered(&self.diagnostics_filter())
    }

    pub(crate) fn collected_diagnostics(&self) -> Vec<DiagnosticSummary> {
        let mut diagnostics = Vec::new();
        if let Ok(state) = &self.state {
            if let Some(summary) = &state.panels.summary {
                diagnostics.extend(summary.diagnostics.clone());
            }
        }
        if let Some(detail) = &self.real_detail {
            diagnostics.extend(detail.diagnostics.clone());
        }
        if let Some(chunk) = &self.real_stream_chunk {
            diagnostics.extend(chunk.decode_diagnostics.clone());
        }
        if let Some(render) = &self.real_render {
            diagnostics.extend(render.diagnostics.clone());
        }
        if let Some(result) = &self.text_search_result {
            diagnostics.extend(result.page_errors.iter().map(|error| DiagnosticSummary {
                severity: DiagnosticSeverity::Warning,
                code: DiagnosticCode::Unknown,
                message: format!(
                    "text extraction failed on page {}: {}",
                    error.page_index + 1,
                    error.message
                ),
                node: None,
                page_index: Some(error.page_index),
                object: None,
            }));
        }
        diagnostics
    }

    pub(crate) fn copy_diagnostics_json(&mut self, ctx: &egui::Context) {
        let diagnostics = self.filtered_diagnostics();
        let json = diagnostics_payload_to_json_string(&diagnostics);
        ctx.copy_text(json);
        self.status_log.push(format!(
            "copied diagnostics JSON with {} filtered diagnostics",
            diagnostics.len()
        ));
    }

    pub(crate) fn copy_markdown_report(&mut self, ctx: &egui::Context) {
        let diagnostics = self.filtered_diagnostics();
        let report = build_markdown_report(&MarkdownReportInput {
            document: self
                .state
                .as_ref()
                .ok()
                .and_then(|state| state.panels.summary.as_ref()),
            selected_object: self.real_detail.as_ref(),
            diagnostics: &diagnostics,
            object_search: self.object_search_result.as_ref(),
            text_search: self.text_search_result.as_ref(),
            max_diagnostics: REPORT_DIAGNOSTIC_LIMIT,
            max_object_hits: REPORT_SEARCH_HIT_LIMIT,
            max_text_hits: REPORT_SEARCH_HIT_LIMIT,
            max_bytes: MARKDOWN_REPORT_LIMIT_BYTES,
        });
        ctx.copy_text(report.text.clone());
        self.status_log.push(format!(
            "copied Markdown diagnostic report{}",
            if report.truncated { " (truncated)" } else { "" }
        ));
        self.copied_excerpt = Some(report);
    }
}
