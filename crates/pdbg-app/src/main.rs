fn main() {
    let options = CliOptions::parse();
    if options.gui {
        run_gui(options);
        return;
    }

    run_headless(options);
}

#[derive(Clone, Debug, Default)]
struct CliOptions {
    gui: bool,
    gui_smoke_ms: Option<u64>,
    pdf_path: Option<String>,
    render_max_dimension: Option<u32>,
}

impl CliOptions {
    fn parse() -> Self {
        let mut options = Self::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--gui" => options.gui = true,
                "--gui-smoke-ms" => {
                    options.gui_smoke_ms = args.next().and_then(|value| value.parse().ok());
                }
                "--pdf" => options.pdf_path = args.next(),
                "--render-max-dimension" | "--max-render-dimension" => {
                    options.render_max_dimension = args
                        .next()
                        .and_then(|value| parse_render_max_dimension(&value));
                }
                _ => {
                    if let Some(value) = arg.strip_prefix("--gui-smoke-ms=") {
                        options.gui_smoke_ms = value.parse().ok();
                    } else if let Some(value) = arg.strip_prefix("--pdf=") {
                        options.pdf_path = Some(value.to_string());
                    } else if let Some(value) = arg
                        .strip_prefix("--render-max-dimension=")
                        .or_else(|| arg.strip_prefix("--max-render-dimension="))
                    {
                        options.render_max_dimension = parse_render_max_dimension(value);
                    }
                }
            }
        }
        options
    }
}

fn parse_render_max_dimension(value: &str) -> Option<u32> {
    value.parse::<u32>().ok().filter(|dimension| *dimension > 0)
}

fn run_headless(options: CliOptions) {
    match open_app_state(options.pdf_path.as_deref()) {
        Ok(state) => {
            let file = state
                .panels
                .summary
                .as_ref()
                .map(|summary| summary.file_path.as_str())
                .unwrap_or("<none>");
            println!("pdbg-app headless smoke: {file}");
        }
        Err(err) => {
            eprintln!("pdbg-app failed: {err:?}");
            std::process::exit(1);
        }
    }
}

fn open_app_state(pdf_path: Option<&str>) -> Result<pdbg_app::AppState, String> {
    if let Some(path) = pdf_path {
        #[cfg(feature = "real-mupdf")]
        {
            return pdbg_app::AppState::new_real_path(path).map_err(|err| err.message);
        }
        #[cfg(not(feature = "real-mupdf"))]
        {
            let _ = path;
            return Err(
                "`--pdf` requires building pdbg-app with `--features real-mupdf`".to_string(),
            );
        }
    }
    pdbg_app::AppState::new_headless().map_err(|err| err.message)
}

#[cfg(feature = "gui")]
fn run_gui(options: CliOptions) {
    let start_empty_when_no_pdf = options.pdf_path.is_none();
    let options = pdbg_app::gui::GuiRunOptions {
        smoke_exit_after: options.gui_smoke_ms.map(std::time::Duration::from_millis),
        pdf_path: options.pdf_path,
        recent_files_path: None,
        start_empty_when_no_pdf,
        render_max_dimension: options.render_max_dimension,
    };
    if let Err(err) = pdbg_app::gui::run_gui_with_options(options) {
        eprintln!("pdbg-app GUI failed: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "gui"))]
fn run_gui(_options: CliOptions) {
    eprintln!("pdbg-app GUI is behind the optional `gui` feature");
    eprintln!("run: cargo run -p pdbg-app --features gui -- --gui [--render-max-dimension 8192]");
    std::process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::parse_render_max_dimension;

    #[test]
    fn parse_render_max_dimension_accepts_positive_pixels() {
        assert_eq!(parse_render_max_dimension("8192"), Some(8192));
        assert_eq!(parse_render_max_dimension("0"), None);
        assert_eq!(parse_render_max_dimension("bad"), None);
    }
}
