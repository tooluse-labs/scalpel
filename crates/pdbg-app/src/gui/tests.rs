use super::*;

#[test]
fn workspace_layout_preserves_initial_preview_width_on_retina_sized_window() {
    let layout = workspace_panel_layout(1024.0);

    assert!(layout.left.default + layout.right.default <= 1024.0 - PAGE_PREVIEW_MIN_WIDTH);
    assert!(layout.left.default < LEFT_PANEL_DEFAULT_WIDTH);
    assert!(layout.right.default < RIGHT_PANEL_DEFAULT_WIDTH);
    assert_eq!(layout.left.max, LEFT_PANEL_MAX_WIDTH);
    assert_eq!(layout.right.max, RIGHT_PANEL_MAX_WIDTH);
}

#[test]
fn workspace_layout_keeps_wide_window_side_panels_roomy() {
    let layout = workspace_panel_layout(1920.0);

    assert_eq!(layout.left.default, LEFT_PANEL_DEFAULT_WIDTH);
    assert_eq!(layout.right.default, RIGHT_PANEL_DEFAULT_WIDTH);
    assert_eq!(layout.left.max, LEFT_PANEL_MAX_WIDTH);
    assert_eq!(layout.right.max, RIGHT_PANEL_MAX_WIDTH);
}

#[cfg(feature = "real-mupdf")]
fn wait_for_real_render(app: &mut GuiShellApp) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.real_render_job.is_some() && Instant::now() < deadline {
        app.poll_real_render_job();
        if app.real_render_job.is_some() {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    app.poll_real_render_job();
    assert!(
        app.real_render_job.is_none(),
        "real render job did not finish before test timeout"
    );
}

#[cfg(feature = "real-mupdf")]
fn wait_for_real_stream(app: &mut GuiShellApp) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.real_stream_job.is_some() && Instant::now() < deadline {
        app.poll_real_stream_job();
        if app.real_stream_job.is_some() {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    app.poll_real_stream_job();
    assert!(
        app.real_stream_job.is_none(),
        "real stream job did not finish before test timeout"
    );
}

fn wait_for_open_pdf(app: &mut GuiShellApp) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.open_pdf_job.is_some() && Instant::now() < deadline {
        app.poll_open_pdf_job();
        if app.open_pdf_job.is_some() {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    app.poll_open_pdf_job();
    assert!(
        app.open_pdf_job.is_none(),
        "open PDF job did not finish before test timeout"
    );
}

#[cfg(not(feature = "real-mupdf"))]
fn wait_for_text_search(app: &mut GuiShellApp) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.text_search_job.is_some() && Instant::now() < deadline {
        app.poll_text_search_job();
        if app.text_search_job.is_some() {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    app.poll_text_search_job();
    assert!(
        app.text_search_job.is_none(),
        "text search job did not finish before test timeout"
    );
}

#[cfg(not(feature = "real-mupdf"))]
fn wait_for_object_search(app: &mut GuiShellApp) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.object_search_job.is_some() && Instant::now() < deadline {
        app.poll_object_search_job();
        if app.object_search_job.is_some() {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    app.poll_object_search_job();
    assert!(
        app.object_search_job.is_none(),
        "object search job did not finish before test timeout"
    );
}

#[test]
fn virtual_tree_does_not_materialize_rows() {
    let tree = VirtualObjectTree::new(1_000_000);
    assert_eq!(tree.row_count(), 1_000_001);
    assert_eq!(tree.row_label(0), "root / catalog");
    assert_eq!(tree.row_label(999_999), "obj 999999 0 R  /FakeNode248");
}

#[test]
fn font_families_include_cjk_fallback_for_pdf_names() {
    let fonts = pdbg_fonts();
    assert!(fonts.font_data.contains_key(CJK_FONT_NAME));
    assert!(fonts
        .families
        .get(&FontFamily::Name("pdbg-sans".into()))
        .unwrap()
        .iter()
        .any(|name| name == "pdbg-cjk"));
    assert!(fonts
        .families
        .get(&FontFamily::Name("pdbg-mono".into()))
        .unwrap()
        .iter()
        .any(|name| name == "pdbg-cjk"));
    assert!(fonts
        .families
        .get(&FontFamily::Proportional)
        .unwrap()
        .iter()
        .any(|name| name == "pdbg-cjk"));
    assert!(fonts
        .families
        .get(&FontFamily::Monospace)
        .unwrap()
        .iter()
        .any(|name| name == "pdbg-cjk"));
}

#[test]
fn app_icon_decodes_to_square_rgba_with_transparent_corners() {
    let icon = app_icon().expect("app icon");

    assert_eq!(icon.width, icon.height);
    assert!(icon.width >= 512);
    assert_eq!(
        icon.rgba.len(),
        icon.width as usize * icon.height as usize * 4
    );
    assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] == 0));
}

#[test]
fn page_preview_display_size_reserves_footer_space() {
    let texture_size = egui::vec2(900.0, 1400.0);
    let available = egui::vec2(900.0, 1000.0);
    let display_size = page_preview_display_size(texture_size, available, 80.0, 1.0);
    assert!(display_size.y <= 920.0);
    assert!(available.y - display_size.y >= 80.0);
}

#[test]
fn page_preview_display_size_applies_visual_zoom() {
    let base_texture_size = egui::vec2(900.0, 1400.0);
    let available = egui::vec2(900.0, 1000.0);
    let fit_size = page_preview_display_size(base_texture_size, available, 80.0, 1.0);
    let zoomed_size = page_preview_display_size(base_texture_size * 2.0, available, 80.0, 2.0);
    assert!(zoomed_size.y > fit_size.y * 1.9);
}

#[test]
fn page_preview_leading_space_does_not_hide_wide_image_left_edge() {
    assert_eq!(page_preview_leading_space(900.0, 1400.0), 0.0);
    assert_eq!(page_preview_leading_space(900.0, 700.0), 100.0);
}

#[test]
fn preview_controls_overlay_stays_inside_wide_preview() {
    let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(1000.0, 800.0));
    let layout = preview_controls_overlay_layout(rect);

    assert!(!layout.stacked);
    assert_eq!(
        layout.width,
        PREVIEW_ZOOM_CONTROL_WIDTH + PREVIEW_CONTROL_GAP + PREVIEW_PAGER_CONTROL_WIDTH
    );
    // Centered, fully inside the content rect, anchored above the bottom.
    assert_eq!(layout.pos.x, rect.center().x - layout.width * 0.5);
    assert!(layout.pos.x >= rect.left());
    assert!(layout.pos.x + layout.width <= rect.right());
    assert!(layout.pos.y + layout.height < rect.bottom());
}

#[test]
fn preview_controls_overlay_stacks_and_clamps_in_narrow_preview() {
    // Narrower than the single-row layout (416px + margins) but at least the
    // workspace minimum center width.
    let rect = egui::Rect::from_min_size(egui::pos2(300.0, 0.0), egui::vec2(380.0, 600.0));
    let layout = preview_controls_overlay_layout(rect);

    assert!(layout.stacked);
    assert_eq!(layout.width, PREVIEW_ZOOM_CONTROL_WIDTH);
    assert_eq!(layout.height, 2.0 * PREVIEW_CONTROL_GROUP_HEIGHT + 8.0);
    assert!(layout.pos.x >= rect.left());
    assert!(layout.pos.x + layout.width <= rect.right());

    // Degenerate width narrower than even the stacked overlay: pinned to the
    // left margin without panicking on an inverted clamp range.
    let tiny = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(120.0, 600.0));
    let pinned = preview_controls_overlay_layout(tiny);
    assert!(pinned.stacked);
    assert_eq!(pinned.pos.x, tiny.left() + 8.0);
}

#[test]
fn render_zoom_steps_through_supported_levels() {
    assert_eq!(previous_render_zoom(0.5), None);
    assert_eq!(next_render_zoom(0.5), Some(1.0));
    assert_eq!(previous_render_zoom(1.0), Some(0.5));
    assert_eq!(next_render_zoom(1.0), Some(1.5));
    assert_eq!(previous_render_zoom(2.25), Some(2.0));
    assert_eq!(next_render_zoom(2.25), Some(3.0));
    assert_eq!(previous_render_zoom(4.0), Some(3.0));
    assert_eq!(next_render_zoom(4.0), None);
}

#[test]
fn render_rotation_cycles_through_right_angles() {
    assert_eq!(next_render_rotation(0), 90);
    assert_eq!(next_render_rotation(90), 180);
    assert_eq!(next_render_rotation(180), 270);
    assert_eq!(next_render_rotation(270), 0);
    assert_eq!(next_render_rotation(450), 180);
    assert_eq!(next_render_rotation(-90), 0);
}

#[test]
fn page_keyboard_shortcuts_choose_expected_pages() {
    assert_eq!(
        page_keyboard_target_page(2, 5, PageKeyboardShortcut::Previous),
        Some(1)
    );
    assert_eq!(
        page_keyboard_target_page(2, 5, PageKeyboardShortcut::Next),
        Some(3)
    );
    assert_eq!(
        page_keyboard_target_page(2, 5, PageKeyboardShortcut::First),
        Some(0)
    );
    assert_eq!(
        page_keyboard_target_page(2, 5, PageKeyboardShortcut::Last),
        Some(4)
    );
    assert_eq!(
        page_keyboard_target_page(0, 5, PageKeyboardShortcut::Previous),
        Some(0)
    );
    assert_eq!(
        page_keyboard_target_page(4, 5, PageKeyboardShortcut::Next),
        Some(4)
    );
    assert_eq!(
        page_keyboard_target_page(0, 0, PageKeyboardShortcut::Next),
        None
    );
}

#[test]
fn preview_scroll_targets_pages_from_wheel_and_trackpad_delta() {
    let mut accumulator = egui::Vec2::ZERO;
    assert_eq!(
        preview_scroll_target_page(
            2,
            5,
            &mut accumulator,
            egui::vec2(0.0, -PREVIEW_PAGE_SCROLL_THRESHOLD)
        ),
        Some(3)
    );

    accumulator = egui::Vec2::ZERO;
    assert_eq!(
        preview_scroll_target_page(
            2,
            5,
            &mut accumulator,
            egui::vec2(0.0, PREVIEW_PAGE_SCROLL_THRESHOLD)
        ),
        Some(1)
    );

    accumulator = egui::Vec2::ZERO;
    assert_eq!(
        preview_scroll_target_page(
            2,
            5,
            &mut accumulator,
            egui::vec2(-PREVIEW_PAGE_SCROLL_THRESHOLD, 0.0)
        ),
        Some(3)
    );
}

#[test]
fn preview_scroll_accumulates_small_deltas_and_clamps_boundaries() {
    let mut accumulator = egui::Vec2::ZERO;
    assert_eq!(
        preview_scroll_target_page(
            2,
            5,
            &mut accumulator,
            egui::vec2(0.0, -PREVIEW_PAGE_SCROLL_THRESHOLD * 0.5)
        ),
        None
    );
    assert_eq!(
        preview_scroll_target_page(
            2,
            5,
            &mut accumulator,
            egui::vec2(0.0, -PREVIEW_PAGE_SCROLL_THRESHOLD * 0.5)
        ),
        Some(3)
    );

    accumulator = egui::Vec2::ZERO;
    assert_eq!(
        preview_scroll_target_page(
            3,
            5,
            &mut accumulator,
            egui::vec2(0.0, -PREVIEW_PAGE_SCROLL_THRESHOLD * 3.2)
        ),
        Some(4)
    );

    accumulator = egui::Vec2::ZERO;
    assert_eq!(
        preview_scroll_target_page(
            0,
            5,
            &mut accumulator,
            egui::vec2(0.0, PREVIEW_PAGE_SCROLL_THRESHOLD)
        ),
        Some(0)
    );
}

#[test]
fn render_key_request_applies_configured_dimension_limit() {
    let key = RealRenderKey::new(0, 1.5, 90, 8192);
    let request = key.request();

    assert_eq!(request.max_width, 8192);
    assert_eq!(request.max_height, 8192);
    assert_eq!(request.max_pixels, 8192 * 8192);
    assert_eq!(request.max_output_bytes, 8192 * 8192 * 4);
    assert_eq!(request.zoom, 1.5);
    assert_eq!(request.rotation_degrees, 90);
}

#[test]
fn gui_options_default_render_dimension_limit_when_unset_or_zero() {
    assert_eq!(
        render_max_dimension_or_default(None),
        DEFAULT_RENDER_MAX_DIMENSION
    );
    assert_eq!(
        render_max_dimension_or_default(Some(0)),
        DEFAULT_RENDER_MAX_DIMENSION
    );
    assert_eq!(render_max_dimension_or_default(Some(8192)), 8192);
}

