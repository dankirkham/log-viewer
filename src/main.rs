#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    eframe::run_native(
        "ECU Log Viewer",
        eframe::NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(log_viewer::LogViewer::new(cc)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    wasm_bindgen_futures::spawn_local(async {
        log_viewer::start_web()
            .await
            .expect("failed to start application");
    });
}
