mod setup {
    use std::path::{Path, PathBuf};

    use tauri::{App, Manager};

    use crate::paths::{set_resolved, ResolvedData};
    use crate::vin::APP_DATA_DIR;

    #[cfg(target_os = "android")]
    const BUNDLED_REQUESTS: &str = include_str!("../data/requests.json");
    #[cfg(target_os = "android")]
    const BUNDLED_CODES: &[u8] = include_bytes!("../data/code-descriptions.sqlite");
    #[cfg(target_os = "android")]
    const BUNDLED_PIDS: &[u8] = include_bytes!("../data/model-pids.sqlite");

    pub fn prepare_data(app: &App) -> Result<(), Box<dyn std::error::Error>> {
        let app_data_dir = app.path().app_data_dir()?;
        std::fs::create_dir_all(&app_data_dir)?;
        APP_DATA_DIR.get_or_init(|| app_data_dir.clone());

        #[cfg(not(target_os = "android"))]
        {
            let _ = app_data_dir;
            set_resolved(ResolvedData {
                code_descriptions: PathBuf::from("./data/code-descriptions.sqlite"),
                model_pids: PathBuf::from("./data/model-pids.sqlite"),
                requests: PathBuf::from("./data/requests.json"),
                vpic_bundled: resource_exists(app, "data/vpic.sqlite.xz"),
            });
        }

        #[cfg(target_os = "android")]
        {
            let requests = materialize(
                app,
                "data/requests.json",
                &app_data_dir.join("requests.json"),
                Some(BUNDLED_REQUESTS.as_bytes()),
            )?;
            let code_descriptions = materialize(
                app,
                "data/code-descriptions.sqlite",
                &app_data_dir.join("code-descriptions.sqlite"),
                Some(BUNDLED_CODES),
            )?;
            let model_pids = materialize(
                app,
                "data/model-pids.sqlite",
                &app_data_dir.join("model-pids.sqlite"),
                Some(BUNDLED_PIDS),
            )?;
            set_resolved(ResolvedData {
                code_descriptions,
                model_pids,
                requests,
                vpic_bundled: false,
            });
        }

        Ok(())
    }

    #[cfg(not(target_os = "android"))]
    fn resource_exists(app: &App, relative: &str) -> bool {
        app.path()
            .resolve(relative, tauri::path::BaseDirectory::Resource)
            .map(|path| path.is_file())
            .unwrap_or(false)
            || Path::new(relative).is_file()
    }

    #[cfg(target_os = "android")]
    fn materialize(
        app: &App,
        relative: &str,
        dest: &Path,
        bundled: Option<&[u8]>,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        if dest.is_file() {
            if let Ok(meta) = dest.metadata() {
                if meta.len() > 0 {
                    return Ok(dest.to_path_buf());
                }
            }
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if let Ok(src) = app
            .path()
            .resolve(relative, tauri::path::BaseDirectory::Resource)
        {
            if src.is_file() {
                std::fs::copy(&src, dest)?;
                return Ok(dest.to_path_buf());
            }
        }

        if let Some(bytes) = bundled {
            std::fs::write(dest, bytes)?;
            return Ok(dest.to_path_buf());
        }

        Ok(PathBuf::from(relative))
    }
}

use std::{
    fs::File,
    io::{copy, BufReader, BufWriter, Error},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::sleep,
    time::Duration,
};

use serde::Serialize;
use tauri::{async_runtime::spawn, Emitter, Listener, Manager, WebviewWindow};

use crate::bridge::events::{
    do_send_connection_status, listen_connect_elm, listen_decode_vin, listen_send_ports,
};
use crate::bridge::FrontendNotification;
use crate::stats::{
    critical_frequency_calls, custom_pid_calls, frequent_calls, high_frequency_calls,
    less_frequent_calls, once_calls, oxygen_sensors,
};
use crate::vin::vpic_db_path;
use crate::OBD;

#[derive(Serialize, Clone)]
struct AppInfo {
    platform: &'static str,
    #[serde(rename = "vinDatabaseIncluded")]
    vin_database_included: bool,
}

pub fn track_data(window: &Arc<WebviewWindow>, obd: &Arc<Mutex<OBD>>) {
    critical_frequency_calls(window, obd);
    high_frequency_calls(window, obd);
    frequent_calls(window, obd);
    less_frequent_calls(window, obd);
    oxygen_sensors(window, obd);
    once_calls(window, obd);
    custom_pid_calls(window, obd);
}

pub fn connect_obd(
    window: &WebviewWindow,
    port: String,
    baud_rate: u32,
    protocol: u8,
) -> Option<OBD> {
    let mut obd = OBD::new();

    match obd.connect(&port, baud_rate, protocol) {
        Ok(()) => {
            let band = obd.serial_port_baud_rate().unwrap_or_default();
            let port = obd.serial_port_name().unwrap_or_default();

            do_send_connection_status(
                window,
                &obd,
                format!("Connected to port {port} on {band} baud"),
                true,
            );

            Some(obd)
        }
        Err(error) => {
            let message = match error {
                crate::Error::AndroidUnsupported => "ANDROID_UNSUPPORTED".to_string(),
                other => format!("Failed to connect. Error: {other}"),
            };
            do_send_connection_status(window, &obd, message, false);
            None
        }
    }
}

#[cfg(not(target_os = "android"))]
fn try_load_vpic_database(
    window: &Arc<WebviewWindow>,
    db_path: PathBuf,
    xz_path: PathBuf,
) -> Result<(), Error> {
    const EXPECTED_VPIC_SQLITE_FILE_SIZE_BYTES: u64 = 1453146112;

    if let Ok(vpic_sqlite) = File::open(&db_path) {
        if let Ok(metadata) = vpic_sqlite.metadata() {
            if metadata.len() == EXPECTED_VPIC_SQLITE_FILE_SIZE_BYTES {
                println!("[LOAD_VPIC_DATABASE] `vpic.sqlite` already loaded.");
                return Ok(());
            }
        }
    }

    if !xz_path.is_file() {
        let _ = window.emit(
            "display-notification",
            FrontendNotification {
                title: "VIN",
                description: "VIN_DB_EXCLUDED",
            },
        );
        return Ok(());
    }

    println!(
        "[LOAD_VPIC_DATABASE] Loading `vpic.sqlite` - decompressing data from `vpic.sqlite.xz"
    );
    let _ = window.emit(
        "display-notification",
        FrontendNotification {
            title: "VIN Decoder Database",
            description: "Loading local VIN (VPIC) database (SQLite) for the first and only time - please wait before using the VIN decoder.",
        },
    );

    let input = File::open(xz_path)?;
    let output = File::create(db_path)?;

    let reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);