#[test]
fn preview_click_maps_to_render_pixel_coordinates() {
    let image_rect = egui::Rect::from_min_size(egui::pos2(100.0, 40.0), egui::vec2(300.0, 600.0));
    let click = preview_click_from_pos(egui::pos2(250.0, 340.0), image_rect, 900, 1800, 4).unwrap();
    assert_eq!(click.page_index, 4);
    assert!((click.render_x - 450.0).abs() < f32::EPSILON);
    assert!((click.render_y - 900.0).abs() < f32::EPSILON);
}

#[test]
fn preview_click_hit_tests_text_span_bbox_in_page_space() {
    let page = TextPage {
        page_index: 1,
        spans: vec![TextSpan {
            text: "Abstract".to_string(),
            bbox: PageRect {
                x: 100.0,
                y: 50.0,
                width: 200.0,
                height: 24.0,
            },
            untrusted: false,
        }],
    };
    let click = PagePreviewClick {
        page_index: 1,
        render_x: 300.0,
        render_y: 120.0,
        normalized_x: 0.0,
        normalized_y: 0.0,
    };
    let hit = text_hit_from_page_click(&page, click, 2.0).unwrap();
    assert_eq!(hit.page_index, 1);
    assert_eq!(hit.span_index, 0);
    assert_eq!(hit.excerpt, "Abstract");

    let miss = PagePreviewClick {
        render_x: 20.0,
        render_y: 20.0,
        ..click
    };
    assert!(text_hit_from_page_click(&page, miss, 2.0).is_none());
}

#[test]
fn preview_click_hit_tests_visual_bbox_in_page_space() {
    let page = VisualPage {
        page_index: 2,
        elements: vec![
            VisualElement {
                kind: VisualElementKind::Text,
                bbox: PageRect {
                    x: 10.0,
                    y: 10.0,
                    width: 500.0,
                    height: 500.0,
                },
                object: None,
                untrusted: true,
            },
            VisualElement {
                kind: VisualElementKind::Image,
                bbox: PageRect {
                    x: 100.0,
                    y: 50.0,
                    width: 200.0,
                    height: 80.0,
                },
                object: Some(ObjectId { num: 12, gen: 0 }),
                untrusted: false,
            },
        ],
    };
    let click = PagePreviewClick {
        page_index: 2,
        render_x: 300.0,
        render_y: 140.0,
        normalized_x: 0.0,
        normalized_y: 0.0,
    };

    let hit = visual_hit_from_page_click(&page, click, 2.0).unwrap();
    assert_eq!(hit.page_index, 2);
    assert_eq!(hit.element_index, 1);
    assert_eq!(hit.kind, VisualElementKind::Image);
    assert_eq!(hit.object, Some(ObjectId { num: 12, gen: 0 }));
    assert!(hit.contains_click);

    let miss = PagePreviewClick {
        render_x: 2.0,
        render_y: 2.0,
        ..click
    };
    assert!(visual_hit_from_page_click(&page, miss, 2.0).is_none());
}

#[test]
fn visual_hit_for_object_unions_matching_bboxes() {
    let object = ObjectId { num: 30, gen: 0 };
    let page = VisualPage {
        page_index: 0,
        elements: vec![
            VisualElement {
                kind: VisualElementKind::Text,
                bbox: PageRect {
                    x: 20.0,
                    y: 10.0,
                    width: 40.0,
                    height: 30.0,
                },
                object: Some(object),
                untrusted: false,
            },
            VisualElement {
                kind: VisualElementKind::Vector,
                bbox: PageRect {
                    x: 50.0,
                    y: 30.0,
                    width: 60.0,
                    height: 25.0,
                },
                object: Some(object),
                untrusted: true,
            },
            VisualElement {
                kind: VisualElementKind::Image,
                bbox: PageRect {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
                object: Some(ObjectId { num: 99, gen: 0 }),
                untrusted: false,
            },
        ],
    };

    let hit = visual_hit_for_object(&page, object).unwrap();

    assert_eq!(hit.page_index, 0);
    assert_eq!(hit.element_index, 0);
    assert_eq!(hit.kind, VisualElementKind::Unknown);
    assert_eq!(hit.object, Some(object));
    assert!(hit.untrusted);
    assert!(!hit.contains_click);
    assert_eq!(hit.bbox.x, 20.0);
    assert_eq!(hit.bbox.y, 10.0);
    assert_eq!(hit.bbox.width, 90.0);
    assert_eq!(hit.bbox.height, 45.0);
}

#[test]
fn visual_hit_for_page_visual_union_uses_all_visible_bboxes() {
    let object = ObjectId { num: 30, gen: 0 };
    let page = VisualPage {
        page_index: 1,
        elements: vec![
            VisualElement {
                kind: VisualElementKind::Vector,
                bbox: PageRect {
                    x: 10.0,
                    y: 20.0,
                    width: 30.0,
                    height: 40.0,
                },
                object: None,
                untrusted: false,
            },
            VisualElement {
                kind: VisualElementKind::Vector,
                bbox: PageRect {
                    x: 50.0,
                    y: 5.0,
                    width: 20.0,
                    height: 30.0,
                },
                object: None,
                untrusted: false,
            },
            VisualElement {
                kind: VisualElementKind::Image,
                bbox: PageRect {
                    x: 80.0,
                    y: 80.0,
                    width: 0.0,
                    height: 10.0,
                },
                object: None,
                untrusted: true,
            },
        ],
    };

    let hit = visual_hit_for_page_visual_union(&page, object).unwrap();

    assert_eq!(hit.page_index, 1);
    assert_eq!(hit.element_index, 0);
    assert_eq!(hit.kind, VisualElementKind::Vector);
    assert_eq!(hit.object, Some(object));
    assert!(!hit.untrusted);
    assert_eq!(hit.bbox.x, 10.0);
    assert_eq!(hit.bbox.y, 5.0);
    assert_eq!(hit.bbox.width, 60.0);
    assert_eq!(hit.bbox.height, 55.0);
}

#[test]
fn page_content_stream_node_detection_follows_contents_arrays() {
    let doc = pdbg_core::DocumentId(7);
    let page = NodeId::Page {
        doc: doc.clone(),
        index: 0,
    };
    let contents = NodeId::DictEntry {
        doc: doc.clone(),
        parent: Box::new(page),
        key: "Contents".to_string(),
    };
    let content_item = NodeId::ArrayEntry {
        doc: doc.clone(),
        parent: Box::new(contents),
        index: 0,
    };
    let media_box = NodeId::DictEntry {
        doc,
        parent: Box::new(NodeId::Page {
            doc: pdbg_core::DocumentId(7),
            index: 0,
        }),
        key: "MediaBox".to_string(),
    };

    assert!(is_page_content_stream_node(&content_item));
    assert!(!is_page_content_stream_node(&media_box));
}

#[test]
fn real_tree_finds_page_content_stream_descendant() {
    let doc = pdbg_core::DocumentId(7);
    let page = NodeId::ArrayEntry {
        doc: doc.clone(),
        parent: Box::new(NodeId::PageRoot { doc: doc.clone() }),
        index: 1,
    };
    let content_stream = ObjectSummary {
        id: NodeId::DictEntry {
            doc: doc.clone(),
            parent: Box::new(page.clone()),
            key: "Contents".to_string(),
        },
        kind: ObjectKind::Stream,
        label: "Contents".to_string(),
        preview: "7 0 R stream".to_string(),
        object: Some(ObjectId { num: 7, gen: 0 }),
        has_children: false,
        has_stream: true,
        child_count: None,
        byte_size_hint: None,
        diagnostics: Vec::new(),
    };
    let media_box = ObjectSummary {
        id: NodeId::DictEntry {
            doc,
            parent: Box::new(page.clone()),
            key: "MediaBox".to_string(),
        },
        kind: ObjectKind::Array,
        label: "MediaBox".to_string(),
        preview: String::new(),
        object: None,
        has_children: false,
        has_stream: false,
        child_count: None,
        byte_size_hint: None,
        diagnostics: Vec::new(),
    };
    let tree = RealObjectTree {
        rows: vec![
            RealTreeRow {
                summary: ObjectSummary {
                    id: page,
                    kind: ObjectKind::Page,
                    label: "Page 2".to_string(),
                    preview: String::new(),
                    object: None,
                    has_children: true,
                    has_stream: false,
                    child_count: Some(2),
                    byte_size_hint: None,
                    diagnostics: Vec::new(),
                },
                depth: 0,
                expanded: true,
            },
            RealTreeRow {
                summary: media_box,
                depth: 1,
                expanded: false,
            },
            RealTreeRow {
                summary: content_stream,
                depth: 1,
                expanded: false,
            },
        ],
        root_children: Vec::new(),
        total: Some(1),
    };

    assert_eq!(tree.first_page_content_stream_row(0), Some(2));
    assert_eq!(tree.page_content_candidate_rows(0), vec![2]);
}

#[test]
fn selecting_tree_row_clears_preview_hit_selection() {
    let mut app = GuiShellApp::new();
    app.preview_click = Some(PagePreviewClick {
        page_index: 0,
        render_x: 10.0,
        render_y: 20.0,
        normalized_x: 0.1,
        normalized_y: 0.2,
    });
    app.selected_visual_hit = Some(PreviewVisualHit {
        page_index: 0,
        element_index: 0,
        kind: VisualElementKind::Text,
        bbox: PageRect {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        },
        object: None,
        untrusted: true,
        contains_click: true,
    });

    app.select_row_from_tree(app.selected_row);

    assert!(app.preview_click.is_none());
    assert!(app.selected_visual_hit.is_none());
    assert!(app.selected_text_hit.is_none());
}

#[test]
fn visual_page_cache_reuses_lru_and_respects_element_budget() {
    let mut cache = VisualPageCache::new(2, 3);
    cache.insert(VisualPage {
        page_index: 1,
        elements: vec![visual_test_element(1.0)],
    });
    cache.insert(VisualPage {
        page_index: 2,
        elements: vec![visual_test_element(2.0)],
    });

    assert!(cache.get(1).is_some());
    cache.insert(VisualPage {
        page_index: 3,
        elements: vec![visual_test_element(3.0)],
    });

    assert!(cache.get(2).is_none());
    assert!(cache.get(1).is_some());
    assert!(cache.get(3).is_some());

    cache.insert(VisualPage {
        page_index: 4,
        elements: vec![
            visual_test_element(4.0),
            visual_test_element(5.0),
            visual_test_element(6.0),
            visual_test_element(7.0),
        ],
    });
    assert!(cache.get(4).is_none());
}

#[test]
fn visual_object_attribution_text_hides_missing_object() {
    let mut hit = PreviewVisualHit {
        page_index: 0,
        element_index: 0,
        kind: VisualElementKind::Text,
        bbox: PageRect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
        object: None,
        untrusted: true,
        contains_click: true,
    };

    assert_eq!(visual_object_attribution_text(None), None);
    assert_eq!(visual_object_attribution_text(Some(&hit)), None);
    hit.object = Some(ObjectId { num: 12, gen: 0 });
    assert_eq!(
        visual_object_attribution_text(Some(&hit)).as_deref(),
        Some("12 0 R")
    );
}

fn visual_test_element(x: f32) -> VisualElement {
    VisualElement {
        kind: VisualElementKind::Text,
        bbox: PageRect {
            x,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        object: None,
        untrusted: true,
    }
}

#[cfg(not(feature = "real-mupdf"))]
#[test]
fn gui_object_search_navigates_headless_fake_hit() {
    let mut app = GuiShellApp::new();
    app.object_search_query = "2 0 R".to_string();

    app.run_object_search();
    assert!(app.object_search_job.is_some());
    wait_for_object_search(&mut app);

    let result = app.object_search_result.as_ref().unwrap();
    assert!(app.object_search_error.is_none());
    assert!(result.searched_nodes > 0);
    let hit = result
        .hits
        .iter()
        .find(|hit| {
            hit.matched_field == ObjectSearchField::ObjectNumber
                && hit.object == Some(ObjectId { num: 2, gen: 0 })
        })
        .cloned()
        .unwrap();

    app.follow_object_search_hit(&hit);

    assert_eq!(app.selected_row, 2);
    assert_eq!(app.back_stack, vec![0]);
    assert!(app.forward_stack.is_empty());
    assert!(app
        .status_log
        .iter()
        .any(|line| line.starts_with("opened object search hit 2 0 R")));
}

#[test]
fn real_tree_search_hit_row_preserves_search_node() {
    let doc = pdbg_core::DocumentId(7);
    let node = NodeId::DictEntry {
        doc: doc.clone(),
        parent: Box::new(NodeId::DocumentRoot { doc: doc.clone() }),
        key: "Needs".to_string(),
    };
    let hit = ObjectSearchHit {
        label: "Needs".to_string(),
        matched_field: ObjectSearchField::DictionaryKey,
        excerpt: "Needs".to_string(),
        object: None,
        node: Some(node.clone()),
        depth: 2,
    };
    let mut tree = RealObjectTree::from_child_page(&pdbg_core::ChildPage {
        total: Some(0),
        items: Vec::new(),
    });

    let row = tree.ensure_search_hit_row(doc, &hit).unwrap();

    assert_eq!(tree.rows[row].summary.id, node);
    assert_eq!(tree.rows[row].summary.preview, "Needs");
    assert_eq!(tree.rows[row].depth, 2);
}

#[test]
fn text_search_hit_summary_egresses_display_controls() {
    let hit = TextSearchHit {
        page_index: 0,
        span_index: 2,
        excerpt: "A\0B\u{202e}C\nD".to_string(),
        bbox: None,
        untrusted: true,
    };

    let summary = text_search_hit_summary(&hit);

    assert!(summary.contains(&format!("A{}B{}C D", '\u{fffd}', '\u{fffd}')));
    assert!(!summary.contains('\0'));
    assert!(!summary.contains('\u{202e}'));
}

#[cfg(not(feature = "real-mupdf"))]
#[test]
fn gui_text_search_runs_async_caches_and_selects_hit() {
    let mut app = GuiShellApp::new();
    app.text_search_query = "A".to_string();

    app.start_text_search();
    assert!(app.text_search_job.is_some());
    wait_for_text_search(&mut app);

    let result = app.text_search_result.as_ref().unwrap();
    assert_eq!(result.searched_pages, 1);
    assert_eq!(result.cache_hits, 0);
    assert!(result.hits.iter().any(|hit| hit.untrusted));
    assert_eq!(app.text_search_cache.len(), 1);

    app.start_text_search();
    wait_for_text_search(&mut app);
    assert_eq!(app.text_search_result.as_ref().unwrap().cache_hits, 1);

    let hit = app.text_search_result.as_ref().unwrap().hits[0].clone();
    app.follow_text_search_hit(&hit);

    assert_eq!(app.selected_text_hit.as_ref().unwrap().page_index, 0);
    assert_eq!(app.render_page_index, 0);
    assert!(app
        .status_log
        .iter()
        .any(|line| line.starts_with("opened text search hit page 1")));
}

#[test]
fn diagnostics_model_includes_text_page_errors_and_filters_codes() {
    let mut app = GuiShellApp::new();
    app.text_search_result = Some(TextSearchResult {
        hits: Vec::new(),
        searched_pages: 1,
        cache_hits: 0,
        page_errors: vec![pdbg_core::TextSearchPageError {
            page_index: 2,
            message: "limit".to_string(),
        }],
        truncated: false,
    });
    app.diagnostic_min_severity = Some(DiagnosticSeverity::Warning);
    app.diagnostic_code_filter = "unknown".to_string();

    let diagnostics = app.filtered_diagnostics();

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::Unknown
            && diagnostic.page_index == Some(2)
            && diagnostic.message.contains("text extraction failed")
    }));
}

