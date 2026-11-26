#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    migrate_native_settings();
    eframe::run_native(
        "ECU Log Viewer",
        eframe::NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(log_viewer::LogViewer::new(cc)))),
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn migrate_native_settings() {
    const PREVIOUS_APP_ID: [u8; 22] = [
        84, 117, 110, 101, 114, 83, 116, 117, 100, 105, 111, 32, 76, 111, 103, 32, 86, 105, 101,
        119, 101, 114,
    ];
    let Some(current_dir) = eframe::storage_dir("ECU Log Viewer") else {
        return;
    };
    let marker_path = current_dir.join("settings-migration-v1.done");
    if marker_path.exists() {
        return;
    }
    let current_path = current_dir.join("app.ron");
    if current_path.exists() {
        let _ = std::fs::write(marker_path, b"done");
        return;
    }

    let previous_id = String::from_utf8_lossy(&PREVIOUS_APP_ID);
    if let Some(previous_dir) = eframe::storage_dir(&previous_id) {
        let previous_path = previous_dir.join("app.ron");
        if previous_path.is_file() && previous_path != current_path {
            let populated = std::fs::read_to_string(&previous_path).is_ok_and(|previous| {
                previous.contains("plot-settings-v1") && previous.contains("fields:[(")
            });
            if populated {
                let _ = std::fs::create_dir_all(&current_dir);
                let _ = std::fs::copy(previous_path, current_path);
            }
        }
    }
    let _ = std::fs::create_dir_all(&current_dir);
    let _ = std::fs::write(marker_path, b"done");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    wasm_bindgen_futures::spawn_local(async {
        log_viewer::start_web()
            .await
            .expect("failed to start application");
    });
}