    let mut decompressor = xz2::read::XzDecoder::new(reader);
    if copy(&mut decompressor, &mut writer).is_ok() {
        println!("[LOAD_VPIC_DATABASE] Successfully decompressed and loaded `vpic.sqlite.xz` into `vpic.sqlite`");
        let _ = window.emit(
            "display-notification",
            FrontendNotification {
                title: "VIN Decoder Database",
                description: "Successfully loaded local VIN (VPIC) database - you may now decode VINs as expected.",
            },
        );
    } else {
        let _ = window.emit(
            "display-notification",
            FrontendNotification {
                title: "VIN Deocder Database",
                description: "Failed loading database for the first time - VIN decoding may not function properly. Try restarting the app or opening a issue on the GitHub (github.com/provrb/obdium/issues/.",
            },
        );
    }

    Ok(())
}

#[tauri::command]
async fn paired_devices() -> Result<Vec<(String, u32)>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        #[cfg(target_os = "android")]
        {
            crate::bluetooth::paired_device_names()
                .map(|names| names.into_iter().map(|name| (name, 0)).collect())
        }
        #[cfg(not(target_os = "android"))]
        {
            Ok(OBD::get_open_serial_ports())
        }
    }).await.map_err(|error| error.to_string())?
}

pub fn start() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![paired_devices])
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            #[cfg(target_os = "android")]
            window.with_webview(|webview| {
                webview.jni_handle().exec(|env, activity, _| {
                    crate::bluetooth::initialize(env, activity);
                });
            })?;
            let frontend_ready = Arc::new(AtomicBool::new(false));

            setup::prepare_data(app)?;

            #[cfg(not(target_os = "android"))]
            let (db_path, xz_path) = {
                let db_path = vpic_db_path().unwrap();
                let xz_path = app
                    .path()
                    .resolve("data/vpic.sqlite.xz", tauri::path::BaseDirectory::Resource)
                    .unwrap_or_else(|_| PathBuf::from("./data/vpic.sqlite.xz"));
                (db_path, xz_path)
            };

            let window_arc = Arc::new(window);
            listen_send_ports(&window_arc);
            listen_connect_elm(&window_arc);
            let frontend_ready_listener = Arc::clone(&frontend_ready);
            window_arc.listen("frontend-loaded", move |_| {
                frontend_ready_listener.store(true, Ordering::SeqCst);
            });

            spawn(async move {

                while !frontend_ready.load(Ordering::SeqCst) {
                    sleep(Duration::from_millis(100));
                }

                let info = AppInfo {
                    platform: if cfg!(target_os = "android") {
                        "android"
                    } else {
                        "desktop"
                    },
                    vin_database_included: if cfg!(target_os = "android") {
                        false
                    } else {
                        crate::paths::vpic_database_bundled()
                    },
                };
                let _ = window_arc.emit("app-info", &info);

                #[cfg(target_os = "android")]
                {
                    let _ = window_arc.emit(
                        "display-notification",
                        FrontendNotification {
                            title: "VIN",
                            description: "VIN_DB_EXCLUDED",
                        },
                    );
                }

                listen_decode_vin(&window_arc);
                crate::bridge::events::listen_track_custom_pid(&window_arc);

                sleep(Duration::from_secs(1));

                #[cfg(not(target_os = "android"))]
                {
                    let _ = try_load_vpic_database(&window_arc, db_path, xz_path);
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