#[test]
fn stream_range_excerpt_is_bounded_and_escaped() {
    let mut model = LargeStreamModel {
        offset: 0,
        selection_offset: 0,
        selection_len: 16 * 1024,
        ..LargeStreamModel::default()
    };
    model.sync_hex_window();
    let escaped = model.escaped_range_selection();
    assert!(escaped.truncated);
    assert!(escaped.text.len() < COPY_LIMIT_BYTES * 2);
    assert!(escaped.text.contains("00000000"));
}

#[test]
fn stream_hex_window_uses_generated_bytes() {
    let model = LargeStreamModel::default();
    let window = model.hex_window();
    assert!(window.starts_with("00000000  11 30 4f 6e"));
    assert!(window.lines().count() > 1);
}

#[test]
fn hex_dump_bytes_formats_offsets_hex_and_ascii() {
    let dump = hex_dump_bytes(16, b"BT /F1\n");
    assert!(dump.starts_with("00000010  42 54 20 2f 46 31 0a"));
    assert!(dump.contains("BT /F1."));
}

#[test]
fn stream_chunk_display_text_supports_hex_text_and_bytes() {
    let chunk = StreamChunk {
        mode: StreamMode::Decoded,
        offset: 32,
        bytes: b"Hi\n".to_vec(),
        total_size: Some(3),
        truncated: false,
        decode_diagnostics: Vec::new(),
    };

    assert!(
        stream_chunk_display_text(&chunk, StreamViewMode::Hex).starts_with("00000020  48 69 0a")
    );
    assert_eq!(
        stream_chunk_display_text(&chunk, StreamViewMode::Text),
        "Hi\n"
    );
    assert_eq!(
        stream_chunk_display_text(&chunk, StreamViewMode::Bytes),
        "72 105 10"
    );
}

#[test]
fn pdf_content_stream_nice_text_indents_content_stream_structure() {
    let nice = pdf_content_stream_nice_text(
        b"q 1 0 0 -1 0 792 cm q /GS63 gs 0 0 541.44 753.96 re f* Q \
              /P << /MCID 0 >> BDC BT /FT68 360 Tf <0017> Tj ET EMC Q",
    );

    assert_eq!(
            nice,
            "q\n  1 0 0 -1 0 792 cm\n  q\n    /GS63 gs\n    0 0 541.44 753.96 re\n    f*\n  Q\n  /P << /MCID 0 >> BDC\n    BT\n      /FT68 360 Tf\n      <0017> Tj\n    ET\n  EMC\nQ\n"
        );
}

#[test]
fn nice_stream_render_lines_group_selection_by_structural_block() {
    let object = ObjectId { num: 38, gen: 0 };
    let chunks = vec![StreamChunk {
        mode: StreamMode::Decoded,
        offset: 0,
        bytes: b"q /P << /MCID 0 >> BDC BT /F1 9 Tf (Hi) Tj ET EMC Q".to_vec(),
        total_size: Some(64),
        truncated: false,
        decode_diagnostics: Vec::new(),
    }];

    let rows = real_stream_nice_render_lines(object, &chunks);
    let bdc_key = rows
        .iter()
        .find(|row| row.line.text.ends_with(" BDC"))
        .unwrap()
        .line_key
        .clone();
    let bt_key = rows
        .iter()
        .find(|row| row.line.text == "BT")
        .unwrap()
        .line_key
        .clone();

    assert_eq!(
        rows.iter()
            .find(|row| row.line.text == "/F1 9 Tf")
            .unwrap()
            .block_key
            .as_deref(),
        Some(bt_key.as_str())
    );
    assert!(rows
        .iter()
        .find(|row| row.line.text == "/F1 9 Tf")
        .unwrap()
        .guide_blocks
        .iter()
        .any(|(_, key)| key == &bdc_key));
    assert!(rows
        .iter()
        .find(|row| row.line.text == "/F1 9 Tf")
        .unwrap()
        .guide_blocks
        .iter()
        .any(|(_, key)| key == &bt_key));
    assert_eq!(
        rows.iter()
            .find(|row| row.line.text == "EMC")
            .unwrap()
            .block_key
            .as_deref(),
        Some(bdc_key.as_str())
    );
}

#[test]
fn nice_stream_selection_extracts_text_fragments_from_block() {
    let object = ObjectId { num: 7, gen: 0 };
    let chunks = vec![StreamChunk {
        mode: StreamMode::Decoded,
        offset: 0,
        bytes: b"BT /F1 12 Tf (Country of Citizenship) Tj ET".to_vec(),
        total_size: Some(47),
        truncated: false,
        decode_diagnostics: Vec::new(),
    }];
    let rows = real_stream_nice_render_lines(object, &chunks);
    let bt_key = rows
        .iter()
        .find(|row| row.line.text == "BT")
        .unwrap()
        .line_key
        .clone();

    assert_eq!(
        nice_stream_text_fragments_for_selection(&rows, &bt_key),
        vec!["Country of Citizenship"]
    );
}

#[test]
fn text_hit_for_fragments_unions_matching_text_spans() {
    let page = TextPage {
        page_index: 1,
        spans: vec![
            TextSpan {
                text: "Country of".to_string(),
                bbox: PageRect {
                    x: 10.0,
                    y: 20.0,
                    width: 40.0,
                    height: 8.0,
                },
                untrusted: false,
            },
            TextSpan {
                text: "Citizenship".to_string(),
                bbox: PageRect {
                    x: 52.0,
                    y: 18.0,
                    width: 48.0,
                    height: 12.0,
                },
                untrusted: true,
            },
        ],
    };

    let hit = text_hit_for_text_fragments(&page, &["Country of Citizenship".to_string()])
        .expect("expected positioned text hit");

    assert_eq!(hit.page_index, 1);
    assert_eq!(hit.span_index, 0);
    assert!(hit.untrusted);
    let bbox = hit.bbox.unwrap();
    assert_eq!(bbox.x, 10.0);
    assert_eq!(bbox.y, 18.0);
    assert_eq!(bbox.width, 90.0);
    assert_eq!(bbox.height, 12.0);
}

#[test]
fn nice_stream_text_hit_selects_matching_text_show_line() {
    let object = ObjectId { num: 7, gen: 0 };
    let chunks = vec![StreamChunk {
        mode: StreamMode::Decoded,
        offset: 0,
        bytes: b"BT /F1 12 Tf (Country of Citizenship) Tj (China) Tj ET".to_vec(),
        total_size: Some(59),
        truncated: false,
        decode_diagnostics: Vec::new(),
    }];
    let rows = real_stream_nice_render_lines(object, &chunks);
    let hit = TextSearchHit {
        page_index: 0,
        span_index: 0,
        excerpt: "Country of Citizenship".to_string(),
        bbox: None,
        untrusted: false,
    };

    let key = nice_stream_selection_key_for_text_hit(&rows, &hit).unwrap();
    let row = rows.iter().find(|row| row.line_key == key).unwrap();

    assert_eq!(row.line.text, "(Country of Citizenship) Tj");
}

#[test]
fn nice_stream_visual_selection_matches_vector_and_image_order() {
    let object = ObjectId { num: 7, gen: 0 };
    let chunks = vec![StreamChunk {
        mode: StreamMode::Decoded,
        offset: 0,
        bytes: b"BT (Title) Tj ET q 10 10 20 20 re f Q q /Im0 Do Q".to_vec(),
        total_size: Some(57),
        truncated: false,
        decode_diagnostics: Vec::new(),
    }];
    let rows = real_stream_nice_render_lines(object, &chunks);
    let page = VisualPage {
        page_index: 0,
        elements: vec![
            VisualElement {
                kind: VisualElementKind::Text,
                bbox: PageRect {
                    x: 1.0,
                    y: 1.0,
                    width: 10.0,
                    height: 5.0,
                },
                object: None,
                untrusted: false,
            },
            VisualElement {
                kind: VisualElementKind::Vector,
                bbox: PageRect {
                    x: 10.0,
                    y: 10.0,
                    width: 20.0,
                    height: 20.0,
                },
                object: None,
                untrusted: false,
            },
            VisualElement {
                kind: VisualElementKind::Image,
                bbox: PageRect {
                    x: 40.0,
                    y: 50.0,
                    width: 30.0,
                    height: 25.0,
                },
                object: None,
                untrusted: false,
            },
        ],
    };
    let vector_key = rows
        .iter()
        .find(|row| row.line.text == "f")
        .and_then(|row| row.block_key.clone())
        .unwrap();
    let image_key = rows
        .iter()
        .find(|row| row.line.text.ends_with(" Do"))
        .and_then(|row| row.block_key.clone())
        .unwrap();

    let vector_hit =
        nice_stream_visual_hit_for_selection(&page, &rows, &vector_key, object, None).unwrap();
    let image_hit =
        nice_stream_visual_hit_for_selection(&page, &rows, &image_key, object, None).unwrap();

    assert_eq!(vector_hit.element_index, 1);
    assert_eq!(vector_hit.kind, VisualElementKind::Vector);
    assert_eq!(vector_hit.bbox.x, 10.0);
    assert_eq!(image_hit.element_index, 2);
    assert_eq!(image_hit.kind, VisualElementKind::Image);
    assert_eq!(image_hit.bbox.x, 40.0);
}

