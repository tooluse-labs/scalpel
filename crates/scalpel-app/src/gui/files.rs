use super::*;

pub(crate) fn open_app_state(
    pdf_path: Option<&str>,
    password: Option<&str>,
) -> Result<AppState, String> {
    if let Some(path) = pdf_path {
        #[cfg(feature = "real-mupdf")]
        {
            return AppState::new_real_path_with_password(path, password)
                .map_err(|err| err.message);
        }
        #[cfg(not(feature = "real-mupdf"))]
        {
            let _ = password;
            let _ = path;
            return Err(
                "`--pdf` requires building scalpel-app with `--features real-mupdf`".to_string(),
            );
        }
    }
    let _ = password;
    AppState::new_headless().map_err(|err| err.message)
}

pub(crate) fn choose_pdf_file() -> Option<String> {
    rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .pick_file()
        .map(|path| path.to_string_lossy().into_owned())
}

pub(crate) fn choose_stream_export_path(suggested_file_name: &str) -> Option<String> {
    rfd::FileDialog::new()
        .set_file_name(suggested_file_name)
        .save_file()
        .map(|path| path.to_string_lossy().into_owned())
}

/// Raw single-filter image streams keep their native container extension so
/// the exported file opens directly in image viewers.
pub(crate) fn suggested_export_file_name(
    object: ObjectId,
    mode: StreamMode,
    filters: &[String],
) -> String {
    let extension = match mode {
        StreamMode::Raw => match filters {
            [single] if single == "DCTDecode" => "jpg",
            [single] if single == "JPXDecode" => "jp2",
            _ => "bin",
        },
        StreamMode::Decoded => "bin",
    };
    format!(
        "object-{}-{}-{}.{extension}",
        object.num,
        object.gen,
        stream_mode_label(mode)
    )
}

pub(crate) fn default_recent_files_path() -> PathBuf {
    if let Some(path) =
        std::env::var_os("SCALPEL_RECENT_FILES_PATH").filter(|path| !path.is_empty())
    {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        return PathBuf::from(path).join("scalpel").join("recent-files.txt");
    }
    if let Some(path) = std::env::var_os("HOME").filter(|path| !path.is_empty()) {
        return PathBuf::from(path)
            .join(".config")
            .join("scalpel")
            .join("recent-files.txt");
    }
    std::env::temp_dir()
        .join("scalpel")
        .join("recent-files.txt")
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct UiSettings {
    pub(crate) dark_mode: bool,
    pub(crate) left_panel_width: Option<f32>,
    pub(crate) right_panel_width: Option<f32>,
    pub(crate) render_zoom: Option<f32>,
}

pub(crate) fn ui_settings_path_for(recent_files_path: &Path) -> PathBuf {
    if let Some(path) = std::env::var_os("SCALPEL_UI_SETTINGS_PATH").filter(|path| !path.is_empty())
    {
        return PathBuf::from(path);
    }
    recent_files_path.with_file_name("ui-settings.txt")
}

pub(crate) fn load_ui_settings_from(path: &Path) -> UiSettings {
    let mut settings = UiSettings::default();
    let Ok(contents) = fs::read_to_string(path) else {
        return settings;
    };
    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match (key.trim(), value.trim()) {
            ("dark_mode", value) => settings.dark_mode = value == "true",
            ("left_panel_width", value) => {
                settings.left_panel_width =
                    parse_panel_width(value, LEFT_PANEL_MIN_WIDTH, LEFT_PANEL_MAX_WIDTH);
            }
            ("right_panel_width", value) => {
                settings.right_panel_width =
                    parse_panel_width(value, RIGHT_PANEL_MIN_WIDTH, RIGHT_PANEL_MAX_WIDTH);
            }
            ("render_zoom", value) => settings.render_zoom = parse_render_zoom(value),
            _ => {}
        }
    }
    settings
}

fn parse_panel_width(value: &str, min: f32, max: f32) -> Option<f32> {
    value
        .parse::<f32>()
        .ok()
        .filter(|width| width.is_finite())
        .map(|width| width.clamp(min, max))
}

