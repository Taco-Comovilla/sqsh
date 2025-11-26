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
async fn optimize_image(file_path: String, overwrite: bool) -> Result<OptimizationResult, String> {
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

    // Always use a temporary file for optimization first
    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let temp_dir = std::env::temp_dir();
    let temp_name = format!("{}_{}.{}", file_stem, uuid::Uuid::new_v4(), extension);
    let temp_path = temp_dir.join(temp_name);

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

    let new_size = fs::metadata(&temp_path).map_err(|e| e.to_string())?.len();

    if new_size >= original_size {
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

    // Optimization successful
    let output_path = if overwrite {
        // Overwrite original file
        // Use copy + remove to handle potential cross-device issues gracefully
        fs::copy(&temp_path, path).map_err(|e| e.to_string())?;
        fs::remove_file(&temp_path).map_err(|e| e.to_string())?;
        path.to_string_lossy().to_string()
    } else {
        // Keep temp file
        temp_path.to_string_lossy().to_string()
    };

    Ok(OptimizationResult {
        original_size,
        new_size,
        saved_bytes: original_size - new_size,
        output_path,
        skipped: false,
    })
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