#[test]
fn nice_stream_image_selection_uses_cm_bbox_for_highlight() {
    let object = ObjectId { num: 7, gen: 0 };
    let chunks = vec![StreamChunk {
        mode: StreamMode::Decoded,
        offset: 0,
        bytes: b"q 100 0 0 50 300 400 cm /Im0 Do Q".to_vec(),
        total_size: Some(38),
        truncated: false,
        decode_diagnostics: Vec::new(),
    }];
    let rows = real_stream_nice_render_lines(object, &chunks);
    let page = VisualPage {
        page_index: 0,
        elements: vec![
            VisualElement {
                kind: VisualElementKind::Image,
                bbox: PageRect {
                    x: 300.0,
                    y: 400.0,
                    width: 100.0,
                    height: 50.0,
                },
                object: None,
                untrusted: false,
            },
            VisualElement {
                kind: VisualElementKind::Image,
                bbox: PageRect {
                    x: 300.0,
                    y: 50.0,
                    width: 100.0,
                    height: 50.0,
                },
                object: None,
                untrusted: false,
            },
        ],
    };
    let image_key = rows
        .iter()
        .find(|row| row.line.text == "/Im0 Do")
        .map(|row| row.line_key.clone())
        .unwrap();

    let hit = nice_stream_visual_hit_for_selection(&page, &rows, &image_key, object, Some(500.0))
        .unwrap();

    assert_eq!(hit.kind, VisualElementKind::Image);
    assert_eq!(hit.element_index, 1);
    assert_eq!(hit.bbox.x, 300.0);
    assert_eq!(hit.bbox.y, 50.0);
    assert_eq!(hit.bbox.width, 100.0);
    assert_eq!(hit.bbox.height, 50.0);
}

#[test]
fn nice_stream_visual_hit_prefers_image_bbox_overlap_over_ordinal() {
    let object = ObjectId { num: 7, gen: 0 };
    let chunks = vec![StreamChunk {
        mode: StreamMode::Decoded,
        offset: 0,
        bytes: b"q 100 0 0 50 0 0 cm /Im0 Do Q q 20 0 0 10 300 400 cm /Im1 Do Q".to_vec(),
        total_size: Some(72),
        truncated: false,
        decode_diagnostics: Vec::new(),
    }];
    let rows = real_stream_nice_render_lines(object, &chunks);
    let page = VisualPage {
        page_index: 0,
        elements: vec![VisualElement {
            kind: VisualElementKind::Image,
            bbox: PageRect {
                x: 300.0,
                y: 90.0,
                width: 20.0,
                height: 10.0,
            },
            object: None,
            untrusted: false,
        }],
    };
    let hit = PreviewVisualHit {
        page_index: 0,
        element_index: 0,
        kind: VisualElementKind::Image,
        bbox: page.elements[0].bbox.clone(),
        object: None,
        untrusted: false,
        contains_click: true,
    };

    let key = nice_stream_selection_key_for_visual_hit(&page, &rows, &hit, Some(500.0)).unwrap();
    let row = rows.iter().find(|row| row.line_key == key).unwrap();

    assert_eq!(row.line.text, "/Im1 Do");
}

#[test]
fn nice_stream_visual_hit_selects_matching_draw_operation() {
    let object = ObjectId { num: 7, gen: 0 };
    let chunks = vec![StreamChunk {
        mode: StreamMode::Decoded,
        offset: 0,
        bytes: b"q 10 10 20 20 re f Q q /Im0 Do Q".to_vec(),
        total_size: Some(37),
        truncated: false,
        decode_diagnostics: Vec::new(),
    }];
    let rows = real_stream_nice_render_lines(object, &chunks);
    let page = VisualPage {
        page_index: 0,
        elements: vec![
            VisualElement {
                kind: VisualElementKind::Vector,
                bbox: PageRect {
                    x: 10.0,
                    y: 10.0,
                    width: 20.0,
                    height: 20.0,
                },
                object: None,
                untrusted: false,
            },
            VisualElement {
                kind: VisualElementKind::Image,
                bbox: PageRect {
                    x: 40.0,
                    y: 50.0,
                    width: 30.0,
                    height: 25.0,
                },
                object: None,
                untrusted: false,
            },
        ],
    };
    let hit = PreviewVisualHit {
        page_index: 0,
        element_index: 1,
        kind: VisualElementKind::Image,
        bbox: page.elements[1].bbox.clone(),
        object: None,
        untrusted: false,
        contains_click: true,
    };

    let key = nice_stream_selection_key_for_visual_hit(&page, &rows, &hit, None).unwrap();
    let row = rows.iter().find(|row| row.line_key == key).unwrap();

    assert_eq!(row.line.text, "/Im0 Do");
}

#[test]
fn real_stream_default_limit_uses_bounded_windows() {
    let stream = StreamSummary {
        object: ObjectId { num: 262, gen: 0 },
        filters: vec!["FlateDecode".to_string()],
        raw_size_hint: Some(6417),
        decoded_size_hint: None,
        can_decode: true,
        image_preview_available: false,
    };

    assert_eq!(real_stream_default_limit(&stream, StreamMode::Raw), 6417);
    assert_eq!(
        real_stream_default_limit(&stream, StreamMode::Decoded),
        REAL_STREAM_DEFAULT_VIEW_LIMIT_BYTES
    );
}

#[test]
fn real_stream_loaded_label_reports_partial_and_complete_spans() {
    let partial = vec![StreamChunk {
        mode: StreamMode::Decoded,
        offset: 0,
        bytes: vec![b' '; 4096],
        total_size: Some(72_511),
        truncated: true,
        decode_diagnostics: Vec::new(),
    }];
    assert_eq!(real_stream_loaded_label(&partial), "4096 of 72511 bytes");
    assert!(real_stream_chunks_has_more(&partial));

    let complete = vec![StreamChunk {
        mode: StreamMode::Decoded,
        offset: 0,
        bytes: vec![b' '; 2860],
        total_size: Some(2860),
        truncated: false,
        decode_diagnostics: Vec::new(),
    }];
    assert_eq!(real_stream_loaded_label(&complete), "2860 bytes");
}

#[test]
fn real_stream_scroll_request_loads_adjacent_content_at_edges() {
    let chunks = vec![
        StreamChunk {
            mode: StreamMode::Decoded,
            offset: 4096,
            bytes: vec![b' '; 4096],
            total_size: Some(16_000),
            truncated: false,
            decode_diagnostics: Vec::new(),
        },
        StreamChunk {
            mode: StreamMode::Decoded,
            offset: 8192,
            bytes: vec![b' '; 4096],
            total_size: Some(16_000),
            truncated: true,
            decode_diagnostics: Vec::new(),
        },
    ];

    assert_eq!(
        real_stream_scroll_request(300.0, 100.0, 400.0, -24.0, true, &chunks, 4096),
        Some(12_288)
    );
    assert_eq!(
        real_stream_scroll_request(0.0, 100.0, 400.0, 24.0, true, &chunks, 4096),
        Some(0)
    );
    assert_eq!(
        real_stream_scroll_request(160.0, 100.0, 400.0, -24.0, true, &chunks, 4096),
        None
    );
    assert_eq!(
        real_stream_scroll_request(300.0, 100.0, 400.0, -24.0, false, &chunks, 4096),
        None
    );
}

#[test]
fn real_stream_window_cache_keeps_recent_neighbors_bounded() {
    let mut app = GuiShellApp::new();
    let object = ObjectId { num: 4, gen: 0 };
    for index in 0..(REAL_STREAM_MAX_LOADED_WINDOWS + 2) {
        let offset = (index * 64) as u64;
        let key = RealStreamKey {
            object,
            mode: StreamMode::Decoded,
            offset,
            limit: 64,
        };
        app.insert_real_stream_window(
            key,
            StreamChunk {
                mode: StreamMode::Decoded,
                offset,
                bytes: vec![b' '; 64],
                total_size: Some(1024),
                truncated: true,
                decode_diagnostics: Vec::new(),
            },
        );
    }

    assert_eq!(
        app.real_stream_windows.len(),
        REAL_STREAM_MAX_LOADED_WINDOWS
    );
    assert_eq!(app.real_stream_windows.front().unwrap().key.offset, 2 * 64);
    assert_eq!(
        app.real_stream_windows.back().unwrap().key.offset,
        (REAL_STREAM_MAX_LOADED_WINDOWS + 1) as u64 * 64
    );
}

#[test]
fn real_stream_view_presets_choose_nice_text_and_raw_hex() {
    assert_eq!(
        real_stream_preset_defaults(RealStreamPreset::Nice, true),
        (StreamMode::Decoded, StreamViewMode::Text)
    );
    assert_eq!(
        real_stream_preset_defaults(RealStreamPreset::Nice, false),
        (StreamMode::Raw, StreamViewMode::Text)
    );
    assert_eq!(
        real_stream_preset_defaults(RealStreamPreset::Raw, true),
        (StreamMode::Raw, StreamViewMode::Hex)
    );
}

#[test]
fn image_streams_default_to_raw_preset() {
    let mut stream = StreamSummary {
        object: ObjectId { num: 4, gen: 0 },
        filters: vec!["DCTDecode".to_string()],
        raw_size_hint: Some(1308),
        decoded_size_hint: Some(17_400),
        can_decode: true,
        image_preview_available: true,
    };
    assert_eq!(real_stream_initial_preset(&stream), RealStreamPreset::Raw);

    stream.image_preview_available = false;
    assert_eq!(real_stream_initial_preset(&stream), RealStreamPreset::Nice);
}

#[test]
fn render_result_color_image_compacts_padded_stride() {
    let render = RenderResult {
        page_index: 0,
        width: 1,
        height: 2,
        stride: 8,
        pixels_rgba: vec![1, 2, 3, 4, 99, 99, 99, 99, 5, 6, 7, 8, 88, 88, 88, 88],
        duration_ms: 0,
        diagnostics: Vec::new(),
    };

    let image = render_result_color_image(&render).unwrap();
    assert_eq!(image.size, [1, 2]);
    assert_eq!(image.pixels[0], Color32::from_rgba_unmultiplied(1, 2, 3, 4));
    assert_eq!(image.pixels[1], Color32::from_rgba_unmultiplied(5, 6, 7, 8));
}

#[test]
fn selected_hex_text_is_copy_authority() {
    let mut model = LargeStreamModel {
        selection_offset: 1024,
        selection_len: 16 * 1024,
        ..LargeStreamModel::default()
    };
    model.selected_hex_text = Some("00000000  11 30 4f 6e".to_string());
    let escaped = model.escaped_copy_text();
    assert!(!escaped.truncated);
    assert_eq!(escaped.text, "00000000  11 30 4f 6e");
    assert_eq!(model.copy_source_label(), "selected hex text");
}

#[test]
fn read_only_hex_window_resets_to_canonical_dump() {
    let mut model = LargeStreamModel::default();
    let canonical = model.hex_text.clone();
    model.hex_text.push_str("mutated");
    model.reset_hex_window();
    assert_eq!(model.hex_text, canonical);
}

#[test]
fn option_text_hides_rust_debug_wrappers() {
    assert_eq!(option_text(Some("fake-hash")), "fake-hash");
    assert_eq!(option_text(None), "-");
}

#[test]
fn real_tree_row_anatomy_uses_kind_count_ref_preview_and_diagnostics() {
    let summary = ObjectSummary {
        id: NodeId::XrefObject {
            doc: pdbg_core::DocumentId(7),
            object: ObjectId { num: 12, gen: 0 },
        },
        kind: ObjectKind::Dict,
        label: "Info".to_string(),
        preview: "<< /Creator (...) >>".to_string(),
        object: Some(ObjectId { num: 12, gen: 0 }),
        has_children: true,
        has_stream: true,
        child_count: Some(3),
        byte_size_hint: None,
        diagnostics: vec![pdbg_core::DiagnosticSummary {
            severity: DiagnosticSeverity::Warning,
            code: pdbg_core::DiagnosticCode::RepairWarning,
            message: "repaired".to_string(),
            node: None,
            page_index: None,
            object: Some(ObjectId { num: 12, gen: 0 }),
        }],
    };
    let tree = RealObjectTree::from_child_page(&pdbg_core::ChildPage {
        total: Some(1),
        items: vec![summary],
    });

    assert_eq!(tree.row_count_label(), "1 object");
    assert_eq!(tree.row_depth(0), Some(0));
    assert_eq!(tree.row_depth(1), Some(1));
    assert_eq!(tree.row_tree_marker(0), Some("-"));
    assert!(tree.row_label(1).contains("[12 0 R]"));
    let job = tree.row_layout_job(1, false);
    let row_text = job.text;
    assert!(row_text.contains("<> Info (3) [12 0 R]"));
    assert!(!row_text.contains("preview"));
    assert!(row_text.contains("stream"));
}

