fn main() {
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
