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
                _ => {
                    if let Some(value) = arg.strip_prefix("--gui-smoke-ms=") {
                        options.gui_smoke_ms = value.parse().ok();
                    } else if let Some(value) = arg.strip_prefix("--pdf=") {
                        options.pdf_path = Some(value.to_string());
                    }
                }
            }
        }
        options
    }
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
    let options = pdbg_app::gui::GuiRunOptions {
        smoke_exit_after: options.gui_smoke_ms.map(std::time::Duration::from_millis),
        pdf_path: options.pdf_path,
        recent_files_path: None,
    };
    if let Err(err) = pdbg_app::gui::run_gui_with_options(options) {
        eprintln!("pdbg-app GUI failed: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "gui"))]
fn run_gui(_options: CliOptions) {
    eprintln!("pdbg-app GUI is behind the optional `gui` feature");
    eprintln!("run: cargo run -p pdbg-app --features gui -- --gui");
    std::process::exit(2);
}