#[test]
fn real_tree_detail_refresh_keeps_structural_label_stable() {
    let id = NodeId::DocumentRoot {
        doc: pdbg_core::DocumentId(7),
    };
    let summary = ObjectSummary {
        id: id.clone(),
        kind: ObjectKind::Unknown,
        label: "Trailer".to_string(),
        preview: "PDF trailer dictionary".to_string(),
        object: None,
        has_children: true,
        has_stream: false,
        child_count: Some(4),
        byte_size_hint: None,
        diagnostics: Vec::new(),
    };
    let mut tree = RealObjectTree {
        rows: vec![RealTreeRow {
            summary,
            depth: 0,
            expanded: false,
        }],
        root_children: Vec::new(),
        total: Some(1),
    };
    let detail = ObjectDetail {
        id,
        kind: ObjectKind::Trailer,
        object: None,
        label: "Object".to_string(),
        preview: "<object preview exceeds max depth>".to_string(),
        value: ObjectValue::Container,
        dictionary_entries: Some(pdbg_core::ChildPage {
            total: Some(4),
            items: Vec::new(),
        }),
        array_entries: None,
        stream: None,
        diagnostics: Vec::new(),
    };

    tree.update_row_from_detail(0, &detail);

    let row_text = tree.row_layout_job(0, false).text;
    assert!(row_text.contains("trl Trailer (4)"));
    assert!(!row_text.contains("Object"));
    assert_eq!(tree.row_label(0), "PDF trailer dictionary");
}

#[test]
fn real_tree_page_rows_use_dictionary_badge_before_detail_load() {
    let doc = pdbg_core::DocumentId(9);
    let summary = ObjectSummary {
        id: NodeId::ArrayEntry {
            doc: doc.clone(),
            parent: Box::new(NodeId::PageRoot { doc: doc.clone() }),
            index: 3,
        },
        kind: ObjectKind::Page,
        label: "Page 4".to_string(),
        preview: String::new(),
        object: None,
        has_children: true,
        has_stream: false,
        child_count: None,
        byte_size_hint: None,
        diagnostics: Vec::new(),
    };
    let tree = RealObjectTree::from_child_page(&pdbg_core::ChildPage {
        total: Some(1),
        items: vec![summary],
    });

    let row_text = tree.row_layout_job(1, false).text;
    assert!(row_text.starts_with("<> Page 4"));
    assert!(!row_text.starts_with("page Page 4"));
}

#[test]
fn real_tree_xref_object_rows_use_object_badge_before_detail_load() {
    let doc = pdbg_core::DocumentId(10);
    let summary = ObjectSummary {
        id: NodeId::XrefObject {
            doc,
            object: ObjectId { num: 4, gen: 0 },
        },
        kind: ObjectKind::XrefEntry,
        label: "Object 4 0 R".to_string(),
        preview: String::new(),
        object: Some(ObjectId { num: 4, gen: 0 }),
        has_children: true,
        has_stream: false,
        child_count: Some(5),
        byte_size_hint: None,
        diagnostics: Vec::new(),
    };
    let tree = RealObjectTree::from_child_page(&pdbg_core::ChildPage {
        total: Some(1),
        items: vec![summary],
    });

    let row_text = tree.row_layout_job(1, false).text;
    assert!(row_text.starts_with("<> Object 4 0 R"));
    assert!(!row_text.starts_with("xref Object 4 0 R"));
}

#[test]
fn node_breadcrumb_formats_public_path_segments() {
    let id = NodeId::DictEntry {
        doc: pdbg_core::DocumentId(1),
        parent: Box::new(NodeId::ArrayEntry {
            doc: pdbg_core::DocumentId(1),
            parent: Box::new(NodeId::PageRoot {
                doc: pdbg_core::DocumentId(1),
            }),
            index: 0,
        }),
        key: "Contents".to_string(),
    };

    assert_eq!(node_breadcrumb(&id), "Pages/[0]/Contents");
}

#[test]
fn page_index_for_node_follows_page_children() {
    let id = NodeId::DictEntry {
        doc: pdbg_core::DocumentId(1),
        parent: Box::new(NodeId::ArrayEntry {
            doc: pdbg_core::DocumentId(1),
            parent: Box::new(NodeId::Page {
                doc: pdbg_core::DocumentId(1),
                index: 3,
            }),
            index: 0,
        }),
        key: "Contents".to_string(),
    };

    assert_eq!(page_index_for_node(&id), Some(3));
    assert_eq!(
        page_index_for_node(&NodeId::ArrayEntry {
            doc: pdbg_core::DocumentId(1),
            parent: Box::new(NodeId::DictEntry {
                doc: pdbg_core::DocumentId(1),
                parent: Box::new(NodeId::Catalog {
                    doc: pdbg_core::DocumentId(1),
                }),
                key: "Pages".to_string(),
            }),
            index: 1,
        }),
        Some(1)
    );
    assert_eq!(
        page_index_for_node(&NodeId::ResourceGroup {
            doc: pdbg_core::DocumentId(1),
            page_index: 2,
            group: pdbg_core::ResourceGroup::XObjects,
        }),
        Some(2)
    );
    assert_eq!(
        page_index_for_node(&NodeId::Trailer {
            doc: pdbg_core::DocumentId(1),
        }),
        None
    );
}

#[test]
fn page_row_node_detection_ignores_page_child_nodes() {
    let doc = pdbg_core::DocumentId(1);
    let page = NodeId::ArrayEntry {
        doc: doc.clone(),
        parent: Box::new(NodeId::PageRoot { doc: doc.clone() }),
        index: 2,
    };
    let media_box = NodeId::DictEntry {
        doc,
        parent: Box::new(page.clone()),
        key: "MediaBox".to_string(),
    };

    assert!(is_page_row_node_for_index(&page, 2));
    assert!(!is_page_row_node_for_index(&media_box, 2));
}

#[test]
fn recent_pdf_paths_are_deduped_bounded_and_persisted() {
    let recent_path = temp_recent_file_path("round-trip");
    let dir = recent_path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    let tmp_path = unique_recent_tmp_path(&recent_path);
    assert_eq!(tmp_path.parent(), recent_path.parent());
    assert_ne!(tmp_path, recent_path.with_extension("tmp"));

    let first = dir.join("first.pdf");
    let second = dir.join("second.pdf");
    std::fs::write(&first, b"%PDF-1.7\n").unwrap();
    std::fs::write(&second, b"%PDF-1.7\n").unwrap();

    let mut recent = Vec::new();
    assert!(record_recent_pdf_path(
        &mut recent,
        &first.to_string_lossy()
    ));
    assert!(record_recent_pdf_path(
        &mut recent,
        &second.to_string_lossy()
    ));
    assert!(record_recent_pdf_path(
        &mut recent,
        &first.to_string_lossy()
    ));
    assert_eq!(recent.len(), 2);
    assert_eq!(
        recent[0],
        first.canonicalize().unwrap().to_string_lossy().to_string()
    );
    assert!(!record_recent_pdf_path(&mut recent, "bad\npath.pdf"));

    for index in 0..(RECENT_PDF_MAX_ITEMS + 4) {
        let path = dir.join(format!("extra-{index}.pdf"));
        std::fs::write(&path, b"%PDF-1.7\n").unwrap();
        record_recent_pdf_path(&mut recent, &path.to_string_lossy());
    }
    assert_eq!(recent.len(), RECENT_PDF_MAX_ITEMS);

    save_recent_pdf_paths_to(&recent_path, &recent).unwrap();
    let loaded = load_recent_pdf_paths_from(&recent_path);
    assert_eq!(loaded, recent);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn hex_dump_rows_align_offset_hex_and_ascii_columns() {
    let bytes: Vec<u8> = (0u8..20).chain(*b"Hi~\x7f").collect();

    let full = hex_dump_row(0x10, &bytes[..16]);
    assert!(full.starts_with("00000010  00 01 02 03 "));
    assert!(full.ends_with("  ................"));

    let partial = hex_dump_row(0x20, &bytes[16..]);
    // Short rows pad the hex column so the ASCII column starts at the same
    // position: 8 offset digits + 2 spaces + 16 * 3 hex cells + 1 space.
    let ascii_start = 8 + 2 + 16 * 3 + 1;
    assert_eq!(&full[ascii_start..], "................");
    assert_eq!(&partial[ascii_start..], "....Hi~.");

    let dump = hex_dump_bytes(0x10, &bytes);
    assert_eq!(dump.lines().count(), 2);
    assert!(dump.starts_with("00000010  "));
    assert!(dump.contains("\n00000020  "));
}

#[test]
fn hex_jump_offsets_parse_decimal_and_hex() {
    assert_eq!(parse_hex_jump_offset("1024"), Some(1024));
    assert_eq!(parse_hex_jump_offset(" 0x400 "), Some(0x400));
    assert_eq!(parse_hex_jump_offset("0XFF"), Some(255));
    assert_eq!(parse_hex_jump_offset(""), None);
    assert_eq!(parse_hex_jump_offset("0x"), None);
    assert_eq!(parse_hex_jump_offset("12g"), None);
    assert_eq!(parse_hex_jump_offset("-4"), None);
}

#[test]
fn xref_entry_location_labels_cover_all_kinds() {
    let object = ObjectId { num: 7, gen: 0 };
    let free = XrefEntryInfo {
        object: ObjectId { num: 0, gen: 65535 },
        kind: XrefEntryKind::Free,
        offset: 0,
        objstm_index: None,
        section: None,
    };
    let normal = XrefEntryInfo {
        object,
        kind: XrefEntryKind::Normal,
        offset: 1234,
        objstm_index: None,
        section: Some(0),
    };
    let compressed = XrefEntryInfo {
        object,
        kind: XrefEntryKind::Compressed,
        offset: 12,
        objstm_index: Some(3),
        section: Some(2),
    };

    assert_eq!(xref_entry_location_label(&free), "—");
    assert_eq!(xref_entry_location_label(&normal), "@ 1234");
    assert_eq!(xref_entry_location_label(&compressed), "objstm 12 [3]");
    assert_eq!(XrefEntryKind::Compressed.as_public_str(), "compressed");

    assert_eq!(xref_entry_section_label(&free, 1), "—");
    assert_eq!(xref_entry_section_label(&normal, 1), "0");
    assert_eq!(xref_entry_section_label(&normal, 3), "0");
    assert_eq!(xref_entry_section_label(&compressed, 3), "2 (latest)");
}

#[test]
fn ui_settings_round_trip_and_validation() {
    let recent_path = temp_recent_file_path("ui-settings");
    let dir = recent_path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    let settings_path = dir.join("ui-settings.txt");

    let settings = UiSettings {
        dark_mode: true,
        left_panel_width: Some(300.0),
        right_panel_width: Some(420.0),
        render_zoom: Some(2.0),
    };
    save_ui_settings_to(&settings_path, &settings).unwrap();
    assert_eq!(load_ui_settings_from(&settings_path), settings);

    // Missing file falls back to defaults.
    assert_eq!(
        load_ui_settings_from(&dir.join("missing.txt")),
        UiSettings::default()
    );

    // Out-of-range widths clamp; unknown zoom levels and junk are rejected.
    std::fs::write(
        &settings_path,
        "dark_mode=false\nleft_panel_width=10000\nright_panel_width=junk\nrender_zoom=2.7\n",
    )
    .unwrap();
    let loaded = load_ui_settings_from(&settings_path);
    assert!(!loaded.dark_mode);
    assert_eq!(loaded.left_panel_width, Some(LEFT_PANEL_MAX_WIDTH));
    assert_eq!(loaded.right_panel_width, None);
    assert_eq!(loaded.render_zoom, None);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn displayed_file_paths_neutralize_controls() {
    let path = format!("/tmp/report{}fdp.pdf", '\u{202e}');
    let label = display_file_chip_label(&path);
    let hover = display_path_hover(&path);

    assert!(!label.contains('\u{202e}'));
    assert!(!hover.contains('\u{202e}'));
    assert!(label.contains('\u{fffd}'));
    assert!(hover.contains('\u{fffd}'));
    assert_eq!(normalize_recent_pdf_path(&path), Some(path));
}

#[test]
fn displayed_file_paths_decode_common_html_entities() {
    let label = display_file_chip_label(r#"C:\tmp\123&quot;asdasdas.pdf"#);

    assert_eq!(label, "123\"asdasdas.pdf");
    assert!(!label.contains("&quot;"));
}

#[test]
fn page_preview_tint_dims_only_in_dark_mode() {
    set_dark_mode(false);
    assert_eq!(page_preview_image_tint(), Color32::WHITE);

    set_dark_mode(true);
    let dark_tint = page_preview_image_tint();
    assert!(dark_tint.r() < Color32::WHITE.r());
    assert_eq!(dark_tint.r(), dark_tint.g());
    assert_eq!(dark_tint.g(), dark_tint.b());

    set_dark_mode(false);
}

#[test]
fn inspector_image_preview_tint_matches_page_preview_tint() {
    set_dark_mode(false);
    assert_eq!(inspector_image_preview_tint(), page_preview_image_tint());

    set_dark_mode(true);
    assert_eq!(inspector_image_preview_tint(), page_preview_image_tint());

    set_dark_mode(false);
}

#[test]
fn gui_empty_workspace_starts_without_fake_document() {
    let app = GuiShellApp::new_with_options(GuiRunOptions {
        start_empty_when_no_pdf: true,
        ..GuiRunOptions::default()
    });

    assert!(app.empty_workspace);
    assert!(app.state.is_err());
    assert_eq!(app.page_count(), 0);
    assert!(app.real_render_job.is_none());
    assert_eq!(app.window_title(), APP_TITLE);
    assert_eq!(app.breadcrumb_label(), "No document");
    assert!(app.status_log.iter().any(|line| line == "No PDF open"));
}

#[test]
fn gui_window_title_reflects_document_and_pending_open() {
    let mut app = GuiShellApp::new();
    assert_eq!(app.window_title(), format!("fake.pdf - {APP_TITLE}"));

    app.open_pdf_from_path("fixtures/synthetic/minimal.pdf".to_string());
    assert!(app.open_pdf_job.is_some());
    assert_eq!(
        app.window_title(),
        format!("Opening minimal.pdf - {APP_TITLE}")
    );
    app.cancel_open_pdf_job();
}

#[cfg(not(feature = "real-mupdf"))]
#[test]
fn gui_open_pdf_without_real_mupdf_keeps_current_document() {
    let recent_path = temp_recent_file_path("fake-open");
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        recent_files_path: Some(recent_path),
        start_empty_when_no_pdf: false,
        ..GuiRunOptions::default()
    });
    let initial_row = app.tree.row_label(0);

    app.open_pdf_from_path("fixtures/synthetic/minimal.pdf".to_string());
    assert!(app.open_pdf_job.is_some());
    wait_for_open_pdf(&mut app);

    assert_eq!(app.tree.row_label(0), initial_row);
    assert!(app
        .open_pdf_error
        .as_deref()
        .is_some_and(|err| err.contains("requires building pdbg-app")));
    assert!(app.recent_pdf_paths.is_empty());
}

#[test]
fn gui_open_pdf_cancel_discards_pending_job() {
    let recent_path = temp_recent_file_path("cancel-open");
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        recent_files_path: Some(recent_path),
        start_empty_when_no_pdf: false,
        ..GuiRunOptions::default()
    });

    app.open_pdf_from_path("fixtures/synthetic/minimal.pdf".to_string());
    assert!(app.open_pdf_job.is_some());
    app.cancel_open_pdf_job();

    assert!(app.open_pdf_job.is_none());
    assert_eq!(app.open_pdf_error.as_deref(), Some("open cancelled"));
    assert!(app
        .status_log
        .iter()
        .any(|line| line.starts_with("discarded pending open ")));
}