fn parse_render_zoom(value: &str) -> Option<f32> {
    let zoom = value.parse::<f32>().ok().filter(|zoom| zoom.is_finite())?;
    RENDER_ZOOM_LEVELS
        .iter()
        .copied()
        .find(|level| (level - zoom).abs() < 0.01)
}

pub(crate) fn save_ui_settings_to(path: &Path, settings: &UiSettings) -> io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }

    let mut out = format!("dark_mode={}\n", settings.dark_mode);
    if let Some(width) = settings.left_panel_width {
        out.push_str(&format!("left_panel_width={width}\n"));
    }
    if let Some(width) = settings.right_panel_width {
        out.push_str(&format!("right_panel_width={width}\n"));
    }
    if let Some(zoom) = settings.render_zoom {
        out.push_str(&format!("render_zoom={zoom}\n"));
    }

    let tmp_path = unique_recent_tmp_path(path);
    fs::write(&tmp_path, out)?;
    match fs::rename(&tmp_path, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(&tmp_path);
            Err(err)
        }
    }
}

pub(crate) fn load_recent_pdf_paths_from(path: &Path) -> Vec<String> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut seen = HashSet::new();
    let mut paths = Vec::new();
    for line in contents.lines() {
        let entry = line.trim();
        if !is_recent_path_entry_safe(entry) {
            continue;
        }
        if seen.insert(entry.to_string()) {
            paths.push(entry.to_string());
        }
        if paths.len() >= RECENT_PDF_MAX_ITEMS {
            break;
        }
    }
    paths
}

pub(crate) fn save_recent_pdf_paths_to(path: &Path, paths: &[String]) -> io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }

    let mut out = String::new();
    for path in paths
        .iter()
        .filter(|path| is_recent_path_entry_safe(path))
        .take(RECENT_PDF_MAX_ITEMS)
    {
        out.push_str(path);
        out.push('\n');
    }

    let tmp_path = unique_recent_tmp_path(path);
    fs::write(&tmp_path, out)?;
    match fs::rename(&tmp_path, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(&tmp_path);
            Err(err)
        }
    }
}

pub(crate) fn unique_recent_tmp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("recent-files");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let tmp_name = format!("{file_name}.{}.{}.tmp", std::process::id(), nonce);
    path.with_file_name(tmp_name)
}

pub(crate) fn record_recent_pdf_path(paths: &mut Vec<String>, path: &str) -> bool {
    let Some(path) = normalize_recent_pdf_path(path) else {
        return false;
    };
    let before = paths.clone();
    paths.retain(|entry| entry != &path);
    paths.insert(0, path);
    paths.truncate(RECENT_PDF_MAX_ITEMS);
    *paths != before
}

pub(crate) fn normalize_recent_pdf_path(path: &str) -> Option<String> {
    let path = path.trim();
    if !is_recent_path_entry_safe(path) {
        return None;
    }
    let path = Path::new(path);
    let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = normalized.to_string_lossy().into_owned();
    is_recent_path_entry_safe(&text).then_some(text)
}

pub(crate) fn is_recent_path_entry_safe(path: &str) -> bool {
    !path.trim().is_empty() && !path.contains('\n') && !path.contains('\r')
}

pub(crate) fn is_pdf_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

pub(crate) fn load_initial_real_detail(
    state: &Result<AppState, String>,
    tree: &TreeModel,
) -> (Option<ObjectDetail>, Option<String>) {
    if !tree.is_real() {
        return (None, None);
    }
    match state {
        Ok(state) => load_initial_real_detail_from_state(state, tree),
        Err(err) => (None, Some(err.clone())),
    }
}

pub(crate) fn load_initial_real_detail_from_state(
    state: &AppState,
    tree: &TreeModel,
) -> (Option<ObjectDetail>, Option<String>) {
    let TreeModel::Real(tree) = tree else {
        return (None, None);
    };
    let Some(summary) = tree.summary(0) else {
        return (None, None);
    };
    match load_object_detail(state, &summary.id) {
        Ok(detail) => (Some(detail), None),
        Err(err) => (None, Some(err)),
    }
}

pub(crate) fn load_initial_real_pages(
    state: &Result<AppState, String>,
    tree: &TreeModel,
) -> (Option<ChildPage<ObjectSummary>>, Option<String>) {
    if !tree.is_real() {
        return (None, None);
    }
    match state {
        Ok(state) => load_initial_real_pages_from_state(state, tree),
        Err(err) => (None, Some(err.clone())),
    }
}

