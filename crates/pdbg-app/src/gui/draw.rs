use super::*;

impl GuiShellApp {
    pub(crate) fn draw_workspace(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let available_rect = ui.available_rect_before_wrap();
        if available_rect.width() <= 0.0 || available_rect.height() <= 0.0 {
            return;
        }

        let layout = workspace_panel_layout(available_rect.width());
        let mut left_width = self.left_panel_width.unwrap_or(layout.left.default);
        let mut right_width = self.right_panel_width.unwrap_or(layout.right.default);
        clamp_workspace_widths(
            &mut left_width,
            &mut right_width,
            layout,
            available_rect.width(),
        );

        let rects = workspace_rects(available_rect, left_width, right_width);
        let left_splitter_response = ui.interact(
            rects.left_splitter,
            egui::Id::new("workspace_left_splitter"),
            egui::Sense::drag(),
        );
        let right_splitter_response = ui.interact(
            rects.right_splitter,
            egui::Id::new("workspace_right_splitter"),
            egui::Sense::drag(),
        );
        let pointer_delta_x = ui.input(|input| input.pointer.delta().x);
        if left_splitter_response.dragged() {
            left_width += pointer_delta_x;
        }
        if right_splitter_response.dragged() {
            right_width -= pointer_delta_x;
        }
        clamp_workspace_widths(
            &mut left_width,
            &mut right_width,
            layout,
            available_rect.width(),
        );
        self.left_panel_width = Some(left_width);
        self.right_panel_width = Some(right_width);

        let rects = workspace_rects(available_rect, left_width, right_width);
        show_framed_child(ui, rects.left, panel_frame(), |ui| self.draw_tree(ui));
        show_framed_child(
            ui,
            rects.center,
            egui::Frame::new()
                .fill(theme().canvas)
                .inner_margin(egui::Margin::symmetric(10, 10)),
            |ui| self.draw_page_preview(ui),
        );
        show_framed_child(ui, rects.right, panel_frame(), |ui| {
            self.draw_inspector(ui, ctx)
        });
        draw_workspace_splitter(ui, rects.left_splitter, &left_splitter_response);
        draw_workspace_splitter(ui, rects.right_splitter, &right_splitter_response);
        ui.advance_cursor_after_rect(available_rect);
    }

    pub(crate) fn draw_recent_file_list(
        &self,
        ui: &mut egui::Ui,
        id_salt: &'static str,
        max_height: f32,
    ) -> Option<String> {
        if self.recent_pdf_paths.is_empty() {
            ui.label(RichText::new("No recent files").color(theme().muted));
            return None;
        }

        let mut path_to_open = None;
        ScrollArea::vertical()
            .id_salt(id_salt)
            .max_height(max_height)
            .show(ui, |ui| {
                for path in self.recent_pdf_paths.clone() {
                    let label = display_file_chip_label(&path);
                    let row_width = ui.available_width().max(160.0);
                    if ui
                        .add_sized(
                            egui::vec2(row_width, 28.0),
                            egui::Button::selectable(false, ())
                                .left_text(RichText::new(label).size(12.0).color(theme().text))
                                .truncate(),
                        )
                        .on_hover_text(display_path_hover(&path))
                        .clicked()
                    {
                        path_to_open = Some(path);
                    }
                }
            });
        path_to_open
    }