#[cfg(feature = "real-mupdf")]
#[test]
fn real_gui_open_pdf_action_replaces_document_and_records_recent() {
    let recent_path = temp_recent_file_path("real-open");
    let path = write_temp_pdf("gui-open-action", &synthetic_two_page_pdf());
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: None,
        recent_files_path: Some(recent_path.clone()),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });
    assert!(!app.tree.is_real());

    app.open_pdf_from_path(path.to_string_lossy().to_string());
    assert!(app.open_pdf_job.is_some());
    wait_for_open_pdf(&mut app);
    wait_for_real_render(&mut app);

    assert!(app.tree.is_real());
    assert_eq!(app.page_count(), 2);
    assert!(app.open_pdf_error.is_none());
    assert!(!app.open_pdf_dialog_open);
    let canonical = path.canonicalize().unwrap().to_string_lossy().to_string();
    assert_eq!(
        app.window_title(),
        format!("{} - {APP_TITLE}", display_file_chip_label(&canonical))
    );
    assert_eq!(app.recent_pdf_paths.first(), Some(&canonical));
    assert_eq!(
        load_recent_pdf_paths_from(&recent_path).first(),
        Some(&canonical)
    );

    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(recent_path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn real_gui_open_pdf_prompts_for_password_and_retries() {
    let recent_path = temp_recent_file_path("real-password-open");
    let path = encrypted_minimal_pdf_path("gui-password-open");
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: None,
        recent_files_path: Some(recent_path.clone()),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });
    let initial_row = app.tree.row_label(0);

    app.open_pdf_from_path(path.to_string_lossy().to_string());
    assert!(app.open_pdf_job.is_some());
    wait_for_open_pdf(&mut app);

    assert_eq!(app.tree.row_label(0), initial_row);
    assert_eq!(app.open_pdf_error.as_deref(), Some("Password required"));
    assert!(app.open_pdf_dialog_open);
    assert!(app.recent_pdf_paths.is_empty());

    app.open_pdf_password_input = "user".to_string();
    app.open_pdf_from_path(path.to_string_lossy().to_string());
    assert!(app.open_pdf_job.is_some());
    wait_for_open_pdf(&mut app);
    wait_for_real_render(&mut app);

    assert!(app.tree.is_real());
    assert!(app.open_pdf_error.is_none());
    assert!(!app.open_pdf_dialog_open);
    assert!(app.open_pdf_password_input.is_empty());
    let canonical = path.canonicalize().unwrap().to_string_lossy().to_string();
    assert_eq!(app.recent_pdf_paths.first(), Some(&canonical));

    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(recent_path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn real_gui_model_loads_bounded_tree_and_detail_from_pdf_path() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/synthetic/minimal.pdf"
    );
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: Some(fixture.to_string()),
        recent_files_path: Some(temp_recent_file_path("gui-isolated")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });
    assert!(app.real_render_job.is_some());
    wait_for_real_render(&mut app);

    assert!(app.state.is_ok());
    assert!(matches!(app.tree, TreeModel::Real(_)));
    assert_eq!(app.tree.row_count(), 5);
    assert_eq!(app.tree.row_count_label(), "4 objects");
    assert!(app.real_detail.is_some());
    let pages = app.real_pages.as_ref().unwrap();
    assert_eq!(pages.total, Some(1));
    assert_eq!(pages.items[0].label, "Page 1");
    assert!(app.real_render.is_some());
    assert!(app.breadcrumb_label().contains("Root"));
    assert!(app.status_log[0].contains("real MuPDF opened"));
    assert!(app
        .status_log
        .iter()
        .any(|line| line.starts_with("loaded page list")));
    assert!(app
        .status_log
        .iter()
        .any(|line| line.starts_with("queued page 1")));
    assert!(app
        .status_log
        .iter()
        .any(|line| line.starts_with("rendered page 1")));
}

