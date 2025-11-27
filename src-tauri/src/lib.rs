use image::codecs::jpeg::JpegEncoder;
use oxipng::{InFile, Options, OutFile};
use std::fs;
use std::io::Write;
use std::path::Path;
use zip::write::FileOptions;

#[derive(serde::Serialize)]
struct OptimizationResult {
    original_size: u64,
    new_size: u64,
    saved_bytes: u64,
    output_path: String,
    skipped: bool,
}

#[tauri::command]
async fn optimize_image(file_path: String, overwrite: bool, convert_to: Option<String>) -> Result<OptimizationResult, String> {
    // Offload the heavy lifting to a blocking thread
    tauri::async_runtime::spawn_blocking(move || {
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
            let img = image::open(path).map_err(|e| e.to_string())?;
            let file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;
            let mut writer = std::io::BufWriter::new(file);

            match target_extension {
                "jpg" => {
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
                "webp" => {
                    img.write_to(&mut writer, image::ImageFormat::WebP)
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
            });
        }

        // Calculate saved bytes (can be negative if size increased during conversion)
        let saved_bytes = if original_size > new_size {
            original_size - new_size
        } else {
            0 // Or we could return 0 if it increased, or handle negative in UI. 
              // For now, let's keep it 0 to avoid confusing the UI if it expects unsigned.
              // Wait, u64 cannot be negative. So if new_size > original_size, we must return 0 or handle it.
              // Let's return 0 for saved_bytes if it increased.
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

        Ok(OptimizationResult {
            original_size,
            new_size,
            saved_bytes,
            output_path,
            skipped: false,
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
async fn save_file(src_path: String, dest_path: String) -> Result<(), String> {
    fs::copy(&src_path, &dest_path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![optimize_image, zip_files, save_file])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