    pub(crate) fn handle_dropped_pdf_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        let Some(path) = dropped
            .into_iter()
            .filter_map(|file| file.path)
            .find(|path| is_pdf_path(path))
        else {
            return;
        };
        self.open_pdf_from_path(path.to_string_lossy().into_owned());
    }

    pub(crate) fn open_pdf_with_file_dialog(&mut self) {
        if let Some(path) = choose_pdf_file() {
            self.open_pdf_dialog_open = false;
            self.open_pdf_password_input.clear();
            self.open_pdf_error = None;
            self.open_pdf_from_path(path);
        }
    }

    pub(crate) fn draw_open_pdf_dialog(&mut self, ctx: &egui::Context) {
        if !self.open_pdf_dialog_open {
            return;
        }

        let mut path_to_open = None;
        let mut choose_another = false;
        let mut close_requested = false;
        let opening = self.open_pdf_job.is_some();
        let password_required = self.open_pdf_error.as_deref() == Some("Password required");
        let show_password = password_required || !self.open_pdf_password_input.is_empty();
        let title = if show_password {
            "Password Required"
        } else {
            "Open PDF Failed"
        };

        let modal_response = egui::Modal::new(egui::Id::new("open_pdf_dialog"))
            .backdrop_color(egui::Color32::from_black_alpha(72))
            .frame(
                egui::Frame::new()
                    .fill(theme().surface)
                    .stroke(egui::Stroke::new(1.0, theme().strong_border))
                    .corner_radius(10)
                    .inner_margin(egui::Margin::same(0))
                    .shadow(egui::Shadow {
                        offset: [0, 16],
                        blur: 32,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(46),
                    }),
            )
            .show(ctx, |ui| {
                ui.set_width(440.0);
                egui::Frame::new()
                    .fill(theme().panel)
                    .inner_margin(egui::Margin::symmetric(18, 14))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(title).strong().size(17.0).color(theme().text));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_sized(
                                            egui::vec2(28.0, 28.0),
                                            egui::Button::new(
                                                RichText::new("×").size(18.0).color(theme().muted),
                                            )
                                            .fill(theme().chip_bg),
                                        )
                                        .on_hover_text("Close")
                                        .clicked()
                                    {
                                        close_requested = true;
                                    }
                                },
                            );
                        });
                    });

                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(18, 16))
                    .show(ui, |ui| {
                        if !self.open_pdf_path_input.trim().is_empty() {
                            ui.label(RichText::new("PDF file").small().color(theme().muted));
                            ui.add_space(4.0);
                            egui::Frame::new()
                                .fill(theme().code_bg)
                                .stroke(egui::Stroke::new(1.0, theme().border))
                                .corner_radius(6)
                                .inner_margin(egui::Margin::symmetric(10, 7))
                                .show(ui, |ui| {
                                    truncated_label(
                                        ui,
                                        RichText::new(display_file_chip_label(
                                            &self.open_pdf_path_input,
                                        ))
                                        .color(theme().text),
                                        ui.available_width(),
                                        Some(&display_path_hover(&self.open_pdf_path_input)),
                                    );
                                });
                            ui.add_space(12.0);
                        }

                        if let Some(err) = &self.open_pdf_error {
                            let message = if err == "Password required" {
                                "This PDF requires a password."
                            } else {
                                err
                            };
                            egui::Frame::new()
                                .fill(theme().error_bg)
                                .stroke(egui::Stroke::new(1.0, theme().error_fg))
                                .corner_radius(6)
                                .inner_margin(egui::Margin::symmetric(10, 8))
                                .show(ui, |ui| {
                                    ui.colored_label(theme().error_fg, message);
                                });
                        }

                        if show_password {
                            ui.add_space(10.0);
                            ui.label(RichText::new("Password").small().color(theme().muted));
                            ui.add_space(4.0);
                            let response = ui.add_enabled(
                                !opening,
                                TextEdit::singleline(&mut self.open_pdf_password_input)
                                    .desired_width(ui.available_width())
                                    .password(true)
                                    .hint_text("Enter PDF password"),
                            );
                            if !opening
                                && response.lost_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Enter))
                            {
                                path_to_open = Some(self.open_pdf_path_input.clone());
                            }
                            if !opening && !ctx.memory(|memory| memory.has_focus(response.id)) {
                                response.request_focus();
                            }
                        }

                        ui.add_space(18.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if opening {
                                if ui.button("Cancel open").clicked() {
                                    self.cancel_open_pdf_job();
                                    close_requested = true;
                                    self.open_pdf_password_input.clear();
                                }
                                ui.label(RichText::new("Opening PDF...").color(theme().muted));
                            } else if show_password {
                                let can_unlock = !self.open_pdf_path_input.trim().is_empty();
                                if ui
                                    .add_enabled(
                                        can_unlock,
                                        egui::Button::new(
                                            RichText::new("Unlock").color(theme().surface),
                                        )
                                        .fill(theme().accent)
                                        .min_size(egui::vec2(78.0, 30.0)),
                                    )
                                    .clicked()
                                {
                                    path_to_open = Some(self.open_pdf_path_input.clone());
                                }
                                if ui.button("Cancel").clicked() {
                                    close_requested = true;
                                    self.open_pdf_password_input.clear();
                                }
                            } else {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Choose another...")
                                                .color(theme().surface),
                                        )
                                        .fill(theme().accent)
                                        .min_size(egui::vec2(128.0, 30.0)),
                                    )
                                    .clicked()
                                {
                                    choose_another = true;
                                }
                                if ui.button("Close").clicked() {
                                    close_requested = true;
                                }
                            }
                        });
                    });
            });

        if modal_response.should_close() {
            close_requested = true;
        }
        if choose_another {
            self.open_pdf_password_input.clear();
            self.open_pdf_error = None;
            self.open_pdf_with_file_dialog();
        }
        if close_requested && self.open_pdf_job.is_some() {
            self.cancel_open_pdf_job();
            self.open_pdf_password_input.clear();
        }
        if choose_another || close_requested {
            self.open_pdf_dialog_open = false;
        }
        if let Some(path) = path_to_open {
            self.open_pdf_from_path(path);
        }
    }

    pub(crate) fn draw_about_dialog(&mut self, ctx: &egui::Context) {
        if !self.about_dialog_open {
            return;
        }

        let mut close_requested = false;
        let mupdf_version = mupdf_version_label();
        let backend = backend_label(&mupdf_version);
        let platform = platform_label();
        let render_limit = format!("{} px max dimension", self.render_max_dimension);
        let about_text = about_info_text(&backend, &platform, &render_limit);
        let modal_response = egui::Modal::new(egui::Id::new("about_dialog"))
            .backdrop_color(egui::Color32::from_black_alpha(72))
            .frame(
                egui::Frame::new()
                    .fill(theme().surface)
                    .stroke(egui::Stroke::new(1.0, theme().strong_border))
                    .corner_radius(10)
                    .inner_margin(egui::Margin::same(0))
                    .shadow(egui::Shadow {
                        offset: [0, 16],
                        blur: 32,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(46),
                    }),
            )
            .show(ctx, |ui| {
                ui.set_width(520.0);
                egui::Frame::new()
                    .fill(theme().panel)
                    .inner_margin(egui::Margin::symmetric(22, 16))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("Scalpel")
                                        .strong()
                                        .size(22.0)
                                        .color(theme().text),
                                );
                                ui.add_space(2.0);
                                ui.label(
                                    RichText::new("PDF structure dissection tool")
                                        .size(12.5)
                                        .color(theme().muted),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_sized(
                                            egui::vec2(28.0, 28.0),
                                            egui::Button::new(
                                                RichText::new("×").size(18.0).color(theme().muted),
                                            )
                                            .fill(theme().chip_bg),
                                        )
                                        .on_hover_text("Close")
                                        .clicked()
                                    {
                                        close_requested = true;
                                    }
                                },
                            );
                        });
                    });

                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(22, 18))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(
                                "Dissect object trees, streams, xref tables, rendered pages, text positions, image resources, and diagnostics.",
                            )
                            .size(13.5)
                            .color(theme().muted),
                        );
                        ui.add_space(18.0);

                        egui::Frame::new()
                            .fill(theme().code_bg)
                            .stroke(egui::Stroke::new(1.0, theme().border))
                            .corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(16, 13))
                            .show(ui, |ui| {
                                egui::Grid::new("about_info_grid")
                                    .num_columns(2)
                                    .spacing(egui::vec2(24.0, 10.0))
                                    .show(ui, |ui| {
                                        about_info_row(ui, "Version", env!("CARGO_PKG_VERSION"));
                                        about_info_row(ui, "Commit", build_commit_label());
                                        about_info_row(ui, "Release date", release_date_label());
                                        about_info_row(ui, "Backend", &backend);
                                        about_info_row(ui, "OS", &platform);
                                        about_info_row(ui, "Build", build_profile_label());
                                        about_info_row(ui, "Render limit", &render_limit);
                                        about_link_row(ui, "GitHub", APP_GITHUB_URL);
                                    });
                            });

                        ui.add_space(18.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Close").size(13.0).color(theme().surface),
                                    )
                                    .fill(theme().accent)
                                    .min_size(egui::vec2(88.0, 34.0)),
                                )
                                .clicked()
                            {
                                close_requested = true;
                            }
                            if ui
                                .add_sized(
                                    egui::vec2(92.0, 34.0),
                                    egui::Button::new(RichText::new("Copy Info").size(13.0)),
                                )
                                .clicked()
                            {
                                ctx.copy_text(about_text.clone());
                            }
                        });
                    });
            });

        if modal_response.should_close() || close_requested {
            self.about_dialog_open = false;
        }
    }

    pub(crate) fn open_pdf_from_path(&mut self, path: String) {
        let path = path.trim().to_string();
        if path.is_empty() {
            self.open_pdf_error = Some("PDF path is empty".to_string());
            return;
        }

        let password_owned = self.open_pdf_password_input.trim().to_string();
        let password = (!password_owned.is_empty()).then_some(password_owned);
        if let Some(job) = self.open_pdf_job.take() {
            self.status_log.push(format!(
                "discarded pending open {}",
                display_file_chip_label(job.key())
            ));
        }

        let worker_path = path.clone();
        self.open_pdf_job = Some(BackgroundJob::spawn_uncancellable(
            path.clone(),
            move || open_pdf_worker_result(worker_path, password),
        ));
        self.open_pdf_path_input = path.clone();
        self.open_pdf_error = None;
        self.status_log
            .push(format!("opening {}", display_file_chip_label(&path)));
    }

    pub(crate) fn cancel_open_pdf_job(&mut self) {
        if let Some(job) = self.open_pdf_job.take() {
            self.open_pdf_error = Some("open cancelled".to_string());
            self.status_log.push(format!(
                "discarded pending open {}",
                display_file_chip_label(job.key())
            ));
        }
    }

    pub(crate) fn poll_open_pdf_job(&mut self) {
        match self.open_pdf_job.as_ref().map(BackgroundJob::poll) {
            None | Some(JobPoll::Pending) => {}
            Some(JobPoll::Finished(output)) => {
                self.open_pdf_job = None;
                match output.result {
                    Ok(OpenPdfJobResult::Opened(model)) => {
                        self.apply_opened_pdf_model(*model);
                        self.open_pdf_path_input = output.key.clone();
                        self.open_pdf_password_input.clear();
                        self.open_pdf_dialog_open = false;
                        self.open_pdf_error = None;
                        self.record_recent_pdf_path(&output.key);
                    }
                    Ok(OpenPdfJobResult::NeedsPassword) => {
                        self.open_pdf_path_input = output.key;
                        self.open_pdf_dialog_open = true;
                        self.open_pdf_error = Some("Password required".to_string());
                        self.status_log
                            .push("document requires a password before inspection".to_string());
                    }
                    Err(err) => {
                        self.open_pdf_error = Some(err.clone());
                        self.open_pdf_dialog_open = true;
                        self.status_log.push(format!(
                            "failed to open {}: {err}",
                            display_file_chip_label(&output.key)
                        ));
                    }
                }
            }
            Some(JobPoll::Disconnected(path)) => {
                self.open_pdf_job = None;
                self.open_pdf_error = Some("open worker disconnected".to_string());
                self.open_pdf_dialog_open = true;
                self.status_log.push(format!(
                    "open {} worker disconnected",
                    display_file_chip_label(&path)
                ));
            }
        }
    }

    pub(crate) fn apply_opened_pdf_model(&mut self, model: OpenedPdfModel) {
        self.cancel_inflight_document_jobs();

        self.state = Ok(model.state);
        self.empty_workspace = false;
        self.tree = model.tree;
        self.stream = LargeStreamModel::default();
        self.real_stream_preset = RealStreamPreset::Nice;
        self.real_stream_mode = StreamMode::Decoded;
        self.real_stream_view_mode = StreamViewMode::Text;
        self.real_stream_offset = 0;
        self.real_stream_limit = REAL_STREAM_DEFAULT_VIEW_LIMIT_BYTES;
        self.real_stream_key = None;
        self.real_stream_chunk = None;
        self.real_stream_windows.clear();
        self.real_stream_error = None;
        self.decoded_stream_cache = StreamChunkCache::new(
            DECODED_STREAM_CACHE_MAX_ITEMS,
            DECODED_STREAM_CACHE_MAX_BYTES,
        );
        self.selected_row = self.tree.initial_selected_row();
        self.scroll_selected_tree_row = false;
        self.back_stack.clear();
        self.forward_stack.clear();
        self.selected_tab = InspectorTab::Object;
        self.real_detail = model.real_detail;
        self.real_detail_error = model.real_detail_error;
        self.real_pages = model.real_pages;
        self.real_pages_error = model.real_pages_error;
        self.render_page_index = 0;
        self.render_zoom = DEFAULT_RENDER_ZOOM;
        self.render_rotation_degrees = 0;
        self.real_render_key = None;
        self.real_render = None;
        self.real_render_error = None;
        self.real_render_texture = None;
        self.render_cache = RenderResultCache::new(RENDER_CACHE_MAX_ITEMS, RENDER_CACHE_MAX_BYTES);
        self.object_search_query.clear();
        self.object_search_result = None;
        self.object_search_error = None;
        self.text_search_query.clear();
        self.text_search_result = None;
        self.text_search_error = None;
        self.text_search_cache =
            TextPageCache::new(TEXT_SEARCH_CACHE_MAX_PAGES, TEXT_SEARCH_CACHE_MAX_BYTES);
        self.visual_page_cache = VisualPageCache::new(
            VISUAL_CLICK_CACHE_MAX_PAGES,
            VISUAL_CLICK_CACHE_MAX_ELEMENTS,
        );
        self.selected_text_hit = None;
        self.selected_visual_hit = None;
        self.pending_preview_stream_selection = None;
        self.preview_click = None;
        self.clear_xref_state();
        self.reset_hex_state();
        self.clear_image_preview();
        self.copied_excerpt = None;
        self.status_log = model.status_log;
        self.refresh_real_render();
    }

    pub(crate) fn cancel_inflight_document_jobs(&mut self) {
        // Dropping cancellable BackgroundJobs cancels their tokens; open jobs
        // are uncancellable and just detached from the UI state.
        self.open_pdf_job = None;
        self.real_stream_job = None;
        self.hex_job = None;
        self.image_preview_job = None;
        self.stream_export_job = None;
        self.real_render_job = None;
        self.object_search_job = None;
        self.text_search_job = None;
    }

    pub(crate) fn record_recent_pdf_path(&mut self, path: &str) {
        if !record_recent_pdf_path(&mut self.recent_pdf_paths, path) {
            return;
        }
        if let Err(err) = save_recent_pdf_paths_to(&self.recent_files_path, &self.recent_pdf_paths)
        {
            self.status_log
                .push(format!("recent file save failed: {err}"));
        }
    }

    pub(crate) fn draw_top_bar(&mut self, ui: &mut egui::Ui) {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), TOP_BAR_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.spacing_mut().button_padding = egui::vec2(10.0, 5.0);
                ui.spacing_mut().interact_size.y = TOP_BAR_BUTTON_HEIGHT;
                let brand = ui
                    .add(
                        egui::Label::new(
                            RichText::new("Scalpel")
                                .strong()
                                .size(15.0)
                                .color(theme().top_bar_text),
                        )
                        .sense(egui::Sense::click()),
                    )
                    .on_hover_text("About Scalpel");
                if brand.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if brand.clicked() {
                    self.about_dialog_open = true;
                }
                top_bar_separator(ui);
                if top_bar_button(ui, "Open PDF...", true).clicked() {
                    self.open_pdf_with_file_dialog();
                }
                if !self.recent_pdf_paths.is_empty() {
                    let mut recent_to_open = None;
                    ui.scope(|ui| {
                        // Match the top-bar buttons; the default widget fill is
                        // tuned for panels, not the bar.
                        let button = theme().top_bar_button;
                        let button_hover = theme().top_bar_button_hover;
                        let stroke = egui::Stroke::new(1.0, theme().border);
                        let widgets = &mut ui.visuals_mut().widgets;
                        widgets.inactive.weak_bg_fill = button;
                        widgets.inactive.bg_stroke = stroke;
                        widgets.hovered.weak_bg_fill = button_hover;
                        widgets.hovered.bg_stroke = stroke;
                        widgets.active.weak_bg_fill = button_hover;
                        widgets.active.bg_stroke = stroke;
                        widgets.open.weak_bg_fill = button_hover;
                        widgets.open.bg_stroke = stroke;
                        ui.menu_button(
                            RichText::new("Recent")
                                .size(12.0)
                                .color(theme().top_bar_text),
                            |ui| {
                                // Restore the app style so the dropdown stays light.
                                ui.set_style(pdbg_style());
                                for path in self.recent_pdf_paths.clone() {
                                    if ui
                                        .button(display_file_chip_label(&path))
                                        .on_hover_text(display_path_hover(&path))
                                        .clicked()
                                    {
                                        recent_to_open = Some(path);
                                        ui.close();
                                    }
                                }
                            },
                        );
                    });
                    if let Some(path) = recent_to_open {
                        self.open_pdf_from_path(path);
                    }
                }
                top_bar_separator(ui);
                if top_bar_icon_button(ui, "‹", !self.back_stack.is_empty(), "Back").clicked() {
                    self.go_back();
                }
                if top_bar_icon_button(ui, "›", !self.forward_stack.is_empty(), "Forward").clicked()
                {
                    self.go_forward();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (label, hover) = if dark_mode_enabled() {
                        ("Light", "Switch to the light theme")
                    } else {
                        ("Dark", "Switch to the dark theme")
                    };
                    if top_bar_button(ui, label, true)
                        .on_hover_text(hover)
                        .clicked()
                    {
                        self.set_theme(ui.ctx(), !dark_mode_enabled());
                    }
                });
            },
        );
    }

    pub(crate) fn set_theme(&mut self, ctx: &egui::Context, dark: bool) {
        set_dark_mode(dark);
        ctx.set_global_style(pdbg_style());
        self.save_ui_settings();
    }

    pub(crate) fn draw_status_bar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        ui.painter().hline(
            rect.x_range(),
            rect.top(),
            egui::Stroke::new(1.0, theme().border),
        );
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(self.breadcrumb_label())
                    .monospace()
                    .size(12.0)
                    .color(theme().muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !self.empty_workspace {
                    let (color, label, hover) = if self.safe_mode_active() {
                        (theme().safe, "Safe Mode", "Safe Mode on")
                    } else {
                        (theme().error_fg, "Safe Mode off", "Safe Mode off")
                    };
                    ui.label(RichText::new(label).size(11.0).color(theme().muted));
                    ui.add_space(3.0);
                    ui.label(RichText::new("●").size(11.0).color(color))
                        .on_hover_text(hover);
                }
                if let Some(hit) = self.selected_text_hit.clone() {
                    ui.add_space(16.0);
                    ui.label(
                        RichText::new(text_search_hit_summary(&hit))
                            .monospace()
                            .size(11.0)
                            .color(theme().accent),
                    )
                    .on_hover_text(text_search_hit_hover(&hit));
                }
            });
        });
    }

    pub(crate) fn draw_tree(&mut self, ui: &mut egui::Ui) {
        if self.empty_workspace {
            self.draw_empty_tree_panel(ui);
            return;
        }

        self.draw_object_search(ui);
        ui.add_space(8.0);
        self.draw_text_search(ui);
        ui.add_space(8.0);
        section_header(ui, "Document Tree", Some(&self.tree.row_count_label()));

        if self.tree.is_real() {
            self.draw_real_tree_rows(ui);
            return;
        }

        let row_height = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
        ScrollArea::vertical().show_rows(ui, row_height, self.tree.row_count(), |ui, range| {
            for row in range {
                let selected = row == self.selected_row;
                let job = self.tree.row_layout_job(row, selected);
                if ui.selectable_label(selected, job).clicked() {
                    self.select_row_from_tree(row);
                }
            }
        });
    }

    pub(crate) fn draw_real_tree_rows(&mut self, ui: &mut egui::Ui) {
        let mut clicked_row = None;
        let mut toggled_row = None;
        let scroll_selected = self.scroll_selected_tree_row;
        let mut scrolled_selected = false;
        ScrollArea::vertical()
            .id_salt("real_document_tree_rows")
            .show(ui, |ui| {
                for row in 0..self.tree.row_count() {
                    let selected = row == self.selected_row;
                    let depth = self.tree.real_row_depth(row).unwrap_or(0);
                    let marker = self.tree.real_row_tree_marker(row).unwrap_or(" ");
                    let job = self.tree.row_layout_job(row, selected);
                    let row_label = self.tree.row_label(row);

                    ui.horizontal(|ui| {
                        ui.add_space((depth as f32) * 18.0);
                        let marker_response = ui
                            .add(
                                egui::Label::new(dense_monospace_text(marker).color(if selected {
                                    theme().accent
                                } else {
                                    theme().muted
                                }))
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_text("Expand or collapse");
                        if marker_response.clicked() && marker != " " {
                            toggled_row = Some(row);
                        }

                        let row_width = ui.available_width().max(24.0);
                        let row_response = ui
                            .add_sized(
                                egui::vec2(row_width, ui.text_style_height(&TextStyle::Body) + 4.0),
                                egui::Button::selectable(selected, ())
                                    .left_text(job)
                                    .truncate(),
                            )
                            .on_hover_text(row_label);
                        if selected && scroll_selected {
                            row_response.scroll_to_me(Some(egui::Align::Center));
                            scrolled_selected = true;
                        }
                        if row_response.double_clicked() {
                            toggled_row = Some(row);
                        } else if row_response.clicked() {
                            clicked_row = Some(row);
                        }
                    });
                }
            });
        if let Some(row) = toggled_row {
            self.select_row_from_tree(row);
        } else if let Some(row) = clicked_row {
            self.select_row_from_tree(row);
        }
        if scrolled_selected {
            self.scroll_selected_tree_row = false;
        }
    }

    pub(crate) fn draw_empty_tree_panel(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "Open PDF", Some("No document"));
        section_frame().show(ui, |ui| {
            if ui.button("Open PDF...").clicked() {
                self.open_pdf_with_file_dialog();
            }
            ui.add_space(8.0);
            section_header(ui, "Recent Files", None);
            if let Some(path) = self.draw_recent_file_list(ui, "empty_tree_recent_paths", 220.0) {
                self.open_pdf_from_path(path);
            }
        });
    }

    pub(crate) fn draw_object_search(&mut self, ui: &mut egui::Ui) {
        let status = object_search_status_label(
            self.object_search_result.as_ref(),
            self.object_search_error.as_deref(),
            self.object_search_job.is_some(),
        );
        section_header(ui, "Object Search", Some(&status));

        let mut run_search = false;
        let mut cancel_search = false;
        let mut clear_search = false;
        section_frame().show(ui, |ui| {
            let controls = draw_search_controls(
                ui,
                &mut self.object_search_query,
                "object, key, name, scalar",
                self.object_search_job.is_some(),
            );
            run_search |= controls.submit;
            cancel_search |= controls.cancel;
            clear_search |= controls.clear;
        });

        if clear_search {
            self.cancel_object_search_job();
            self.object_search_query.clear();
            self.object_search_result = None;
            self.object_search_error = None;
        } else if cancel_search {
            self.cancel_object_search_job();
        } else if run_search {
            self.run_object_search();
        }

        if let Some(err) = &self.object_search_error {
            ui.add_space(6.0);
            ui.colored_label(theme().error_fg, err);
        }

        let mut clicked_hit = None;
        if let Some(result) = &self.object_search_result {
            if !result.hits.is_empty() {
                ui.add_space(6.0);
                ScrollArea::vertical()
                    .id_salt("object_search_results")
                    .max_height(170.0)
                    .show(ui, |ui| {
                        for hit in &result.hits {
                            let label = object_search_hit_summary(hit);
                            if ui
                                .selectable_label(false, dense_monospace_text(label))
                                .on_hover_text(node_label_for_hit(hit))
                                .clicked()
                            {
                                clicked_hit = Some(hit.clone());
                            }
                        }
                    });
            } else if self.object_search_error.is_none() {
                ui.add_space(6.0);
                ui.label(RichText::new("No matches").small().color(theme().muted));
            }
        }
        if let Some(hit) = clicked_hit {
            self.follow_object_search_hit(&hit);
        }
    }

    pub(crate) fn draw_text_search(&mut self, ui: &mut egui::Ui) {
        let status = text_search_status_label(
            self.text_search_result.as_ref(),
            self.text_search_error.as_deref(),
            self.text_search_job.is_some(),
        );
        section_header(ui, "Text Search", Some(&status));

        let mut run_search = false;
        let mut cancel_search = false;
        let mut clear_search = false;
        section_frame().show(ui, |ui| {
            let controls = draw_search_controls(
                ui,
                &mut self.text_search_query,
                "page text",
                self.text_search_job.is_some(),
            );
            run_search |= controls.submit;
            cancel_search |= controls.cancel;
            clear_search |= controls.clear;
        });

        if clear_search {
            self.cancel_text_search_job();
            self.text_search_query.clear();
            self.text_search_result = None;
            self.text_search_error = None;
            self.selected_text_hit = None;
            self.selected_visual_hit = None;
        } else if cancel_search {
            self.cancel_text_search_job();
        } else if run_search {
            self.start_text_search();
        }

        if let Some(err) = &self.text_search_error {
            ui.add_space(6.0);
            ui.colored_label(theme().error_fg, err);
        }

        let mut clicked_hit = None;
        if let Some(result) = &self.text_search_result {
            if !result.page_errors.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "{} pages had extraction errors",
                        result.page_errors.len()
                    ))
                    .small()
                    .color(theme().warn_fg),
                );
            }
            if !result.hits.is_empty() {
                ui.add_space(6.0);
                ScrollArea::vertical()
                    .id_salt("text_search_results")
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for hit in &result.hits {
                            let selected = self
                                .selected_text_hit
                                .as_ref()
                                .is_some_and(|selected| text_hits_same_position(selected, hit));
                            if ui
                                .selectable_label(
                                    selected,
                                    dense_monospace_text(text_search_hit_summary(hit)),
                                )
                                .on_hover_text(text_search_hit_hover(hit))
                                .clicked()
                            {
                                clicked_hit = Some(hit.clone());
                            }
                        }
                    });
            } else if self.text_search_error.is_none() && self.text_search_job.is_none() {
                ui.add_space(6.0);
                ui.label(RichText::new("No matches").small().color(theme().muted));
            }
        }
        if let Some(hit) = clicked_hit {
            self.follow_text_search_hit(&hit);
        }
    }

    pub(crate) fn draw_page_preview(&mut self, ui: &mut egui::Ui) {
        if self.empty_workspace {
            self.draw_empty_page_preview(ui);
            return;
        }

        if self.draw_real_page_preview(ui) {
            return;
        }

        let available = ui.available_size();
        let desired = egui::vec2(available.x.max(320.0), available.y.max(360.0));
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme().canvas);

        let max_page = rect.shrink2(egui::vec2(44.0, 34.0));
        let page_ratio = 0.72;
        let mut page_h = max_page.height();
        let mut page_w = page_h * page_ratio;
        if page_w > max_page.width() {
            page_w = max_page.width();
            page_h = page_w / page_ratio;
        }
        let page_rect = egui::Rect::from_center_size(max_page.center(), egui::vec2(page_w, page_h));
        let shadow_rect = page_rect.translate(egui::vec2(0.0, 3.0));
        painter.rect_filled(shadow_rect, 3.0, Color32::from_black_alpha(18));
        painter.rect_filled(page_rect, 3.0, theme().page);
        painter.rect_stroke(
            page_rect,
            3.0,
            egui::Stroke::new(1.0, theme().strong_border),
            egui::StrokeKind::Outside,
        );

        let content = page_rect.shrink(34.0);
        for index in 0..9 {
            let y = content.top() + index as f32 * 24.0;
            painter.line_segment(
                [
                    egui::pos2(content.left(), y),
                    egui::pos2(content.right() - (index % 3) as f32 * 54.0, y),
                ],
                egui::Stroke::new(2.5, Color32::from_rgb(88, 105, 120)),
            );
        }

        let highlight = egui::Rect::from_min_size(
            egui::pos2(content.left() + 36.0, content.top() + 190.0),
            egui::vec2(220.0, 86.0),
        );
        painter.rect_filled(
            highlight,
            2.0,
            Color32::from_rgba_premultiplied(215, 100, 53, 18),
        );
        painter.rect_stroke(
            highlight,
            2.0,
            egui::Stroke::new(2.0, theme().operator),
            egui::StrokeKind::Outside,
        );
        painter.text(
            highlight.left_top() + egui::vec2(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            self.selected_object_label(),
            egui::FontId::monospace(13.0),
            theme().text,
        );
    }

    pub(crate) fn draw_empty_page_preview(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_size();
        let desired = egui::vec2(available.x.max(320.0), available.y.max(360.0));
        section_frame().fill(theme().canvas).show(ui, |ui| {
            ui.set_min_size(desired);
            ui.vertical_centered(|ui| {
                ui.add_space((desired.y * 0.28).min(180.0));
                ui.label(
                    RichText::new("No PDF open")
                        .strong()
                        .size(16.0)
                        .color(theme().text),
                );
                ui.add_space(8.0);
                if ui.button("Open PDF...").clicked() {
                    self.open_pdf_with_file_dialog();
                }
                ui.add_space(12.0);
                ui.label(RichText::new("Drop PDF here").color(theme().muted));
                if !self.recent_pdf_paths.is_empty() {
                    ui.add_space(16.0);
                    section_header(ui, "Recent Files", None);
                    if let Some(path) =
                        self.draw_recent_file_list(ui, "empty_preview_recent_paths", 140.0)
                    {
                        self.open_pdf_from_path(path);
                    }
                }
            });
        });
    }

    pub(crate) fn draw_real_page_preview(&mut self, ui: &mut egui::Ui) -> bool {
        let content_rect = ui.available_rect_before_wrap();
        if self.real_render_job.is_some() {
            let available = ui.available_size();
            let desired = egui::vec2(
                available.x.max(PAGE_PREVIEW_MIN_WIDTH),
                available.y.max(PAGE_PREVIEW_MIN_HEIGHT),
            );
            let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, theme().canvas);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Rendering page...",
                egui::FontId::proportional(13.0),
                theme().muted,
            );
            self.draw_floating_preview_controls(ui, content_rect);
            return true;
        }
        if let Some(err) = &self.real_render_error {
            ui.colored_label(theme().error_fg, err.clone());
            self.draw_floating_preview_controls(ui, content_rect);
            return true;
        }
        let Some(render) = &self.real_render else {
            return false;
        };
        if self.real_render_texture.is_none() {
            let Some(image) = render_result_color_image(render) else {
                ui.colored_label(theme().error_fg, "render output has invalid RGBA layout");
                return true;
            };
            self.real_render_texture = Some(ui.ctx().load_texture(
                "real-page-preview",
                image,
                egui::TextureOptions::LINEAR,
            ));
        }
        let Some(texture) = &self.real_render_texture else {
            return false;
        };

        let available = ui.available_size();
        let texture_id = texture.id();
        let texture_size = texture.size_vec2();
        let render_page_index = render.page_index;
        let render_width = render.width;
        let render_height = render.height;
        let render_zoom = self.render_zoom;
        let render_rotation = self.render_rotation_degrees;
        let preview_click = self.preview_click;
        let selected_hit = self
            .selected_text_hit
            .as_ref()
            .filter(|hit| hit.page_index == render_page_index)
            .cloned();
        let selected_visual_hit = self
            .selected_visual_hit
            .as_ref()
            .filter(|hit| hit.page_index == render_page_index)
            .cloned();

        // The zoom/pager controls are drawn above the preview, so the image
        // can use the full remaining height; nothing is rendered below it.
        let display_size = page_preview_display_size(texture_size, available, 0.0, render_zoom);
        let image_area_height = available.y.max(1.0);
        ScrollArea::both()
            .id_salt("real_page_preview_scroll")
            .max_height(image_area_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(page_preview_leading_space(
                        ui.available_width(),
                        display_size.x,
                    ));
                    let response = ui.add(
                        egui::Image::new((texture_id, display_size))
                            .bg_fill(theme().page)
                            .corner_radius(3)
                            .sense(egui::Sense::click()),
                    );
                    let image_rect = response.rect;
                    if response.clicked() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            let click = preview_click_from_pos(
                                pos,
                                image_rect,
                                render_width,
                                render_height,
                                render_page_index,
                            );
                            self.preview_click = click;
                            let mut highlighted_stream_code = false;
                            if let Some(click) = click {
                                self.select_text_hit_for_preview_click(click);
                                self.select_visual_hit_for_preview_click(click);
                                highlighted_stream_code =
                                    self.open_nice_stream_for_preview_selection(click.page_index);
                            }
                            self.selected_tab = if highlighted_stream_code {
                                InspectorTab::Stream
                            } else {
                                InspectorTab::Object
                            };
                        }
                    }
                    let painter = ui.painter_at(image_rect);
                    if let Some(hit) = &selected_visual_hit {
                        if let Some(rect) = visual_hit_bbox_image_rect(
                            hit,
                            image_rect,
                            render_width,
                            render_height,
                            render_zoom,
                            render_rotation,
                        ) {
                            painter.rect_stroke(
                                rect,
                                0.0,
                                egui::Stroke::new(2.0, theme().accent),
                                egui::StrokeKind::Outside,
                            );
                        }
                    }
                    if let Some(hit) = &selected_hit {
                        if let Some(rect) = text_hit_bbox_image_rect(
                            hit,
                            image_rect,
                            render_width,
                            render_height,
                            render_zoom,
                            render_rotation,
                        ) {
                            painter.rect_stroke(
                                rect,
                                0.0,
                                egui::Stroke::new(2.0, theme().operator),
                                egui::StrokeKind::Outside,
                            );
                        }
                    }
                    if let Some(click) =
                        preview_click.filter(|click| click.page_index == render_page_index)
                    {
                        let marker = egui::pos2(
                            image_rect.left() + click.normalized_x * image_rect.width(),
                            image_rect.top() + click.normalized_y * image_rect.height(),
                        );
                        painter.circle_stroke(marker, 5.0, egui::Stroke::new(1.5, theme().accent));
                        painter.line_segment(
                            [
                                marker + egui::vec2(-8.0, 0.0),
                                marker + egui::vec2(8.0, 0.0),
                            ],
                            egui::Stroke::new(1.0, theme().accent),
                        );
                        painter.line_segment(
                            [
                                marker + egui::vec2(0.0, -8.0),
                                marker + egui::vec2(0.0, 8.0),
                            ],
                            egui::Stroke::new(1.0, theme().accent),
                        );
                    }
                });
            });
        self.draw_floating_preview_controls(ui, content_rect);
        true
    }

    pub(crate) fn draw_floating_preview_controls(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        if self.page_count() == 0 {
            return;
        }
        let layout = preview_controls_overlay_layout(content_rect);
        egui::Area::new(egui::Id::new("preview_floating_controls"))
            .order(egui::Order::Middle)
            .fixed_pos(layout.pos)
            .show(ui.ctx(), |ui| {
                if layout.stacked {
                    ui.vertical(|ui| {
                        let mut rerender = false;
                        ui.horizontal(|ui| rerender = self.draw_preview_zoom_group(ui));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.add_space(
                                ((PREVIEW_ZOOM_CONTROL_WIDTH - PREVIEW_PAGER_CONTROL_WIDTH) * 0.5)
                                    .max(0.0),
                            );
                            self.draw_real_preview_pager(ui);
                        });
                        if rerender {
                            self.refresh_real_render();
                        }
                    });
                } else {
                    ui.horizontal(|ui| self.draw_preview_control_groups(ui));
                }
            });
    }

    pub(crate) fn draw_preview_control_groups(&mut self, ui: &mut egui::Ui) {
        let rerender = self.draw_preview_zoom_group(ui);
        ui.add_space(PREVIEW_CONTROL_GAP);
        self.draw_real_preview_pager(ui);
        if rerender {
            self.refresh_real_render();
        }
    }

    pub(crate) fn draw_preview_zoom_group(&mut self, ui: &mut egui::Ui) -> bool {
        let mut rerender = false;
        {
            {
                preview_control_group(ui, PREVIEW_ZOOM_CONTROL_WIDTH, |ui| {
                    let previous_zoom = previous_render_zoom(self.render_zoom);
                    if preview_icon_button(
                        ui,
                        PreviewControlIcon::ZoomOut,
                        previous_zoom.is_some(),
                        "Zoom out",
                    )
                    .clicked()
                    {
                        if let Some(zoom) = previous_zoom {
                            self.render_zoom = zoom;
                            rerender = true;
                        }
                    }
                    ui.add_sized(
                        [64.0, 30.0],
                        egui::Label::new(
                            RichText::new(format!("{:.0}%", self.render_zoom * 100.0))
                                .size(13.0)
                                .color(theme().text),
                        ),
                    );
                    let next_zoom = next_render_zoom(self.render_zoom);
                    if preview_icon_button(
                        ui,
                        PreviewControlIcon::ZoomIn,
                        next_zoom.is_some(),
                        "Zoom in",
                    )
                    .clicked()
                    {
                        if let Some(zoom) = next_zoom {
                            self.render_zoom = zoom;
                            rerender = true;
                        }
                    }
                    preview_control_separator(ui);
                    if preview_icon_button(
                        ui,
                        PreviewControlIcon::RotateRight,
                        true,
                        format!(
                            "Rotate to {} deg",
                            next_render_rotation(self.render_rotation_degrees)
                        ),
                    )
                    .clicked()
                    {
                        self.render_rotation_degrees =
                            next_render_rotation(self.render_rotation_degrees);
                        rerender = true;
                    }
                });
            }
        }
        rerender
    }

    pub(crate) fn draw_real_preview_pager(&mut self, ui: &mut egui::Ui) {
        let page_count = self.page_count();
        if page_count == 0 {
            return;
        }
        preview_control_group(ui, PREVIEW_PAGER_CONTROL_WIDTH, |ui| {
            if preview_icon_button(
                ui,
                PreviewControlIcon::PreviousPage,
                self.render_page_index > 0,
                "Previous page",
            )
            .clicked()
            {
                self.set_render_page_from_pager(self.render_page_index - 1);
            }
            ui.add_sized(
                [64.0, 30.0],
                egui::Label::new(
                    RichText::new(format!(
                        "{}/{}",
                        self.render_page_index + 1,
                        page_count.max(1)
                    ))
                    .size(13.0)
                    .color(theme().text),
                ),
            );
            if preview_icon_button(
                ui,
                PreviewControlIcon::NextPage,
                self.render_page_index + 1 < page_count,
                "Next page",
            )
            .clicked()
            {
                self.set_render_page_from_pager(self.render_page_index + 1);
            }
        });
    }

    pub(crate) fn draw_inspector(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.empty_workspace {
            self.draw_empty_inspector(ui);
            return;
        }

        section_header(ui, "Inspector", Some(&self.selected_object_label()));
        segmented_control(
            ui,
            &mut self.selected_tab,
            &[
                (InspectorTab::Object, "Object", true),
                (InspectorTab::Stream, "Stream", true),
                (InspectorTab::Hex, "Hex", true),
                (InspectorTab::Xref, "Xref", true),
                (InspectorTab::Diagnostics, "Diagnostics", true),
            ],
        );
        ui.add_space(6.0);

        match self.selected_tab {
            InspectorTab::Object => self.draw_object_panel(ui),
            InspectorTab::Stream => self.draw_stream_panel(ui, ctx),
            InspectorTab::Hex => self.draw_hex_panel(ui, ctx),
            InspectorTab::Xref => self.draw_xref_panel(ui),
            InspectorTab::Diagnostics => self.draw_diagnostics_panel(ui, ctx),
        }
    }

    pub(crate) fn export_stream_with_dialog(&mut self, stream: &StreamSummary, mode: StreamMode) {
        let suggested = suggested_export_file_name(stream.object, mode, &stream.filters);
        if let Some(path) = choose_stream_export_path(&suggested) {
            self.start_stream_export(stream.object, mode, path);
        }
    }

    pub(crate) fn draw_image_preview_section(&mut self, ui: &mut egui::Ui, object: ObjectId) {
        self.ensure_image_preview(object);

        section_frame().show(ui, |ui| {
            section_header(ui, "Image Preview", None);
            if self.image_preview_job.is_some() {
                ui.label(RichText::new("Decoding image...").color(theme().muted));
                return;
            }
            if let Some((error_object, err)) = &self.image_preview_error {
                if *error_object == object {
                    ui.colored_label(theme().error_fg, err.clone());
                }
                return;
            }
            if !self
                .image_preview_result
                .as_ref()
                .is_some_and(|(result_object, _)| *result_object == object)
            {
                return;
            }

            let needs_texture = self
                .image_preview_texture
                .as_ref()
                .is_none_or(|(texture_object, _)| *texture_object != object);
            if needs_texture {
                let Some((_, preview)) = &mut self.image_preview_result else {
                    return;
                };
                let Some(color_image) = image_preview_color_image(preview) else {
                    ui.colored_label(theme().error_fg, "image preview has invalid RGBA layout");
                    return;
                };
                // The GPU texture is the only consumer of the pixel buffer;
                // drop the CPU copy (dimensions and diagnostics stay).
                preview.pixels_rgba = Vec::new();
                let texture = ui.ctx().load_texture(
                    "image-object-preview",
                    color_image,
                    egui::TextureOptions::LINEAR,
                );
                self.image_preview_texture = Some((object, texture));
            }
            let Some((_, texture)) = &self.image_preview_texture else {
                return;
            };
            let Some((_, preview)) = &self.image_preview_result else {
                return;
            };

            let texture_size = texture.size_vec2();
            let scale = (ui.available_width() / texture_size.x)
                .min(IMAGE_PREVIEW_MAX_HEIGHT / texture_size.y)
                .clamp(0.01, 1.0);
            ui.add(
                egui::Image::new((texture.id(), texture_size * scale))
                    .bg_fill(theme().page)
                    .corner_radius(3),
            );
            ui.label(
                RichText::new(format!(
                    "{}×{} px{}",
                    preview.width,
                    preview.height,
                    if scale < 1.0 { " (scaled to fit)" } else { "" }
                ))
                .small()
                .color(theme().muted),
            );
            if !preview.diagnostics.is_empty() {
                ui.add_space(4.0);
                for diagnostic in preview.diagnostics.clone() {
                    draw_diagnostic_card(ui, &diagnostic);
                }
            }
        });
    }

    pub(crate) fn draw_selected_xobject_resource_action(
        &mut self,
        ui: &mut egui::Ui,
        stream_object: ObjectId,
        chunks: &[StreamChunk],
    ) {
        let Some(selection_key) = self.real_stream_selected_block.clone() else {
            return;
        };
        let rows = real_stream_nice_render_lines(stream_object, chunks);
        let Some(resource_name) = nice_stream_do_resource_for_selection(&rows, &selection_key)
        else {
            return;
        };

        ui.add_space(4.0);
        egui::Frame::new()
            .fill(theme().code_bg)
            .stroke(egui::Stroke::new(1.0, theme().border))
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("XObject resource")
                            .small()
                            .color(theme().muted),
                    );
                    ui.label(
                        RichText::new(format!("/{resource_name}"))
                            .font(mono_font_id(DENSE_ROW_FONT_SIZE))
                            .color(theme().accent),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Open referenced object").clicked() {
                            self.select_xobject_resource_from_stream_context(
                                stream_object,
                                &resource_name,
                            );
                        }
                    });
                });
            });
        ui.add_space(6.0);
    }

    pub(crate) fn draw_hex_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.tree.is_real() {
            ui.label(RichText::new("Hex view requires a real PDF document").color(theme().muted));
            return;
        }
        if let Some(err) = &self.real_detail_error {
            ui.colored_label(theme().error_fg, err.clone());
            return;
        }
        let Some(detail) = &self.real_detail else {
            ui.label(RichText::new("No object selected").color(theme().muted));
            return;
        };
        let Some(stream) = detail.stream.clone() else {
            ui.label(RichText::new("Selected object has no stream").color(theme().muted));
            return;
        };

        self.ensure_hex_chunk(&stream);

        let mut mode_changed = false;
        let mut requested_offset: Option<u64> = None;
        section_frame().show(ui, |ui| {
            section_header(
                ui,
                "Hex View",
                Some(&format!("{} {} R", stream.object.num, stream.object.gen)),
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("Bytes").small().color(theme().muted));
                mode_changed |= segmented_control(
                    ui,
                    &mut self.hex_mode,
                    &[
                        (StreamMode::Raw, "Raw", true),
                        (StreamMode::Decoded, "Decoded", stream.can_decode),
                    ],
                );
                ui.add_space(10.0);
                ui.label(RichText::new("Offset").small().color(theme().muted));
                let input = ui.add(
                    TextEdit::singleline(&mut self.hex_jump_input)
                        .desired_width(110.0)
                        .hint_text("1024 or 0x400")
                        .font(TextStyle::Monospace),
                );
                let submitted =
                    input.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if ui.button("Go").clicked() || submitted {
                    match parse_hex_jump_offset(&self.hex_jump_input) {
                        Some(offset) => requested_offset = Some(offset),
                        None => {
                            self.hex_error =
                                Some("offset must be decimal or 0x-prefixed hex".to_string());
                        }
                    }
                }
            });
        });

        if mode_changed {
            self.clear_hex_chunk();
        }
        if let Some(offset) = requested_offset.take() {
            self.set_hex_offset(offset);
        }

        if self.hex_job.is_some() && self.hex_chunk.is_none() {
            ui.add_space(8.0);
            ui.label(RichText::new("Loading bytes...").color(theme().muted));
            return;
        }
        if let Some(err) = &self.hex_error {
            ui.add_space(8.0);
            ui.colored_label(theme().error_fg, err.clone());
            return;
        }
        let Some(chunk) = self.hex_chunk.clone() else {
            return;
        };

        let window_end = chunk.offset + chunk.bytes.len() as u64;
        let has_more = chunk.truncated || chunk.total_size.is_some_and(|total| window_end < total);
        let total_label = match chunk.total_size {
            Some(total) => format!("of {total} bytes"),
            None => "(total size unknown)".to_string(),
        };

        ui.add_space(8.0);
        section_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(chunk.offset > 0, egui::Button::new("← Previous"))
                    .clicked()
                {
                    requested_offset =
                        Some(chunk.offset.saturating_sub(HEX_VIEW_WINDOW_BYTES as u64));
                }
                if ui
                    .add_enabled(has_more, egui::Button::new("Next →"))
                    .clicked()
                {
                    requested_offset = Some(window_end);
                }
                ui.label(
                    RichText::new(format!(
                        "{:#x}–{:#x} {total_label}",
                        chunk.offset, window_end
                    ))
                    .small()
                    .color(theme().muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Copy window").clicked() {
                        let dump = hex_dump_bytes(chunk.offset, &chunk.bytes);
                        let escaped = escape_pdf_text(
                            &dump,
                            EgressFormat::Markdown,
                            HEX_VIEW_WINDOW_BYTES * 8,
                        );
                        ctx.copy_text(escaped.text.clone());
                        self.status_log.push(format!(
                            "copied hex window @ {:#x} ({} bytes{})",
                            chunk.offset,
                            chunk.bytes.len(),
                            if escaped.truncated { ", truncated" } else { "" }
                        ));
                        self.copied_excerpt = Some(escaped);
                    }
                });
            });
            ui.add_space(4.0);

            if chunk.bytes.is_empty() {
                ui.label(RichText::new("Stream window is empty").color(theme().muted));
            } else {
                let row_count = chunk.bytes.len().div_ceil(HEX_VIEW_BYTES_PER_ROW);
                let row_height = 15.0;
                ScrollArea::both()
                    .id_salt("hex_view_scroll")
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - 8.0).max(STREAM_VIEW_MIN_HEIGHT))
                    .show_rows(ui, row_height, row_count, |ui, rows| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        for row in rows {
                            let start = row * HEX_VIEW_BYTES_PER_ROW;
                            let end = (start + HEX_VIEW_BYTES_PER_ROW).min(chunk.bytes.len());
                            let line_offset = chunk.offset + start as u64;
                            ui.add(
                                egui::Label::new(
                                    RichText::new(hex_dump_row(
                                        line_offset,
                                        &chunk.bytes[start..end],
                                    ))
                                    .font(mono_font_id(STREAM_VIEW_FONT_SIZE))
                                    .color(theme().text),
                                )
                                .extend()
                                .selectable(true),
                            );
                        }
                    });
            }

            if !chunk.decode_diagnostics.is_empty() {
                ui.add_space(6.0);
                for diagnostic in &chunk.decode_diagnostics {
                    draw_diagnostic_card(ui, diagnostic);
                }
            }
        });

        if let Some(offset) = requested_offset {
            self.set_hex_offset(offset);
        }
    }

    pub(crate) fn draw_xref_panel(&mut self, ui: &mut egui::Ui) {
        self.ensure_xref_slice();

        if let Some(err) = self.xref_error.clone() {
            section_frame().show(ui, |ui| {
                section_header(ui, "Xref Table", None);
                ui.colored_label(theme().error_fg, err);
            });
            return;
        }
        let Some(slice) = self.xref_slice.clone() else {
            return;
        };

        let first = slice.offset;
        let last = first + slice.items.len().saturating_sub(1);
        let detail = if slice.items.is_empty() {
            format!("{} entries", slice.total)
        } else {
            format!("objects {first}–{last} of {}", slice.total)
        };
        let detail = if slice.sections > 1 {
            format!("{detail} · {} sections", slice.sections)
        } else {
            detail
        };

        let mut requested_offset = None;
        let mut follow_object = None;
        section_frame().show(ui, |ui| {
            section_header(ui, "Xref Table", Some(&detail));
            if slice.total > XREF_PAGE_SIZE {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(first > 0, egui::Button::new("← Previous"))
                        .clicked()
                    {
                        requested_offset = Some(first.saturating_sub(XREF_PAGE_SIZE));
                    }
                    let has_more = first + slice.items.len() < slice.total;
                    if ui
                        .add_enabled(has_more, egui::Button::new("Next →"))
                        .clicked()
                    {
                        requested_offset = Some(first + XREF_PAGE_SIZE);
                    }
                    ui.label(
                        RichText::new(format!("{} per page", XREF_PAGE_SIZE))
                            .small()
                            .color(theme().muted),
                    );
                });
                ui.add_space(4.0);
            }

            let row_height = 18.0;
            ScrollArea::vertical()
                .id_salt("xref_table_scroll")
                .auto_shrink([false, false])
                .max_height((ui.available_height() - 8.0).max(STREAM_VIEW_MIN_HEIGHT))
                .show_rows(ui, row_height, slice.items.len(), |ui, rows| {
                    egui::Grid::new("xref_table_grid")
                        .num_columns(4)
                        .spacing([14.0, 3.0])
                        .striped(true)
                        .show(ui, |ui| {
                            dense_label(ui, "object");
                            dense_label(ui, "type");
                            dense_label(ui, "section");
                            dense_label(ui, "location");
                            ui.end_row();
                            for entry in &slice.items[rows] {
                                let object_text =
                                    format!("{} {}", entry.object.num, entry.object.gen);
                                let navigable =
                                    self.tree.is_real() && entry.kind != XrefEntryKind::Free;
                                if navigable {
                                    if ui
                                        .link(dense_monospace_text(object_text))
                                        .on_hover_text("Show this object in the tree")
                                        .clicked()
                                    {
                                        follow_object = Some(entry.object);
                                    }
                                } else {
                                    ui.label(
                                        dense_monospace_text(object_text).color(theme().muted),
                                    );
                                }
                                let kind_color = match entry.kind {
                                    XrefEntryKind::Free => theme().muted,
                                    XrefEntryKind::Normal => theme().text,
                                    XrefEntryKind::Compressed => theme().accent,
                                };
                                ui.label(
                                    dense_monospace_text(entry.kind.as_public_str())
                                        .color(kind_color),
                                );
                                let section_text = xref_entry_section_label(entry, slice.sections);
                                let is_update = entry.section.is_some_and(|section| section > 0);
                                ui.label(dense_monospace_text(section_text).color(if is_update {
                                    theme().operator
                                } else {
                                    theme().muted
                                }))
                                .on_hover_text(
                                    "Which xref section defines this entry: 0 is the original \
                                     document, higher numbers are later incremental updates",
                                );
                                ui.label(dense_monospace_text(xref_entry_location_label(entry)));
                                ui.end_row();
                            }
                        });
                });
        });

        if let Some(offset) = requested_offset {
            self.set_xref_offset(offset);
        }
        if let Some(object) = follow_object {
            self.follow_real_reference(object);
        }
    }

    pub(crate) fn draw_empty_inspector(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "Inspector", Some("No document"));
        section_frame().show(ui, |ui| {
            ui.label(RichText::new("Waiting for a PDF").color(theme().muted));
            ui.add_space(8.0);
            if ui.button("Open PDF...").clicked() {
                self.open_pdf_with_file_dialog();
            }
        });
    }

    pub(crate) fn draw_object_panel(&mut self, ui: &mut egui::Ui) {
        if self.tree.is_real() {
            self.draw_real_object_panel(ui);
            return;
        }

        section_frame().show(ui, |ui| {
            ui.label(
                RichText::new(self.selected_object_label())
                    .monospace()
                    .strong()
                    .color(theme().text),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("Indirect references")
                    .small()
                    .color(theme().muted),
            );
            ui.horizontal_wrapped(|ui| {
                for target in self.tree.reference_targets(self.selected_row) {
                    if ui
                        .link(RichText::new(format!("{target} 0 R")).monospace())
                        .on_hover_text("Resolve reference and push navigation history")
                        .clicked()
                    {
                        self.follow_reference(target);
                    }
                }
            });
        });
        ui.add_space(8.0);
        if let Ok(state) = &self.state {
            if let Some(preview) = &state.panels.detail_preview {
                section_frame().show(ui, |ui| {
                    ui.label(RichText::new("Preview").small().color(theme().muted));
                    ui.add_space(3.0);
                    truncated_monospace(ui, preview);
                });
            }
            if let Some(summary) = &state.panels.summary {
                ui.add_space(8.0);
                section_frame().show(ui, |ui| {
                    ui.label(
                        RichText::new("Document summary")
                            .small()
                            .color(theme().muted),
                    );
                    ui.add_space(3.0);
                    egui::Grid::new("document_summary_grid")
                        .num_columns(2)
                        .spacing([12.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            dense_label(ui, "hash");
                            truncated_monospace(ui, option_text(summary.file_hash.as_deref()));
                            ui.end_row();
                            dense_label(ui, "version");
                            truncated_monospace(ui, option_text(summary.pdf_version.as_deref()));
                            ui.end_row();
                            dense_label(ui, "permissions");
                            truncated_monospace(
                                ui,
                                format!(
                                    "print={} copy={} modify={}",
                                    summary.permissions.print,
                                    summary.permissions.copy,
                                    summary.permissions.modify
                                ),
                            );
                            ui.end_row();
                        });
                });
            }
        } else if let Err(err) = &self.state {
            ui.colored_label(theme().error_fg, err);
        }
    }

    pub(crate) fn draw_real_object_panel(&mut self, ui: &mut egui::Ui) {
        if self.draw_preview_click_panel(ui) {
            return;
        }

        if let Some(err) = &self.real_detail_error {
            ui.colored_label(theme().error_fg, err);
            return;
        }

        let Some(detail) = self.real_detail.clone() else {
            ui.label(RichText::new("No object selected").color(theme().muted));
            return;
        };

        section_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                type_badge(ui, &detail.kind);
                let label_width = if detail.object.is_some() {
                    (ui.available_width() - 88.0).max(24.0)
                } else {
                    ui.available_width().max(24.0)
                };
                truncated_label(
                    ui,
                    RichText::new(&detail.label)
                        .monospace()
                        .strong()
                        .color(theme().text),
                    label_width,
                    Some(&detail.label),
                );
                if let Some(object) = detail.object {
                    ui.label(
                        RichText::new(format!("[{} {} R]", object.num, object.gen))
                            .monospace()
                            .color(theme().accent),
                    );
                }
            });
            ui.add_space(6.0);
            egui::Grid::new("real_object_summary_grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    dense_label(ui, "kind");
                    truncated_monospace(ui, object_kind_label(&detail.kind));
                    ui.end_row();
                    dense_label(ui, "value");
                    truncated_monospace(ui, object_value_preview(&detail.value, &detail.preview));
                    ui.end_row();
                    dense_label(ui, "path");
                    truncated_monospace(ui, node_breadcrumb(&detail.id));
                    ui.end_row();
                });
        });

        if detail.dictionary_entries.is_some() || detail.array_entries.is_some() {
            ui.add_space(8.0);
            if ui
                .button("Expand bounded children")
                .on_hover_text("Append only the loaded child page to the tree")
                .clicked()
            {
                self.expand_selected_real_row();
            }
        }

        let references = detail_reference_targets(&detail);
        if !references.is_empty() {
            ui.add_space(8.0);
            section_frame().show(ui, |ui| {
                ui.label(
                    RichText::new("Indirect references")
                        .small()
                        .color(theme().muted),
                );
                ui.add_space(3.0);
                ui.horizontal_wrapped(|ui| {
                    for object in references {
                        if ui
                            .link(
                                RichText::new(format!("{} {} R", object.num, object.gen))
                                    .monospace(),
                            )
                            .on_hover_text("Resolve reference and push navigation history")
                            .clicked()
                        {
                            self.follow_real_reference(object);
                        }
                    }
                });
            });
        }

        if let Some(entries) = &detail.dictionary_entries {
            ui.add_space(8.0);
            section_frame().show(ui, |ui| {
                section_header(
                    ui,
                    "Dictionary",
                    Some(&child_page_detail(entries.total, entries.items.len())),
                );
                egui::Grid::new("real_dictionary_grid")
                    .num_columns(3)
                    .spacing([12.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for entry in &entries.items {
                            ui.label(dense_monospace_text(format!("/{}", entry.key)));
                            type_badge(ui, &entry.value.kind);
                            truncated_monospace(ui, summary_inline_text(&entry.value));
                            ui.end_row();
                        }
                    });
            });
        }

        if let Some(entries) = &detail.array_entries {
            ui.add_space(8.0);
            section_frame().show(ui, |ui| {
                section_header(
                    ui,
                    "Array",
                    Some(&child_page_detail(entries.total, entries.items.len())),
                );
                egui::Grid::new("real_array_grid")
                    .num_columns(3)
                    .spacing([12.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for (index, entry) in entries.items.iter().enumerate() {
                            ui.label(dense_monospace_text(format!("[{index}]")));
                            type_badge(ui, &entry.kind);
                            truncated_monospace(ui, summary_inline_text(entry));
                            ui.end_row();
                        }
                    });
            });
        }
    }

    pub(crate) fn draw_preview_click_panel(&self, ui: &mut egui::Ui) -> bool {
        let Some(click) = self.preview_click else {
            return false;
        };

        section_frame().show(ui, |ui| {
            ui.label(RichText::new("Preview hit").small().color(theme().muted));
            ui.add_space(3.0);
            egui::Grid::new("preview_click_grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    dense_label(ui, "page");
                    truncated_monospace(ui, format!("{}", click.page_index + 1));
                    ui.end_row();

                    dense_label(ui, "render px");
                    truncated_monospace(
                        ui,
                        format!("{:.1}, {:.1}", click.render_x, click.render_y),
                    );
                    ui.end_row();

                    dense_label(ui, "normalized");
                    truncated_monospace(
                        ui,
                        format!("{:.4}, {:.4}", click.normalized_x, click.normalized_y),
                    );
                    ui.end_row();

                    let text_bbox = self
                        .selected_text_hit
                        .as_ref()
                        .filter(|hit| hit.page_index == click.page_index)
                        .and_then(|hit| hit.bbox.as_ref())
                        .map(|bbox| {
                            format!(
                                "x={:.1} y={:.1} w={:.1} h={:.1}",
                                bbox.x, bbox.y, bbox.width, bbox.height
                            )
                        });
                    if let Some(text_bbox) = text_bbox {
                        dense_label(ui, "text bbox");
                        truncated_monospace(ui, text_bbox);
                        ui.end_row();
                    }

                    let visual_hit = self
                        .selected_visual_hit
                        .as_ref()
                        .filter(|hit| hit.page_index == click.page_index);

                    if let Some(hit) = visual_hit {
                        dense_label(ui, "visual kind");
                        truncated_monospace(
                            ui,
                            format!(
                                "{} #{}{}{}",
                                hit.kind.as_public_str(),
                                hit.element_index,
                                if hit.contains_click {
                                    " contains"
                                } else {
                                    " near"
                                },
                                if hit.untrusted { " untrusted" } else { "" }
                            ),
                        );
                        ui.end_row();

                        dense_label(ui, "visual bbox");
                        truncated_monospace(
                            ui,
                            format!(
                                "x={:.1} y={:.1} w={:.1} h={:.1}",
                                hit.bbox.x, hit.bbox.y, hit.bbox.width, hit.bbox.height
                            ),
                        );
                        ui.end_row();

                        if let Some(object) = visual_object_attribution_text(Some(hit)) {
                            dense_label(ui, "object attribution");
                            truncated_monospace(ui, object);
                            ui.end_row();
                        }
                    }
                });
        });
        ui.add_space(8.0);
        true
    }

    pub(crate) fn draw_stream_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.tree.is_real() {
            self.draw_real_stream_panel(ui, ctx);
            return;
        }

        section_frame().show(ui, |ui| {
            egui::Grid::new("stream_controls_grid")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    dense_label(ui, "Offset");
                    let offset_response = ui.add(
                        egui::DragValue::new(&mut self.stream.offset)
                            .range(0..=STREAM_TOTAL_BYTES.saturating_sub(HEX_WINDOW_BYTES))
                            .speed(64),
                    );
                    if offset_response.changed() {
                        self.stream.sync_hex_window();
                    }
                    ui.end_row();

                    dense_label(ui, "Fallback range");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.stream.selection_offset)
                                .range(0..=STREAM_TOTAL_BYTES.saturating_sub(1))
                                .speed(64),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.stream.selection_len)
                                .range(1..=32 * 1024)
                                .speed(16),
                        );
                    });
                    ui.end_row();
                });
            ui.label(
                RichText::new("Read-only hex window. Drag-select visible text before copying.")
                    .small()
                    .color(theme().muted),
            );
        });
        ui.add_space(8.0);

        self.stream.sync_hex_window();
        let output = TextEdit::multiline(&mut self.stream.hex_text)
            .font(egui::TextStyle::Monospace)
            .desired_rows(18)
            .code_editor()
            .show(ui);

        if let Some(cursor_range) = output.cursor_range.filter(|range| !range.is_empty()) {
            self.stream.selected_hex_text =
                Some(cursor_range.slice_str(&self.stream.hex_text).to_string());
        }
        if output.response.changed() {
            self.stream.reset_hex_window();
            self.status_log
                .push("ignored edit in read-only hex view".to_string());
        }

        if ui.button("Copy escaped excerpt").clicked() {
            let source = self.stream.copy_source_label();
            let escaped = self.stream.escaped_copy_text();
            ctx.copy_text(escaped.text.clone());
            self.status_log.push(format!(
                "copied bounded {source} excerpt{}",
                if escaped.truncated {
                    " (truncated)"
                } else {
                    ""
                }
            ));
            self.copied_excerpt = Some(escaped);
        }

        if let Some(escaped) = &self.copied_excerpt {
            ui.add_space(8.0);
            ui.label(
                RichText::new(if escaped.truncated {
                    "Last copied excerpt (truncated)"
                } else {
                    "Last copied excerpt"
                })
                .small()
                .color(theme().muted),
            );
            let mut copied = escaped.text.clone();
            ui.add(
                TextEdit::multiline(&mut copied)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(5)
                    .interactive(false),
            );
        }
    }

    pub(crate) fn draw_real_stream_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(detail) = &self.real_detail else {
            ui.label(RichText::new("No object selected").color(theme().muted));
            return;
        };
        let Some(stream) = detail.stream.clone() else {
            ui.label(RichText::new("Selected object has no stream").color(theme().muted));
            return;
        };
        let xobject_type_label = self
            .state
            .as_ref()
            .ok()
            .and_then(|state| xobject_subtype(state, detail))
            .map(|subtype| {
                ResolvedXObject {
                    object: stream.object,
                    is_image: stream.image_preview_available,
                    subtype: Some(subtype),
                }
                .type_label()
            });

        section_frame().show(ui, |ui| {
            section_header_with_controls(ui, "Stream Summary", |ui| {
                if self.stream_export_job.is_some() {
                    if ui.button("Cancel export").clicked() {
                        self.cancel_stream_export_job();
                    }
                    ui.label(RichText::new("exporting...").small().color(theme().muted));
                } else {
                    ui.menu_button("Export Stream", |ui| {
                        if ui
                            .add_enabled(stream.can_decode, egui::Button::new("Decoded bytes..."))
                            .clicked()
                        {
                            ui.close();
                            self.export_stream_with_dialog(&stream, StreamMode::Decoded);
                        }
                        if ui.button("Raw bytes...").clicked() {
                            ui.close();
                            self.export_stream_with_dialog(&stream, StreamMode::Raw);
                        }
                    });
                }
            });
            let decoded_size = self
                .real_stream_chunk
                .as_ref()
                .filter(|chunk| chunk.mode == StreamMode::Decoded)
                .and_then(|chunk| chunk.total_size);
            draw_stream_summary_grid(ui, &stream, decoded_size, xobject_type_label.as_deref());
        });

        if stream.image_preview_available {
            ui.add_space(8.0);
            self.draw_image_preview_section(ui, stream.object);
        }

        if self.real_stream_key.is_none()
            && self.real_stream_chunk.is_none()
            && self.real_stream_job.is_none()
        {
            self.real_stream_preset = real_stream_initial_preset(&stream);
            self.apply_real_stream_preset(&stream);
        }

        ui.add_space(8.0);
        let mut request_changed = false;
        let mut force_reload = false;
        section_frame().show(ui, |ui| {
            section_header_with_controls(ui, "Stream View", |ui| {
                if ui.button("Reload").clicked() {
                    force_reload = true;
                }
                ui.add_space(6.0);
                if segmented_control(
                    ui,
                    &mut self.real_stream_preset,
                    &[
                        (RealStreamPreset::Nice, "Formatted", true),
                        (RealStreamPreset::Raw, "Raw", true),
                    ],
                ) {
                    request_changed |= self.apply_real_stream_preset(&stream);
                }
            });
            ui.collapsing("Advanced", |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Mode").small().color(theme().muted));
                    let decoded_enabled = stream.can_decode;
                    request_changed |= segmented_control(
                        ui,
                        &mut self.real_stream_mode,
                        &[
                            (StreamMode::Raw, "Raw", true),
                            (StreamMode::Decoded, "Decoded", decoded_enabled),
                        ],
                    );
                    if !decoded_enabled && self.real_stream_mode == StreamMode::Decoded {
                        self.real_stream_mode = StreamMode::Raw;
                        request_changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Format").small().color(theme().muted));
                    request_changed |= segmented_control(
                        ui,
                        &mut self.real_stream_view_mode,
                        &[
                            (StreamViewMode::Hex, "Hex", true),
                            (StreamViewMode::Text, "Text", true),
                            (StreamViewMode::Bytes, "Bytes", true),
                        ],
                    );
                });
                ui.add_space(4.0);
                egui::Grid::new("real_stream_controls_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        dense_label(ui, "Offset");
                        request_changed |= ui
                            .add(egui::DragValue::new(&mut self.real_stream_offset).speed(64))
                            .changed();
                        ui.end_row();

                        dense_label(ui, "Limit");
                        request_changed |= ui
                            .add(
                                egui::DragValue::new(&mut self.real_stream_limit)
                                    .range(1..=REAL_STREAM_MAX_VIEW_LIMIT_BYTES)
                                    .speed(64),
                            )
                            .changed();
                        ui.end_row();
                    });
            });
            if request_changed {
                self.clear_real_stream_chunk();
            }
            if force_reload {
                self.clear_real_stream_chunk();
                self.refresh_real_stream_chunk(stream.object);
            }
            if self.real_stream_job.is_some() {
                if ui.button("Cancel load").clicked() {
                    self.cancel_real_stream_job();
                }
            } else if self.real_stream_key.is_none() {
                self.refresh_real_stream_chunk(stream.object);
            }
        });

        if self.real_stream_job.is_some() && self.real_stream_chunk.is_none() {
            ui.add_space(8.0);
            ui.label(RichText::new("Loading stream chunk...").color(theme().muted));
            return;
        }

        if let Some(err) = &self.real_stream_error {
            ui.add_space(8.0);
            ui.colored_label(theme().error_fg, err);
            return;
        }

        let Some(chunk) = self.real_stream_chunk.clone() else {
            return;
        };
        let loaded_chunks = if self.real_stream_windows.is_empty() {
            vec![chunk.clone()]
        } else {
            self.real_stream_windows
                .iter()
                .map(|window| window.chunk.clone())
                .collect::<Vec<_>>()
        };
        ui.add_space(8.0);
        section_frame().show(ui, |ui| {
            let visible_text = real_stream_chunks_visible_text(
                &loaded_chunks,
                self.real_stream_view_mode,
                self.real_stream_preset,
            );
            section_header_with_controls(
                ui,
                real_stream_preset_label(self.real_stream_preset),
                |ui| {
                    if ui.button("Copy visible text").clicked() {
                        let escaped = escape_pdf_text(
                            &visible_text,
                            EgressFormat::Markdown,
                            COPY_LIMIT_BYTES,
                        );
                        ctx.copy_text(escaped.text.clone());
                        self.status_log.push(format!(
                            "copied visible {} {} stream text{}",
                            stream_view_mode_label(self.real_stream_view_mode).to_ascii_lowercase(),
                            stream_mode_label(chunk.mode),
                            if escaped.truncated {
                                " (truncated)"
                            } else {
                                ""
                            }
                        ));
                        self.copied_excerpt = Some(escaped);
                    }
                    ui.label(
                        RichText::new(real_stream_loaded_label(&loaded_chunks))
                            .small()
                            .color(theme().muted),
                    );
                },
            );
            if self.real_stream_preset == RealStreamPreset::Nice
                && self.real_stream_view_mode == StreamViewMode::Text
            {
                self.draw_selected_xobject_resource_action(ui, stream.object, &loaded_chunks);
            }
            let view_height = (ui.available_height() - 32.0).max(STREAM_VIEW_MIN_HEIGHT);
            let scroll_output = ScrollArea::both()
                .id_salt((
                    "real_stream_visible_chunk",
                    loaded_chunks
                        .first()
                        .map(|chunk| chunk.offset)
                        .unwrap_or_default(),
                    stream_mode_label(chunk.mode),
                    stream_view_mode_label(self.real_stream_view_mode),
                ))
                .min_scrolled_height(view_height)
                .max_height(view_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.real_stream_preset == RealStreamPreset::Nice
                        && self.real_stream_view_mode == StreamViewMode::Text
                    {
                        self.draw_nice_stream_lines(ui, stream.object, &loaded_chunks);
                    } else {
                        ui.add(
                            egui::Label::new(
                                RichText::new(visible_text.as_str())
                                    .font(mono_font_id(STREAM_VIEW_FONT_SIZE))
                                    .color(theme().text),
                            )
                            .extend()
                            .selectable(true),
                        );
                    }
                });
            let auto_request_offset = {
                let hovered = ui.input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pos| scroll_output.inner_rect.contains(pos))
                });
                let scroll_delta_y = ui.input(|input| input.smooth_scroll_delta.y);
                real_stream_scroll_request(
                    scroll_output.state.offset.y,
                    scroll_output.inner_rect.height(),
                    scroll_output.content_size.y,
                    scroll_delta_y,
                    hovered,
                    &loaded_chunks,
                    self.real_stream_limit,
                )
            };
            if let Some(offset) = auto_request_offset {
                self.real_stream_offset = offset;
                self.refresh_real_stream_chunk(stream.object);
            }
            if self.real_stream_job.is_some() {
                ui.label(
                    RichText::new("loading more...")
                        .small()
                        .color(theme().muted),
                );
            } else if real_stream_chunks_has_more(&loaded_chunks) {
                ui.label(
                    RichText::new("more available; scroll down to load")
                        .small()
                        .color(theme().muted),
                );
            }
        });

        if !chunk.decode_diagnostics.is_empty() {
            ui.add_space(8.0);
            for diagnostic in &chunk.decode_diagnostics {
                draw_diagnostic_card(ui, diagnostic);
            }
        }
    }

    pub(crate) fn draw_nice_stream_lines(
        &mut self,
        ui: &mut egui::Ui,
        object: ObjectId,
        chunks: &[StreamChunk],
    ) {
        let rows = real_stream_nice_render_lines(object, chunks);
        let mut hidden_block_indent = None;
        ui.spacing_mut().item_spacing.y = 1.0;
        for row in &rows {
            let line = &row.line;
            if let Some(indent) = hidden_block_indent {
                if line.indent > indent {
                    continue;
                }
                if line.indent == indent && line.block_close {
                    hidden_block_indent = None;
                    continue;
                }
                hidden_block_indent = None;
            }

            let mut collapsed =
                line.block_open && self.real_stream_collapsed_blocks.contains(&row.line_key);
            let selection_key = row.block_key.as_deref().unwrap_or(row.line_key.as_str());
            let selected_key = self.real_stream_selected_block.as_deref();
            let selected =
                selected_key == Some(selection_key) || selected_key == Some(row.line_key.as_str());
            let row_response = ui.horizontal(|ui| {
                ui.add_space(line.indent as f32 * NICE_STREAM_INDENT_WIDTH);
                if line.block_open {
                    let marker = if collapsed { "+" } else { "-" };
                    if ui
                        .small_button(marker)
                        .on_hover_text("Collapse or expand this content block")
                        .clicked()
                    {
                        collapsed = !collapsed;
                        if collapsed {
                            self.real_stream_collapsed_blocks
                                .insert(row.line_key.clone());
                        } else {
                            self.real_stream_collapsed_blocks.remove(&row.line_key);
                        }
                        self.real_stream_selected_block = Some(row.line_key.clone());
                        self.select_nice_stream_selection_for_preview(object, &row.line_key, &rows);
                    }
                } else {
                    ui.add_space(22.0);
                }

                let color = if line.block_open {
                    theme().accent
                } else if line.block_close {
                    theme().muted
                } else {
                    theme().text
                };
                let response = ui.selectable_label(
                    selected,
                    RichText::new(line.text.as_str())
                        .font(mono_font_id(STREAM_VIEW_FONT_SIZE))
                        .color(color),
                );
                if response.clicked() {
                    self.real_stream_selected_block = Some(selection_key.to_string());
                    self.select_nice_stream_selection_for_preview(object, selection_key, &rows);
                    if line.block_open {
                        collapsed = !collapsed;
                        if collapsed {
                            self.real_stream_collapsed_blocks
                                .insert(row.line_key.clone());
                        } else {
                            self.real_stream_collapsed_blocks.remove(&row.line_key);
                        }
                    }
                }
            });
            if selected && self.scroll_selected_nice_stream_row {
                row_response
                    .response
                    .scroll_to_me(Some(egui::Align::Center));
                self.scroll_selected_nice_stream_row = false;
            }
            draw_nice_stream_guides(
                ui,
                row_response.response.rect,
                &row.guide_blocks,
                self.real_stream_selected_block.as_deref(),
            );
            if line.block_open && collapsed {
                hidden_block_indent = Some(line.indent);
            }
        }
    }

    pub(crate) fn draw_diagnostics_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let model = self.diagnostics_model();
        let all_count = model.all().len();
        let diagnostics = model.filtered(&self.diagnostics_filter());

        section_frame().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Severity").small().color(theme().muted));
                egui::ComboBox::from_id_salt("diagnostic_min_severity")
                    .selected_text(match &self.diagnostic_min_severity {
                        Some(severity) => severity.as_public_str(),
                        None => "all",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.diagnostic_min_severity, None, "all");
                        ui.selectable_value(
                            &mut self.diagnostic_min_severity,
                            Some(DiagnosticSeverity::Info),
                            "info+",
                        );
                        ui.selectable_value(
                            &mut self.diagnostic_min_severity,
                            Some(DiagnosticSeverity::Warning),
                            "warning+",
                        );
                        ui.selectable_value(
                            &mut self.diagnostic_min_severity,
                            Some(DiagnosticSeverity::Error),
                            "error",
                        );
                    });
                ui.label(RichText::new("Code").small().color(theme().muted));
                ui.add(
                    TextEdit::singleline(&mut self.diagnostic_code_filter)
                        .desired_width(150.0)
                        .hint_text("code"),
                );
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("{} shown / {} total", diagnostics.len(), all_count))
                        .small()
                        .color(theme().muted),
                );
                if ui.button("Copy JSON").clicked() {
                    self.copy_diagnostics_json(ctx);
                }
                if ui.button("Copy Markdown").clicked() {
                    self.copy_markdown_report(ctx);
                }
            });
        });

        ui.add_space(8.0);
        if diagnostics.is_empty() {
            ui.label(RichText::new("No diagnostics").color(theme().muted));
        } else {
            for diagnostic in diagnostics {
                draw_diagnostic_card(ui, &diagnostic);
            }
        }
    }
}