#[cfg(feature = "real-mupdf")]
#[test]
fn real_gui_large_pdf_smoke_keeps_tree_bounded_and_records_timings() {
    let path = write_temp_pdf("gui-large", &synthetic_large_xref_pdf(1_500));
    let open_start = Instant::now();
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: Some(path.to_string_lossy().to_string()),
        recent_files_path: Some(temp_recent_file_path("gui-isolated")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });
    wait_for_real_render(&mut app);
    let open_elapsed = open_start.elapsed();

    let xref_size = app
        .state
        .as_ref()
        .unwrap()
        .panels
        .summary
        .as_ref()
        .unwrap()
        .xref_size;
    assert!(xref_size > 1_000);
    assert_eq!(app.tree.row_count(), 5);

    let xref_expand_start = Instant::now();
    app.select_row_from_tree(4);
    let xref_elapsed = xref_expand_start.elapsed();
    assert_eq!(app.tree.real_row_tree_marker(app.selected_row), Some("-"));
    assert!(app.tree.row_count() > 5);
    assert!(app.tree.row_count() < xref_size / 10);

    let pages_expand_start = Instant::now();
    app.select_row_from_tree(3);
    let pages_elapsed = pages_expand_start.elapsed();
    assert_eq!(app.tree.real_row_tree_marker(app.selected_row), Some("-"));
    assert!(app.tree.real_row_for_page_index(0).is_some());

    let jump_start = Instant::now();
    app.follow_real_reference(ObjectId { num: 1, gen: 0 });
    let jump_elapsed = jump_start.elapsed();
    assert_eq!(
        app.real_detail.as_ref().and_then(|detail| detail.object),
        Some(ObjectId { num: 1, gen: 0 })
    );
    assert!(app.tree.row_count() < xref_size / 10);
    assert!(open_elapsed < Duration::from_secs(5));
    assert!(xref_elapsed < Duration::from_secs(2));
    assert!(pages_elapsed < Duration::from_secs(2));
    assert!(jump_elapsed < Duration::from_secs(2));

    app.status_log.push(format!(
        "large smoke timings: open={}ms xref_expand={}ms pages_expand={}ms jump={}ms",
        open_elapsed.as_millis(),
        xref_elapsed.as_millis(),
        pages_elapsed.as_millis(),
        jump_elapsed.as_millis()
    ));
    assert!(app
        .status_log
        .iter()
        .any(|line| line.starts_with("large smoke timings:")));

    let _ = std::fs::remove_file(path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn real_gui_stream_panel_loads_bounded_real_bytes() {
    let path = write_temp_pdf("gui-stream", &synthetic_large_xref_pdf(16));
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: Some(path.to_string_lossy().to_string()),
        recent_files_path: Some(temp_recent_file_path("gui-isolated")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });
    wait_for_real_render(&mut app);

    app.follow_real_reference(ObjectId { num: 4, gen: 0 });
    assert!(app
        .real_detail
        .as_ref()
        .is_some_and(|detail| detail.stream.is_some()));
    app.real_stream_mode = StreamMode::Raw;
    app.real_stream_limit = 16;
    app.refresh_real_stream_chunk(ObjectId { num: 4, gen: 0 });
    assert!(app.real_stream_job.is_some());
    wait_for_real_stream(&mut app);

    let chunk = app.real_stream_chunk.as_ref().unwrap();
    assert_eq!(chunk.mode, StreamMode::Raw);
    assert_eq!(chunk.offset, 0);
    assert!(chunk.bytes.starts_with(b"BT /F1"));
    assert!(chunk.truncated);
    assert!(app
        .status_log
        .iter()
        .any(|line| line.contains("queued raw stream chunk 4 0 R")));
    assert!(app
        .status_log
        .iter()
        .any(|line| line.contains("loaded raw stream chunk 4 0 R")));

    let _ = std::fs::remove_file(path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn real_gui_decoded_stream_cache_reuses_loaded_chunk() {
    let path = write_temp_pdf("gui-stream-cache", &synthetic_large_xref_pdf(16));
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: Some(path.to_string_lossy().to_string()),
        recent_files_path: Some(temp_recent_file_path("gui-isolated")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });
    wait_for_real_render(&mut app);

    let object = ObjectId { num: 4, gen: 0 };
    app.follow_real_reference(object);
    app.real_stream_mode = StreamMode::Decoded;
    app.real_stream_limit = 16;
    app.refresh_real_stream_chunk(object);
    assert!(app.real_stream_job.is_some());
    wait_for_real_stream(&mut app);
    assert_eq!(
        app.real_stream_chunk.as_ref().unwrap().mode,
        StreamMode::Decoded
    );
    assert_eq!(app.decoded_stream_cache.len(), 1);

    app.clear_real_stream_chunk();
    app.refresh_real_stream_chunk(object);

    assert!(app.real_stream_job.is_none());
    assert_eq!(
        app.real_stream_chunk.as_ref().unwrap().mode,
        StreamMode::Decoded
    );
    assert!(app
        .status_log
        .iter()
        .any(|line| { line.contains("reused cached decoded stream chunk 4 0 R @ 0") }));

    let _ = std::fs::remove_file(path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn real_gui_stream_job_can_be_cancelled_from_ui_state() {
    let path = write_temp_pdf("gui-stream-cancel", &synthetic_large_xref_pdf(16));
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: Some(path.to_string_lossy().to_string()),
        recent_files_path: Some(temp_recent_file_path("gui-isolated")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });
    wait_for_real_render(&mut app);

    app.follow_real_reference(ObjectId { num: 4, gen: 0 });
    app.real_stream_mode = StreamMode::Raw;
    app.real_stream_limit = 16;
    app.refresh_real_stream_chunk(ObjectId { num: 4, gen: 0 });
    assert!(app.real_stream_job.is_some());

    app.cancel_real_stream_job();
    assert!(app.real_stream_job.is_none());
    assert!(app.real_stream_chunk.is_none());
    assert_eq!(
        app.real_stream_error.as_deref(),
        Some("stream chunk load cancelled")
    );
    assert!(app
        .status_log
        .iter()
        .any(|line| line.contains("cancelled raw stream chunk 4 0 R")));

    let _ = std::fs::remove_file(path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn real_gui_page_controls_refresh_render_parameters() {
    let path = write_temp_pdf("gui-pages", &synthetic_two_page_pdf());
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: Some(path.to_string_lossy().to_string()),
        recent_files_path: Some(temp_recent_file_path("gui-isolated")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });
    wait_for_real_render(&mut app);

    assert_eq!(app.page_count(), 2);
    assert_eq!(app.real_pages.as_ref().unwrap().total, Some(2));
    assert_eq!(app.real_pages.as_ref().unwrap().items[1].label, "Page 2");
    let initial = app.real_render.as_ref().unwrap();
    assert_eq!(initial.page_index, 0);
    assert_eq!((initial.width, initial.height), (200, 100));

    app.render_zoom = 2.0;
    app.refresh_real_render();
    wait_for_real_render(&mut app);
    let zoomed = app.real_render.as_ref().unwrap();
    assert_eq!(zoomed.page_index, 0);
    assert_eq!((zoomed.width, zoomed.height), (400, 200));

    app.render_zoom = 1.0;
    app.refresh_real_render();
    wait_for_real_render(&mut app);
    let reset_zoom = app.real_render.as_ref().unwrap();
    assert_eq!(reset_zoom.page_index, 0);
    assert_eq!((reset_zoom.width, reset_zoom.height), (200, 100));

    app.set_render_page(1);
    wait_for_real_render(&mut app);
    let second_page = app.real_render.as_ref().unwrap();
    assert_eq!(second_page.page_index, 1);
    assert_eq!((second_page.width, second_page.height), (100, 200));

    app.set_render_page(99);
    assert_eq!(app.render_page_index, 1);
    assert!(app
        .status_log
        .iter()
        .any(|line| line.starts_with("rendered page 2 @ 100%")));

    let _ = std::fs::remove_file(path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn real_gui_selecting_page_tree_row_refreshes_preview_page() {
    let path = write_temp_pdf("gui-tree-page-sync", &synthetic_two_page_pdf());
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: Some(path.to_string_lossy().to_string()),
        recent_files_path: Some(temp_recent_file_path("gui-isolated")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });
    wait_for_real_render(&mut app);

    let page_root_row = match &app.tree {
        TreeModel::Real(tree) => tree
            .rows
            .iter()
            .position(|row| {
                matches!(
                    &row.summary.id,
                    NodeId::DictEntry { key, .. } if key == "Pages"
                ) || matches!(&row.summary.id, NodeId::PageRoot { .. })
            })
            .unwrap(),
        TreeModel::Virtual(_) => panic!("expected real tree"),
    };
    app.select_row_from_tree(page_root_row);
    app.expand_selected_real_row();
    let page_two_row = match &app.tree {
        TreeModel::Real(tree) => tree
            .rows
            .iter()
            .position(|row| page_index_for_node(&row.summary.id) == Some(1))
            .unwrap(),
        TreeModel::Virtual(_) => panic!("expected real tree"),
    };

    app.select_row_from_tree(page_two_row);
    wait_for_real_render(&mut app);

    assert_eq!(app.render_page_index, 1);
    assert_eq!(app.real_render.as_ref().unwrap().page_index, 1);

    let _ = std::fs::remove_file(path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn real_gui_pager_expands_and_selects_matching_page_tree_row() {
    let path = write_temp_pdf("gui-pager-tree-sync", &synthetic_two_page_pdf());
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: Some(path.to_string_lossy().to_string()),
        recent_files_path: Some(temp_recent_file_path("gui-isolated")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });
    wait_for_real_render(&mut app);

    assert!(app.tree.real_row_for_page_index(1).is_none());
    let page_root_row = app.tree.real_page_root_row().unwrap();
    let non_page_row = (0..app.tree.row_count())
        .find(|row| *row != page_root_row && app.tree.real_row_tree_marker(*row) == Some("+"))
        .unwrap();
    let non_page_id = match &app.tree {
        TreeModel::Real(tree) => tree
            .summary(non_page_row)
            .map(|summary| summary.id.clone())
            .unwrap(),
        TreeModel::Virtual(_) => panic!("expected real tree"),
    };
    app.expand_real_tree_row(non_page_row);
    let expanded_non_page_row = match &app.tree {
        TreeModel::Real(tree) => tree.row_for_node(&non_page_id).unwrap(),
        TreeModel::Virtual(_) => panic!("expected real tree"),
    };
    assert_eq!(
        app.tree.real_row_tree_marker(expanded_non_page_row),
        Some("-")
    );

    app.set_render_page_from_pager(1);
    wait_for_real_render(&mut app);

    let page_row = app.tree.real_row_for_page_index(1).unwrap();
    let page_root_row = app.tree.real_page_root_row().unwrap();
    let collapsed_non_page_row = match &app.tree {
        TreeModel::Real(tree) => tree.row_for_node(&non_page_id).unwrap(),
        TreeModel::Virtual(_) => panic!("expected real tree"),
    };
    assert_eq!(app.render_page_index, 1);
    assert_eq!(app.selected_row, page_row);
    assert_eq!(app.tree.real_row_tree_marker(page_root_row), Some("-"));
    assert_eq!(app.tree.real_row_tree_marker(page_row), Some("-"));
    assert_eq!(
        app.tree.real_row_tree_marker(collapsed_non_page_row),
        Some("+")
    );
    assert!(app.scroll_selected_tree_row);

    let _ = std::fs::remove_file(path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn real_gui_render_cache_reuses_previous_page() {
    let path = write_temp_pdf("gui-render-cache", &synthetic_two_page_pdf());
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: Some(path.to_string_lossy().to_string()),
        recent_files_path: Some(temp_recent_file_path("gui-isolated")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });
    wait_for_real_render(&mut app);

    app.set_render_page(1);
    wait_for_real_render(&mut app);
    assert_eq!(app.render_cache.len(), 2);

    app.set_render_page(0);

    assert!(app.real_render_job.is_none());
    let render = app.real_render.as_ref().unwrap();
    assert_eq!(render.page_index, 0);
    assert_eq!((render.width, render.height), (200, 100));
    assert!(app
        .status_log
        .iter()
        .any(|line| line.starts_with("reused cached page 1 @ 100%")));

    let _ = std::fs::remove_file(path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn real_gui_render_job_replacement_keeps_latest_page() {
    let path = write_temp_pdf("gui-render-replace", &synthetic_two_page_pdf());
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: Some(path.to_string_lossy().to_string()),
        recent_files_path: Some(temp_recent_file_path("gui-isolated")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });
    assert!(app.real_render_job.is_some());

    app.set_render_page(1);
    wait_for_real_render(&mut app);

    let render = app.real_render.as_ref().unwrap();
    assert_eq!(render.page_index, 1);
    assert_eq!((render.width, render.height), (100, 200));
    assert!(app
        .status_log
        .iter()
        .any(|line| line.starts_with("queued page 2")));

    let _ = std::fs::remove_file(path);
}

#[test]
fn reference_navigation_uses_back_forward_history() {
    let mut app = GuiShellApp::new();
    assert_eq!(app.selected_row, 0);

    app.follow_reference(42);
    assert_eq!(app.selected_row, 42);
    assert_eq!(app.back_stack, vec![0]);
    assert!(app.forward_stack.is_empty());

    app.go_back();
    assert_eq!(app.selected_row, 0);
    assert_eq!(app.forward_stack, vec![42]);
}

#[test]
fn tree_selection_uses_back_forward_history() {
    let mut app = GuiShellApp::new();
    assert_eq!(app.selected_row, 0);

    app.select_row_from_tree(2);
    assert_eq!(app.selected_row, 2);
    assert_eq!(app.back_stack, vec![0]);
    assert!(app.forward_stack.is_empty());

    app.go_back();
    assert_eq!(app.selected_row, 0);
    assert_eq!(app.forward_stack, vec![2]);

    app.go_forward();
    assert_eq!(app.selected_row, 2);
    assert_eq!(app.back_stack, vec![0]);

    app.go_back();
    app.select_row_from_tree(3);
    assert_eq!(app.selected_row, 3);
    assert!(app.forward_stack.is_empty());
}

#[test]
fn smoke_exit_option_is_stored_for_native_launch_tests() {
    let app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: Some(Duration::from_millis(250)),
        pdf_path: None,
        recent_files_path: Some(temp_recent_file_path("gui-isolated")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });
    assert_eq!(app.smoke_exit_after, Some(Duration::from_millis(250)));
}

#[test]
fn fake_shell_keeps_mock_page_preview() {
    let app = GuiShellApp::new();
    assert!(!app.tree.is_real());
    assert!(app.real_pages.is_none());
    assert!(app.real_pages_error.is_none());
    assert!(app.real_render.is_none());
    assert!(app.real_render_error.is_none());
    assert!(!app
        .status_log
        .iter()
        .any(|line| line.starts_with("loaded page list")));
    assert!(!app
        .status_log
        .iter()
        .any(|line| line.starts_with("rendered page preview")));
}

#[test]
fn theme_defines_named_font_stacks_and_severity_colors() {
    let fonts = pdbg_fonts();
    assert!(fonts.font_data.contains_key("InterVariable"));
    assert!(fonts.font_data.contains_key("JetBrainsMono-Regular"));
    assert!(fonts
        .families
        .contains_key(&FontFamily::Name("pdbg-sans".into())));
    assert!(fonts
        .families
        .contains_key(&FontFamily::Name("pdbg-mono".into())));

    let style = pdbg_style();
    assert_eq!(style.visuals.panel_fill, theme().panel);
    assert_eq!(
        theme().severity_fg(&DiagnosticSeverity::Warning),
        theme().warn_fg
    );
    assert_eq!(
        theme().severity_fg(&DiagnosticSeverity::Error),
        theme().error_fg
    );
}

fn temp_recent_file_path(prefix: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!(
            "pdbg-app-{}-{}-{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
        .join("recent-files.txt")
}

#[cfg(feature = "real-mupdf")]
fn write_temp_pdf(prefix: &str, bytes: &[u8]) -> std::path::PathBuf {
    let temp_path = std::env::temp_dir().join(format!(
        "pdbg-app-{}-{}-{}.pdf",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&temp_path, bytes).unwrap();
    temp_path
}

#[cfg(feature = "real-mupdf")]
fn mutool_path() -> PathBuf {
    if let Some(path) = std::env::var_os("PDBG_MUTOOL_PATH") {
        return PathBuf::from(path);
    }
    let source_dir = std::env::var_os("PDBG_MUPDF_SOURCE_DIR")
        .expect("real encrypted GUI test requires PDBG_MUPDF_SOURCE_DIR or PDBG_MUTOOL_PATH");
    let path = PathBuf::from(source_dir)
        .join("build")
        .join("release")
        .join("mutool");
    assert!(
            path.is_file(),
            "real encrypted GUI test requires mutool at {}; build it with `make build=release build/release/mutool` or set PDBG_MUTOOL_PATH",
            path.display()
        );
    path
}

#[cfg(feature = "real-mupdf")]
fn encrypted_minimal_pdf_path(prefix: &str) -> PathBuf {
    let input = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/synthetic/minimal.pdf"
    );
    let output = std::env::temp_dir().join(format!(
        "pdbg-app-{}-{}-{}.pdf",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let status = std::process::Command::new(mutool_path())
        .args([
            "clean", "-E", "aes-128", "-O", "owner", "-U", "user", "-P", "0", input,
        ])
        .arg(&output)
        .status()
        .expect("failed to run mutool");
    assert!(
        status.success(),
        "mutool failed to create encrypted GUI fixture"
    );
    output
}

#[cfg(feature = "real-mupdf")]
fn synthetic_large_xref_pdf(object_count: usize) -> Vec<u8> {
    let object_count = object_count.max(8);
    let mut pdf = String::from("%PDF-1.7\n");
    let mut offsets = vec![0usize; object_count + 1];
    let push_object = |pdf: &mut String, offsets: &mut [usize], number: usize, body: &str| {
        offsets[number] = pdf.len();
        pdf.push_str(&format!("{number} 0 obj\n{body}\nendobj\n"));
    };

    push_object(
        &mut pdf,
        &mut offsets,
        1,
        "<< /Type /Catalog /Pages 2 0 R /Names 5 0 R >>",
    );
    push_object(
        &mut pdf,
        &mut offsets,
        2,
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
    );
    push_object(
        &mut pdf,
        &mut offsets,
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>",
    );
    let stream = "BT /F1 12 Tf 24 120 Td (pdbg large smoke) Tj ET";
    push_object(
        &mut pdf,
        &mut offsets,
        4,
        &format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            stream.len(),
            stream
        ),
    );
    push_object(&mut pdf, &mut offsets, 5, "<< /Dests 6 0 R >>");
    for number in 6..=object_count {
        let prev = if number == 6 { 1 } else { number - 1 };
        push_object(
            &mut pdf,
            &mut offsets,
            number,
            &format!("<< /Index {number} /Prev {prev} 0 R /Name /Node{number} >>"),
        );
    }

    let xref_offset = pdf.len();
    pdf.push_str(&format!("xref\n0 {}\n", object_count + 1));
    pdf.push_str("0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Root 1 0 R /Size {} >>\nstartxref\n{xref_offset}\n%%EOF\n",
        object_count + 1
    ));
    pdf.into_bytes()
}

#[cfg(feature = "real-mupdf")]
fn synthetic_image_pdf() -> Vec<u8> {
    fn push_obj(pdf: &mut String, offsets: &mut Vec<usize>, body: &str) {
        offsets.push(pdf.len());
        pdf.push_str(body);
    }

    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    push_obj(
        &mut pdf,
        &mut offsets,
        "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] \
         /Resources << /XObject << /Im0 4 0 R >> >> >>\nendobj\n",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        "4 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 2 \
         /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 12 >>\n\
         stream\nAAABBBCCCDDD\nendstream\nendobj\n",
    );

    let xref_offset = pdf.len();
    pdf.push_str("xref\n0 5\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Root 1 0 R /Size 5 >>\nstartxref\n{xref_offset}\n%%EOF\n"
    ));
    pdf.into_bytes()
}

#[cfg(feature = "real-mupdf")]
fn synthetic_form_xobject_image_pdf() -> Vec<u8> {
    fn push_obj(pdf: &mut String, offsets: &mut Vec<usize>, body: &str) {
        offsets.push(pdf.len());
        pdf.push_str(body);
    }

    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    push_obj(
        &mut pdf,
        &mut offsets,
        "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] \
         /Resources << /XObject << /Fm0 5 0 R >> >> /Contents 6 0 R >>\nendobj\n",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        "4 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 2 \
         /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 12 >>\n\
         stream\nAAABBBCCCDDD\nendstream\nendobj\n",
    );
    let form_stream = "/Im0 Do";
    push_obj(
        &mut pdf,
        &mut offsets,
        &format!(
            "5 0 obj\n<< /Type /XObject /Subtype /Form /BBox [0 0 72 72] \
             /Resources << /XObject << /Im0 4 0 R >> >> /Length {} >>\n\
             stream\n{}\nendstream\nendobj\n",
            form_stream.len(),
            form_stream
        ),
    );
    let page_stream = "q /Fm0 Do Q";
    push_obj(
        &mut pdf,
        &mut offsets,
        &format!(
            "6 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
            page_stream.len(),
            page_stream
        ),
    );

    let xref_offset = pdf.len();
    pdf.push_str("xref\n0 7\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Root 1 0 R /Size 7 >>\nstartxref\n{xref_offset}\n%%EOF\n"
    ));
    pdf.into_bytes()
}

#[test]
fn export_file_names_pick_native_extensions() {
    let object = ObjectId { num: 7, gen: 0 };
    assert_eq!(
        suggested_export_file_name(object, StreamMode::Raw, &["DCTDecode".to_string()]),
        "object-7-0-raw.jpg"
    );
    assert_eq!(
        suggested_export_file_name(object, StreamMode::Raw, &["JPXDecode".to_string()]),
        "object-7-0-raw.jp2"
    );
    assert_eq!(
        suggested_export_file_name(object, StreamMode::Raw, &["FlateDecode".to_string()]),
        "object-7-0-raw.bin"
    );
    assert_eq!(
        suggested_export_file_name(
            object,
            StreamMode::Raw,
            &["FlateDecode".to_string(), "DCTDecode".to_string()]
        ),
        "object-7-0-raw.bin"
    );
    assert_eq!(
        suggested_export_file_name(object, StreamMode::Decoded, &["DCTDecode".to_string()]),
        "object-7-0-decoded.bin"
    );
}

#[test]
fn nice_stream_do_resource_name_extracts_xobject_name() {
    assert_eq!(
        nice_stream_do_resource_name("/Image6 Do"),
        Some("Image6".to_string())
    );
    assert_eq!(
        nice_stream_do_resource_name("  /Im0   Do  "),
        Some("Im0".to_string())
    );
    assert_eq!(nice_stream_do_resource_name("(Text) Tj"), None);
    assert_eq!(nice_stream_do_resource_name("Do"), None);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn stream_export_writes_full_raw_bytes_to_file() {
    let pdf_path = write_temp_pdf("export-src", &synthetic_image_pdf());
    let state = open_app_state(Some(pdf_path.to_string_lossy().as_ref()), None).unwrap();
    let out_path = std::env::temp_dir().join(format!(
        "pdbg-export-{}-{}.bin",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let key = StreamExportKey {
        object: ObjectId { num: 4, gen: 0 },
        mode: StreamMode::Raw,
        path: out_path.to_string_lossy().into_owned(),
    };
    let cancel = CancelToken::new().unwrap();
    let outcome = stream_export_worker(state.session.clone(), &key, &cancel).unwrap();

    assert_eq!(outcome.bytes_written, 12);
    assert!(!outcome.capped);
    assert_eq!(std::fs::read(&out_path).unwrap(), b"AAABBBCCCDDD");

    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&pdf_path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn selected_do_resource_resolves_to_image_xobject() {
    let pdf_path = write_temp_pdf("do-resource-resolve", &synthetic_image_pdf());
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: Some(pdf_path.to_string_lossy().to_string()),
        recent_files_path: Some(temp_recent_file_path("gui-do-resource")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });

    let resolved = app.resolve_page_xobject_resource(0, "Im0").unwrap();
    assert_eq!(resolved.object, ObjectId { num: 4, gen: 0 });
    assert!(resolved.is_image);
    assert_eq!(resolved.subtype.as_deref(), Some("Image"));

    let err = app.resolve_page_xobject_resource(0, "Missing").unwrap_err();
    assert!(err.contains("no /Missing entry"), "got: {err}");

    let _ = std::fs::remove_file(&pdf_path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn selected_do_resource_resolves_against_stream_resources_first() {
    let pdf_path = write_temp_pdf(
        "do-resource-form-xobject",
        &synthetic_form_xobject_image_pdf(),
    );
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: Some(pdf_path.to_string_lossy().to_string()),
        recent_files_path: Some(temp_recent_file_path("gui-do-resource-form")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });

    let (resolved, source) = app
        .resolve_stream_xobject_resource(ObjectId { num: 5, gen: 0 }, Some(0), "Im0")
        .unwrap();
    assert_eq!(resolved.object, ObjectId { num: 4, gen: 0 });
    assert!(resolved.is_image);
    assert_eq!(resolved.subtype.as_deref(), Some("Image"));
    assert_eq!(source, "stream 5 0 R");

    let form = app.resolve_page_xobject_resource(0, "Fm0").unwrap();
    assert_eq!(form.object, ObjectId { num: 5, gen: 0 });
    assert!(!form.is_image);
    assert_eq!(form.subtype.as_deref(), Some("Form"));
    assert_eq!(form.type_label(), "Form XObject");

    let _ = std::fs::remove_file(&pdf_path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn stream_export_into_missing_directory_fails_cleanly() {
    let pdf_path = write_temp_pdf("export-badpath", &synthetic_image_pdf());
    let state = open_app_state(Some(pdf_path.to_string_lossy().as_ref()), None).unwrap();
    let out_path = std::env::temp_dir()
        .join(format!("pdbg-no-such-dir-{}", std::process::id()))
        .join("export.bin");

    let key = StreamExportKey {
        object: ObjectId { num: 4, gen: 0 },
        mode: StreamMode::Raw,
        path: out_path.to_string_lossy().into_owned(),
    };
    let cancel = CancelToken::new().unwrap();
    let err = stream_export_worker(state.session.clone(), &key, &cancel).unwrap_err();
    assert!(err.contains("cannot create export file"), "got: {err}");
    assert!(!out_path.exists());

    let _ = std::fs::remove_file(&pdf_path);
}

#[cfg(feature = "real-mupdf")]
#[test]
fn image_preview_decodes_for_image_objects() {
    let pdf_path = write_temp_pdf("image-preview-gui", &synthetic_image_pdf());
    let mut app = GuiShellApp::new_with_options(GuiRunOptions {
        smoke_exit_after: None,
        pdf_path: Some(pdf_path.to_string_lossy().to_string()),
        recent_files_path: Some(temp_recent_file_path("gui-isolated")),
        start_empty_when_no_pdf: false,
        render_max_dimension: None,
    });

    let image_object = ObjectId { num: 4, gen: 0 };
    app.ensure_image_preview(image_object);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.image_preview_job.is_some() && Instant::now() < deadline {
        app.poll_image_preview_job();
        if app.image_preview_job.is_some() {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    app.poll_image_preview_job();

    let (object, preview) = app
        .image_preview_result
        .as_ref()
        .expect("image preview should decode");
    assert_eq!(*object, image_object);
    assert_eq!((preview.width, preview.height), (2, 2));
    assert_eq!(&preview.pixels_rgba[0..4], &[0x41, 0x41, 0x41, 0xFF]);

    // Re-keying to a non-image object reports an error instead of a preview.
    app.ensure_image_preview(ObjectId { num: 1, gen: 0 });
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.image_preview_job.is_some() && Instant::now() < deadline {
        app.poll_image_preview_job();
        if app.image_preview_job.is_some() {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    app.poll_image_preview_job();
    assert!(app.image_preview_result.is_none());
    assert!(app
        .image_preview_error
        .as_ref()
        .is_some_and(|(object, _)| object.num == 1));

    let _ = std::fs::remove_file(&pdf_path);
}

#[cfg(feature = "real-mupdf")]
fn synthetic_two_page_pdf() -> Vec<u8> {
    let mut pdf = String::from("%PDF-1.7\n");
    let mut offsets = vec![0usize; 7];
    let push_object = |pdf: &mut String, offsets: &mut [usize], number: usize, body: &str| {
        offsets[number] = pdf.len();
        pdf.push_str(&format!("{number} 0 obj\n{body}\nendobj\n"));
    };

    push_object(
        &mut pdf,
        &mut offsets,
        1,
        "<< /Type /Catalog /Pages 2 0 R >>",
    );
    push_object(
        &mut pdf,
        &mut offsets,
        2,
        "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>",
    );
    push_object(
        &mut pdf,
        &mut offsets,
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents 5 0 R >>",
    );
    push_object(
        &mut pdf,
        &mut offsets,
        4,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 200] /Contents 6 0 R >>",
    );
    push_object(
        &mut pdf,
        &mut offsets,
        5,
        "<< /Length 0 >>\nstream\n\nendstream",
    );
    push_object(
        &mut pdf,
        &mut offsets,
        6,
        "<< /Length 0 >>\nstream\n\nendstream",
    );

    let xref_offset = pdf.len();
    pdf.push_str("xref\n0 7\n0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Root 1 0 R /Size 7 >>\nstartxref\n{xref_offset}\n%%EOF\n"
    ));
    pdf.into_bytes()
}