pub(crate) fn load_initial_real_pages_from_state(
    state: &AppState,
    tree: &TreeModel,
) -> (Option<ChildPage<ObjectSummary>>, Option<String>) {
    let TreeModel::Real(tree) = tree else {
        return (None, None);
    };
    let Some(page_root) = tree.page_root_summary() else {
        return (None, None);
    };

    match state.session.run_task(|document| {
        document.children(
            &page_root.id,
            ChildRange {
                offset: 0,
                limit: 64,
            },
            ChildContainer::Array,
        )
    }) {
        Ok(pages) => (Some(pages), None),
        Err(err) => (None, Some(err.message)),
    }
}

pub(crate) fn load_object_detail(state: &AppState, id: &NodeId) -> Result<ObjectDetail, String> {
    state
        .session
        .run_task(|document| {
            document.object_detail(
                id,
                ChildRange {
                    offset: 0,
                    limit: 32,
                },
            )
        })
        .map_err(|err| err.message)
}

pub(crate) fn initial_status_log(
    state: &Result<AppState, String>,
    tree: &TreeModel,
    pdf_path: Option<&str>,
) -> Vec<String> {
    match (state, tree, pdf_path) {
        (Ok(_), TreeModel::Real(tree), Some(path)) => vec![
            format!("real MuPDF opened {}", display_file_chip_label(path)),
            format!("loaded bounded root page: {}", tree.row_count_label()),
            "real stream bytes available as bounded raw/decoded chunks".to_string(),
        ],
        (Err(err), _, Some(path)) => vec![
            format!("failed to open {}", display_file_chip_label(path)),
            err.clone(),
        ],
        _ => vec![
            "fake shim opened fake.pdf".to_string(),
            "virtual object tree uses generated rows".to_string(),
            "large stream pane uses generated bytes".to_string(),
        ],
    }
}

pub(crate) fn open_pdf_worker_result(
    path: String,
    password: Option<String>,
) -> Result<OpenPdfJobResult, String> {
    let state = open_app_state(Some(&path), password.as_deref())?;
    if state
        .panels
        .summary
        .as_ref()
        .is_some_and(|summary| summary.needs_password)
    {
        return Ok(OpenPdfJobResult::NeedsPassword);
    }
    Ok(OpenPdfJobResult::Opened(Box::new(build_opened_pdf_model(
        state, &path,
    ))))
}

pub(crate) fn build_opened_pdf_model(state: AppState, path: &str) -> OpenedPdfModel {
    let tree = TreeModel::from_app_state(&state, true);
    let (real_detail, real_detail_error) = load_initial_real_detail_from_state(&state, &tree);
    let (real_pages, real_pages_error) = load_initial_real_pages_from_state(&state, &tree);
    let mut status_log = opened_pdf_status_log(&tree, path);
    if let Some(pages) = &real_pages {
        status_log.push(format!(
            "loaded page list {}",
            child_page_detail(pages.total, pages.items.len())
        ));
    } else if let Some(err) = &real_pages_error {
        status_log.push(format!("page list load failed: {err}"));
    }

    OpenedPdfModel {
        state,
        tree,
        real_detail,
        real_detail_error,
        real_pages,
        real_pages_error,
        status_log,
    }
}

pub(crate) fn opened_pdf_status_log(tree: &TreeModel, path: &str) -> Vec<String> {
    match tree {
        TreeModel::Real(tree) => vec![
            format!("real MuPDF opened {}", display_file_chip_label(path)),
            format!("loaded bounded root page: {}", tree.row_count_label()),
            "real stream bytes available as bounded raw/decoded chunks".to_string(),
        ],
        TreeModel::Virtual(_) => vec![
            format!("opened {}", display_file_chip_label(path)),
            "virtual object tree uses generated rows".to_string(),
            "large stream pane uses generated bytes".to_string(),
        ],
    }
}

pub(crate) fn empty_status_log() -> Vec<String> {
    vec![
        "No PDF open".to_string(),
        "Open PDF or choose a recent file".to_string(),
    ]
}
