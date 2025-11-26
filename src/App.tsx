import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import "./App.css";

interface OptimizationResult {
  original_size: number;
  new_size: number;
  saved_bytes: number;
  output_path: string;
}

interface ProcessedFile {
  path: string;
  status: "pending" | "optimizing" | "done" | "error";
  result?: OptimizationResult;
  error?: string;
}

function App() {
  const [files, setFiles] = useState<ProcessedFile[]>([]);
  const [overwrite, setOverwrite] = useState(true);

  useEffect(() => {
    const unlisten = listen("tauri://drag-drop", (event) => {
      const payload = event.payload as { paths: string[] };
      if (payload.paths && payload.paths.length > 0) {
        handleFiles(payload.paths);
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [overwrite]); // Re-bind listener if overwrite changes (though not strictly necessary for the listener itself, good for closure capture if we used it there)

  const handleFiles = async (paths: string[]) => {
    // Reset files list for new batch if not overwriting to avoid confusion, 
    // or append? Let's append but clear if it was a previous batch. 
    // Actually, for simplicity, let's just append.
    
    const newFiles = paths.map((path) => ({
      path,
      status: "pending" as const,
    }));
    setFiles((prev) => [...prev, ...newFiles]);

    const optimizedResults: string[] = [];

    for (const file of newFiles) {
      setFiles((prev) =>
        prev.map((f) =>
          f.path === file.path ? { ...f, status: "optimizing" } : f
        )
      );

      try {
        const result = await invoke<OptimizationResult>("optimize_image", {
          filePath: file.path,
          overwrite: overwrite,
        });
        
        optimizedResults.push(result.output_path);

        setFiles((prev) =>
          prev.map((f) =>
            f.path === file.path ? { ...f, status: "done", result } : f
          )
        );
      } catch (e) {
        setFiles((prev) =>
          prev.map((f) =>
            f.path === file.path
              ? { ...f, status: "error", error: String(e) }
              : f
          )
        );
      }
    }

    // Handle non-overwrite logic (Save Dialogs)
    if (!overwrite && optimizedResults.length > 0) {
      if (optimizedResults.length === 1) {
        // Single file: Prompt to save
        const originalPath = paths[0];
        // Guess a name? original_optimized.ext
        // But the user wants a dialog.
        try {
          const savePath = await save({
            defaultPath: originalPath, // Suggest original name, user can change
            filters: [{
              name: 'Image',
              extensions: ['png', 'jpg', 'jpeg']
            }]
          });

          if (savePath) {
             await invoke("save_file", { srcPath: optimizedResults[0], destPath: savePath });
          }
        } catch (e) {
          console.error("Failed to save file:", e);
        }
      } else {
        // Multiple files: Zip them
        try {
          // 1. Create zip in temp
          // We need a temp path for the zip. 
          // Actually, we can just ask where to save the zip first, then stream to it?
          // Or create temp zip then move.
          // Let's ask for save location first, then tell Rust to zip there.
          const savePath = await save({
            defaultPath: 'sqsh.zip',
            filters: [{
              name: 'Zip Archive',
              extensions: ['zip']
            }]
          });

          if (savePath) {
            // Prepare files for zipping: [fs_path, desired_name]
            // We need to find the original filename for each optimized result.
            // optimizedResults contains the paths to the temp files.
            // We need to map them back to the original filenames.
            // Since we pushed to optimizedResults in the same order as newFiles, we can use index.
            
            const filesToZip = optimizedResults.map((path, index) => {
              const originalPath = paths[index];
              // Extract filename from original path
              // Simple split for now, assuming standard separators
              const name = originalPath.split(/[\\/]/).pop() || "image";
              return [path, name];
            });

            await invoke("zip_files", {
              files: filesToZip,
              outputPath: savePath
            });
          }
        } catch (e) {
          console.error("Failed to zip/save files:", e);
        }
      }
    }
  };

  return (
    <main className="container mx-auto p-8 min-h-screen bg-background text-foreground">
      <h1 className="text-4xl font-bold mb-8 text-primary text-center">Sqsh</h1>
      
      <div className="flex justify-center mb-8">
        <label className="flex items-center space-x-3 cursor-pointer">
          <input 
            type="checkbox" 
            checked={overwrite} 
            onChange={(e) => setOverwrite(e.target.checked)}
            className="form-checkbox h-5 w-5 text-primary rounded focus:ring-primary"
          />
          <span className="text-lg font-medium">Overwrite Original Files</span>
        </label>
      </div>

      <div className="border-4 border-dashed border-muted-foreground/20 rounded-xl p-12 mb-8 transition-colors hover:border-primary/50 text-center">
        <p className="text-xl text-muted-foreground">
          Drag & Drop PNG/JPG images here
        </p>
      </div>

      <div className="space-y-4">
        {files.map((file, i) => (
          <div key={i} className="bg-card p-4 rounded-lg shadow border border-border flex justify-between items-center">
            <div className="truncate max-w-[50%]">
              <p className="font-medium">{file.path}</p>
              {file.error && <p className="text-destructive text-sm">{file.error}</p>}
            </div>
            <div className="text-right">
              {file.status === "optimizing" && <span className="text-yellow-500 animate-pulse">Optimizing...</span>}
              {file.status === "done" && file.result && (
                <div className="text-sm">
                  <p className="text-green-500 font-bold">
                    Saved {((file.result.saved_bytes / file.result.original_size) * 100).toFixed(1)}%
                  </p>
                  <p className="text-muted-foreground">
                    {(file.result.original_size / 1024).toFixed(1)}KB → {(file.result.new_size / 1024).toFixed(1)}KB
                  </p>
                </div>
              )}
            </div>
          </div>
        ))}
      </div>
    </main>
  );
}

export default App;