fn about_info_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).size(11.5).color(theme().muted));
    ui.label(RichText::new(value).size(13.5).color(theme().text));
    ui.end_row();
}

fn about_link_row(ui: &mut egui::Ui, label: &str, url: &str) {
    ui.label(RichText::new(label).size(11.5).color(theme().muted));
    ui.hyperlink_to(RichText::new(url).size(13.5), url);
    ui.end_row();
}

#[cfg(feature = "real-mupdf")]
fn backend_label(mupdf_version: &str) -> String {
    if mupdf_version == "linked (version unavailable)" {
        "MuPDF (version unavailable)".to_string()
    } else {
        format!("MuPDF {mupdf_version}")
    }
}

#[cfg(not(feature = "real-mupdf"))]
fn backend_label(_mupdf_version: &str) -> String {
    "sample backend".to_string()
}

fn build_profile_label() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

fn build_commit_label() -> &'static str {
    option_env!("PDBG_BUILD_COMMIT").unwrap_or("unknown")
}

fn release_date_label() -> &'static str {
    option_env!("PDBG_RELEASE_DATE").unwrap_or("unknown")
}

fn about_info_text(backend: &str, platform: &str, render_limit: &str) -> String {
    [
        format!("{APP_TITLE} {}", env!("CARGO_PKG_VERSION")),
        format!("Commit: {}", build_commit_label()),
        format!("Release date: {}", release_date_label()),
        format!("Backend: {backend}"),
        format!("OS: {platform}"),
        format!("Build: {}", build_profile_label()),
        format!("Render limit: {render_limit}"),
        format!("GitHub: {APP_GITHUB_URL}"),
    ]
    .join("\n")
}

