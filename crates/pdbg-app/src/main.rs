fn main() {
    if std::env::args().any(|arg| arg == "--gui") {
        run_gui();
        return;
    }

    run_headless();
}

fn run_headless() {
    match pdbg_app::AppState::new_headless() {
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

#[cfg(feature = "gui")]
fn run_gui() {
    let options = pdbg_app::gui::GuiRunOptions {
        smoke_exit_after: gui_smoke_ms().map(std::time::Duration::from_millis),
    };
    if let Err(err) = pdbg_app::gui::run_gui_with_options(options) {
        eprintln!("pdbg-app GUI failed: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "gui"))]
fn run_gui() {
    eprintln!("pdbg-app GUI is behind the optional `gui` feature");
    eprintln!("run: cargo run -p pdbg-app --features gui -- --gui");
    std::process::exit(2);
}

#[cfg(feature = "gui")]
fn gui_smoke_ms() -> Option<u64> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--gui-smoke-ms" {
            return args.next().and_then(|value| value.parse().ok());
        }
        if let Some(value) = arg.strip_prefix("--gui-smoke-ms=") {
            return value.parse().ok();
        }
    }
    None
}
