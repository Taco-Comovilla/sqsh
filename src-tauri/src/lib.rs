use image::codecs::jpeg::JpegEncoder;
use oxipng::{InFile, Options, OutFile};
use std::fs;
use std::io::Write;
use std::path::Path;
use zip::write::FileOptions;
use walkdir::WalkDir;
use tauri::Manager;

#[derive(serde::Serialize)]
struct OptimizationResult {
    original_size: u64,
    new_size: u64,
    saved_bytes: u64,
    output_path: String,
    skipped: bool,
    duration_ms: u64,
}

#[tauri::command]
async fn optimize_image(file_path: String, overwrite: bool, convert_to: Option<String>) -> Result<OptimizationResult, String> {
    // Offload the heavy lifting to a blocking thread
    tauri::async_runtime::spawn_blocking(move || {
        let start_time = std::time::Instant::now();
        let path = Path::new(&file_path);
        if !path.exists() {
            return Err("File not found".to_string());
        }

        let original_size = fs::metadata(path).map_err(|e| e.to_string())?.len();
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Determine target extension
        let target_extension = if let Some(ref format) = convert_to {
            match format.as_str() {
                "jpg" | "jpeg" => "jpg",
                "webp" => "webp",
                "png" => "png",
                _ => return Err("Unsupported conversion format".to_string()),
            }
        } else {
            extension.as_str()
        };

        // Always use a temporary file for optimization first
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
        let temp_dir = std::env::temp_dir();
        let temp_name = format!("{}_{}.{}", file_stem, uuid::Uuid::new_v4(), target_extension);
        let temp_path = temp_dir.join(temp_name);

        if let Some(ref _format) = convert_to {
            // Conversion logic
            // Use Reader to guess format from content, not just extension
            let img = image::ImageReader::open(path)
                .map_err(|e| e.to_string())?
                .with_guessed_format()
                .map_err(|e| e.to_string())?
                .decode()
                .map_err(|e| e.to_string())?;

            let file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;
            let mut writer = std::io::BufWriter::new(file);

            match target_extension {
                "jpg" => {
                    // JPEG does not support transparency (RGBA). Convert to RGB8.
                    // This drops the alpha channel. For better results, we could blend with a background color,
                    // but to_rgb8() is a standard "flatten" that works for now.
                    let rgb_img = img.to_rgb8();
                    let mut encoder = JpegEncoder::new_with_quality(&mut writer, 80);
                    encoder
                        .encode(
                            &rgb_img,
                            rgb_img.width(),
                            rgb_img.height(),
                            image::ColorType::Rgb8.into(),
                        )
                        .map_err(|e| e.to_string())?;
                }
                "webp" => {
                    img.write_to(&mut writer, image::ImageFormat::WebP)
                        .map_err(|e| e.to_string())?;
                }
                "png" => {
                    img.write_to(&mut writer, image::ImageFormat::Png)
                        .map_err(|e| e.to_string())?;
                }
                _ => return Err("Unsupported conversion format".to_string()),
            }
        } else {
            // Optimization logic (same format)
            match extension.as_str() {
                "png" => {
                    let options = Options::from_preset(2); // Default optimization level
                    let input = InFile::Path(path.to_path_buf());
                    let output = OutFile::Path {
                        path: Some(temp_path.clone()),
                        preserve_attrs: false,
                    };

                    oxipng::optimize(&input, &output, &options).map_err(|e| e.to_string())?;
                }
                "jpg" | "jpeg" => {
                    let img = image::open(path).map_err(|e| e.to_string())?;
                    let file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;
                    let mut writer = std::io::BufWriter::new(file);

                    let mut encoder = JpegEncoder::new_with_quality(&mut writer, 80);
                    encoder
                        .encode(
                            img.as_bytes(),
                            img.width(),
                            img.height(),
                            img.color().into(),
                        )
                        .map_err(|e| e.to_string())?;
                }
                "webp" | "tiff" | "tif" | "bmp" | "gif" | "ico" | "tga" | "dds" | "pnm" | "qoi" | "hdr" | "exr" | "ff" => {
                    return Err("Skipped: Enable auto-convert".to_string());
                }
                _ => return Err("Unsupported file format".to_string()),
            }
        }

        let new_size = fs::metadata(&temp_path).map_err(|e| e.to_string())?.len();

        // Only check for size increase if we are NOT converting OR if we are converting to the SAME format.
        // If converting to a DIFFERENT format, we accept the result regardless of size.
        let is_same_format = extension == target_extension;
        
        if new_size >= original_size && (convert_to.is_none() || is_same_format) {
            // Optimization failed to reduce size, discard result
            fs::remove_file(&temp_path).map_err(|e| e.to_string())?;
            return Ok(OptimizationResult {
                original_size,
                new_size: original_size,
                saved_bytes: 0,
                output_path: file_path, // Return original path
                skipped: true,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
        }

        // Calculate saved bytes (can be negative if size increased during conversion)
        let saved_bytes = if original_size > new_size {
            original_size - new_size
        } else {
            0 
        };

        // Optimization successful
        let output_path = if overwrite {
            if convert_to.is_none() {
                // Direct overwrite of source file
                fs::copy(&temp_path, path).map_err(|e| e.to_string())?;
                fs::remove_file(&temp_path).map_err(|e| e.to_string())?;
                path.to_string_lossy().to_string()
            } else {
                // Conversion with overwrite enabled = Save to source dir, but handle conflicts
                // We do NOT delete the original source file as it has a different extension.
                
                let parent = path.parent().unwrap_or(Path::new("."));
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
                let mut target_name = format!("{}.{}", stem, target_extension);
                let mut target_path = parent.join(&target_name);
                
                // Conflict resolution: append (n) if file exists
                // BUT skip conflict resolution if target_path IS the source path (we are overwriting ourselves)
                let mut counter = 1;
                while target_path.exists() && target_path != path {
                    target_name = format!("{} ({}).{}", stem, counter, target_extension);
                    target_path = parent.join(&target_name);
                    counter += 1;
                }
                
                fs::copy(&temp_path, &target_path).map_err(|e| e.to_string())?;
                fs::remove_file(&temp_path).map_err(|e| e.to_string())?;
                target_path.to_string_lossy().to_string()
            }
        } else {
            // Keep temp file
            temp_path.to_string_lossy().to_string()
        };

        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(OptimizationResult {
            original_size,
            new_size,
            saved_bytes,
            output_path,
            skipped: false,
            duration_ms,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn zip_files(files: Vec<(String, String)>, output_path: String) -> Result<String, String> {
    let path = Path::new(&output_path);
    let file = fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);

    let options = FileOptions::<()>::default().compression_method(zip::CompressionMethod::Stored);
    let mut used_names = std::collections::HashSet::new();

    for (fs_path, desired_name) in files {
        let path = Path::new(&fs_path);
        
        // Handle conflicts
        let mut name_in_zip = desired_name.clone();
        let mut counter = 1;
        
        while used_names.contains(&name_in_zip) {
            let path_obj = Path::new(&desired_name);
            let stem = path_obj.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
            let ext = path_obj.extension().and_then(|s| s.to_str()).unwrap_or("");
            
            name_in_zip = if ext.is_empty() {
                format!("{} ({})", stem, counter)
            } else {
                format!("{} ({}).{}", stem, counter, ext)
            };
            counter += 1;
        }
        
        used_names.insert(name_in_zip.clone());
        
        zip.start_file(name_in_zip, options).map_err(|e| e.to_string())?;
        let content = fs::read(path).map_err(|e| e.to_string())?;
        zip.write_all(&content).map_err(|e| e.to_string())?;
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(output_path)
}

#[tauri::command]
async fn scan_directory(paths: Vec<String>) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    let supported_extensions = [
        "png", "jpg", "jpeg", "webp", "tiff", "tif", "bmp", "gif", "ico", "tga", "dds", "pnm",
        "qoi", "hdr", "exr", "ff",
    ];

    for path_str in paths {
        let path = Path::new(&path_str);
        if path.is_file() {
             if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if supported_extensions.contains(&ext.to_lowercase().as_str()) {
                    files.push(path_str);
                }
            }
        } else if path.is_dir() {
            for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                let entry_path = entry.path();
                if entry_path.is_file() {
                    if let Some(ext) = entry_path.extension().and_then(|e| e.to_str()) {
                        if supported_extensions.contains(&ext.to_lowercase().as_str()) {
                            files.push(entry_path.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }
    Ok(files)
}

#[tauri::command]
async fn save_file(src_path: String, dest_path: String) -> Result<(), String> {
    fs::copy(&src_path, &dest_path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_config(state: tauri::State<'_, std::sync::Mutex<AppConfig>>) -> Result<AppConfig, String> {
    Ok(state.lock().unwrap().clone())
}

#[tauri::command]
async fn update_settings(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, std::sync::Mutex<AppConfig>>,
    dark_mode: Option<bool>,
    overwrite: Option<bool>,
    convert_enabled: Option<bool>,
    convert_format: Option<String>,
) -> Result<(), String> {
    let mut config = state.lock().unwrap();
    if let Some(v) = dark_mode { config.dark_mode = v; }
    if let Some(v) = overwrite { config.overwrite = v; }
    if let Some(v) = convert_enabled { config.convert_enabled = v; }
    if let Some(v) = convert_format { config.convert_format = v; }
    
    save_config(&app_handle, &config);
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct AppConfig {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    #[serde(default = "default_dark_mode")]
    dark_mode: bool,
    #[serde(default = "default_overwrite")]
    overwrite: bool,
    #[serde(default = "default_convert_enabled")]
    convert_enabled: bool,
    #[serde(default = "default_convert_format")]
    convert_format: String,
}

fn default_dark_mode() -> bool { true }
fn default_overwrite() -> bool { true }
fn default_convert_enabled() -> bool { false }
fn default_convert_format() -> String { "jpg".to_string() }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
            dark_mode: default_dark_mode(),
            overwrite: default_overwrite(),
            convert_enabled: default_convert_enabled(),
            convert_format: default_convert_format(),
        }
    }
}

fn save_config(app_handle: &tauri::AppHandle, state: &AppConfig) {
    if let Ok(config_dir) = app_handle.path().config_dir() {
        let app_dir = config_dir.join("sqsh");
        if !app_dir.exists() {
            let _ = std::fs::create_dir_all(&app_dir);
        }
        let path = app_dir.join("sqsh.toml");
        let _ = std::fs::write(path, toml::to_string(state).unwrap_or_default());
    }
}

fn load_config(app_handle: &tauri::AppHandle) -> Option<AppConfig> {
    let config_dir = app_handle.path().config_dir().ok()?;
    let app_dir = config_dir.join("sqsh");
    let path = app_dir.join("sqsh.toml");
    if path.exists() {
        let content = std::fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    } else {
        None
    }
}

const MIN_WINDOW_WIDTH: u32 = 400;
const MIN_WINDOW_HEIGHT: u32 = 800;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            use tauri::Manager;
            let window = app.get_webview_window("main").unwrap();
            let app_handle = app.handle().clone();

            // Enforce minimum size
            let _ = window.set_min_size(Some(tauri::Size::Physical(tauri::PhysicalSize {
                width: MIN_WINDOW_WIDTH,
                height: MIN_WINDOW_HEIGHT,
            })));

            // Load and apply state
            let config = load_config(&app_handle).unwrap_or_default();
            
            // Manage state
            app.manage(std::sync::Mutex::new(config.clone()));

            let mut state = config;

                // 1. Enforce min size
                if state.width < MIN_WINDOW_WIDTH { state.width = MIN_WINDOW_WIDTH; }
                if state.height < MIN_WINDOW_HEIGHT { state.height = MIN_WINDOW_HEIGHT; }

                // 2. Validate on-screen
                if let Ok(available_monitors) = window.available_monitors() {
                    if !available_monitors.is_empty() {
                        let mut best_monitor = &available_monitors[0];


                        // Find monitor with most overlap or closest
                        // For simplicity, let's just find the monitor that contains the top-left corner
                        // OR just clamp to the primary/first if completely off.
                        
                        // Let's try to clamp to the monitor that the window is *mostly* on.
                        // But since we are restoring, we just check if the saved rect is valid in ANY monitor.
                        
                        let mut is_visible = false;
                        for monitor in &available_monitors {
                            let m_pos = monitor.position();
                            let m_size = monitor.size();
                            
                            // Check if top-left is inside
                            if state.x >= m_pos.x && state.x < m_pos.x + m_size.width as i32 &&
                               state.y >= m_pos.y && state.y < m_pos.y + m_size.height as i32 {
                                best_monitor = monitor;
                                is_visible = true;
                                break;
                            }
                        }

                        if !is_visible {
                            // If top-left is not visible, try to find a monitor where the window *could* fit
                            // Default to the first monitor (usually primary)
                            best_monitor = &available_monitors[0];
                        }

                        // Clamp to best_monitor
                        let m_pos = best_monitor.position();
                        let m_size = best_monitor.size();
                        // Wait, Tauri sizes are usually logical or physical depending on API.
                        // set_size uses LogicalSize by default or PhysicalSize?
                        // window.set_size(Size::Physical(...))
                        // The state we saved... we should probably save Physical to be safe, or Logical.
                        // Let's assume we save/load Physical for consistency with monitor APIs which return Physical.
                        
                        // Ensure width/height fits in monitor
                        if state.width > m_size.width { state.width = m_size.width; }
                        if state.height > m_size.height { state.height = m_size.height; }

                        // Clamp X
                        if state.x < m_pos.x { state.x = m_pos.x; }
                        if state.x + state.width as i32 > m_pos.x + m_size.width as i32 {
                            state.x = m_pos.x + m_size.width as i32 - state.width as i32;
                        }

                        // Clamp Y
                        if state.y < m_pos.y { state.y = m_pos.y; }
                        if state.y + state.height as i32 > m_pos.y + m_size.height as i32 {
                            state.y = m_pos.y + m_size.height as i32 - state.height as i32;
                        }
                    }
                }

                // Apply state
                let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
                    width: state.width,
                    height: state.height,
                }));
                let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                    x: state.x,
                    y: state.y,
                }));

            // Setup listeners to save state
            let app_handle = app.handle().clone();
            let window_clone = window.clone();
            
            // Use a mutex to debounce/throttle saving
            use std::sync::{Arc, Mutex};
            use std::time::{Duration, Instant};
            
            let last_save = Arc::new(Mutex::new(Instant::now()));
            
            window.on_window_event(move |event| {
                match event {
                    tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                        let mut last = last_save.lock().unwrap();
                        if last.elapsed() > Duration::from_millis(500) {
                            *last = Instant::now();
                            
                            // Get current state
                            if let (Ok(pos), Ok(size)) = (window_clone.outer_position(), window_clone.outer_size()) {
                                let app_state: tauri::State<std::sync::Mutex<AppConfig>> = app_handle.state();
                                let mut state = app_state.lock().unwrap();
                                state.x = pos.x;
                                state.y = pos.y;
                                state.width = size.width;
                                state.height = size.height;
                                save_config(&app_handle, &state);
                            }
                        }
                    }
                    tauri::WindowEvent::CloseRequested { .. } => {
                        // Always save on close
                         if let (Ok(pos), Ok(size)) = (window_clone.outer_position(), window_clone.outer_size()) {
                            let app_state: tauri::State<std::sync::Mutex<AppConfig>> = app_handle.state();
                            let mut state = app_state.lock().unwrap();
                            state.x = pos.x;
                            state.y = pos.y;
                            state.width = size.width;
                            state.height = size.height;
                            save_config(&app_handle, &state);
                        }
                    }
                    _ => {}
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![optimize_image, zip_files, save_file, get_config, update_settings, scan_directory])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