fn platform_label() -> String {
    format!("{} / {}", std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(feature = "real-mupdf")]
fn mupdf_version_label() -> String {
    read_mupdf_version_from_env().unwrap_or_else(|| "linked (version unavailable)".to_string())
}

#[cfg(not(feature = "real-mupdf"))]
fn mupdf_version_label() -> String {
    "not linked".to_string()
}

#[cfg(feature = "real-mupdf")]
fn read_mupdf_version_from_env() -> Option<String> {
    for path in [
        std::env::var_os("PDBG_MUPDF_SOURCE_DIR").map(PathBuf::from),
        std::env::var_os("PDBG_MUPDF_INCLUDE_DIR").map(PathBuf::from),
    ]
    .into_iter()
    .flatten()
    {
        let version_header = if path.ends_with("include") {
            path.join("mupdf").join("fitz").join("version.h")
        } else {
            path.join("include")
                .join("mupdf")
                .join("fitz")
                .join("version.h")
        };
        if let Ok(header) = fs::read_to_string(version_header) {
            if let Some(version) = parse_mupdf_version_header(&header) {
                return Some(version);
            }
        }
        if let Some(version) = infer_mupdf_version_from_path(&path) {
            return Some(version);
        }
    }
    None
}

#[cfg(feature = "real-mupdf")]
fn parse_mupdf_version_header(header: &str) -> Option<String> {
    header.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("#define FZ_VERSION ")?;
        let version = rest.trim().strip_prefix('"')?.split('"').next()?;
        (!version.trim().is_empty()).then(|| version.to_string())
    })
}

#[cfg(feature = "real-mupdf")]
fn infer_mupdf_version_from_path(path: &Path) -> Option<String> {
    path.ancestors()
        .filter_map(|ancestor| ancestor.file_name()?.to_str())
        .find_map(extract_mupdf_version_from_component)
}

#[cfg(feature = "real-mupdf")]
fn extract_mupdf_version_from_component(component: &str) -> Option<String> {
    let rest = component.split_once("mupdf-")?.1;
    let version = rest
        .split_once("-source")
        .map_or(rest, |(version, _)| version)
        .trim();
    let looks_like_version =
        version.chars().all(|ch| ch.is_ascii_digit() || ch == '.') && version.contains('.');
    looks_like_version.then(|| version.to_string())
}
