import { useState, useEffect } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import "./App.css";

interface OptimizationResult {
  original_size: number;
  new_size: number;
  saved_bytes: number;
  output_path: string;
  skipped: boolean;
  duration_ms: number;
}

interface ProcessedFile {
  id: string;
  path: string;
  status: "pending" | "optimizing" | "done" | "error";
  result?: OptimizationResult;
  error?: string;
}

function formatBytes(bytes: number, decimals = 2) {
  if (!+bytes) return '0 Bytes';

  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];

  const i = Math.floor(Math.log(bytes) / Math.log(k));

  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`;
}

function formatDuration(ms: number) {
  const seconds = ms / 1000;
  if (seconds < 0.1) return "< 0.1s";
  return `${seconds.toFixed(1)}s`;
}

function App() {
  const [files, setFiles] = useState<ProcessedFile[]>([]);
  const [overwrite, setOverwrite] = useState(true);
  const [darkMode, setDarkMode] = useState(true);
  const [convertEnabled, setConvertEnabled] = useState(false);
  const [convertFormat, setConvertFormat] = useState("jpg");
  const [quality, setQuality] = useState(6);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    invoke<{
      dark_mode: boolean;
      overwrite: boolean;
      convert_enabled: boolean;
      convert_format: string;
      quality: number;
    }>("get_config").then((config) => {
      setDarkMode(config.dark_mode);
      setOverwrite(config.overwrite);
      setConvertEnabled(config.convert_enabled);
      setConvertFormat(config.convert_format);
      setQuality(config.quality);
      setLoaded(true);
    }).catch(console.error);
  }, []);

  useEffect(() => {
    if (loaded) {
      invoke("update_settings", {
        darkMode,
        overwrite,
        convertEnabled,
        convertFormat,
        quality
      }).catch(console.error);
    }
  }, [darkMode, overwrite, convertEnabled, convertFormat, quality, loaded]);

  useEffect(() => {
    if (darkMode) {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
  }, [darkMode]);

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
  }, [overwrite, convertEnabled, convertFormat, quality]);

  const handleFiles = async (droppedPaths: string[]) => {
    let allPaths: string[] = [];
    try {
      allPaths = await invoke<string[]>("scan_directory", { paths: droppedPaths });
    } catch (e) {
      console.error("Failed to scan directories:", e);
      allPaths = droppedPaths;
    }

    const newFiles = allPaths.map((path) => ({
      id: crypto.randomUUID(),
      path,
      status: "pending" as const,
    }));
    
    // Prepend new files to show newest first
    setFiles((prev) => [...newFiles, ...prev]);

    const results = new Map<string, string>();
    const CONCURRENCY_LIMIT = 4;
    let activeCount = 0;
    let currentIndex = 0;

    const processNext = async () => {
      if (currentIndex >= newFiles.length) return;

      const file = newFiles[currentIndex];
      currentIndex++;
      activeCount++;

      setFiles((prev) =>
        prev.map((f) =>
          f.id === file.id ? { ...f, status: "optimizing" } : f
        )
      );

      try {
        const result = await invoke<OptimizationResult>("optimize_image", {
          filePath: file.path,
          overwrite: overwrite,
          convertTo: convertEnabled ? convertFormat : null,
          qualityStep: quality,
        });
        
        if (!result.skipped) {
          results.set(file.id, result.output_path);
        }

        setFiles((prev) =>
          prev.map((f) =>
            f.id === file.id ? { ...f, status: "done", result } : f
          )
        );
      } catch (e) {
        setFiles((prev) =>
          prev.map((f) =>
            f.id === file.id
              ? { ...f, status: "error", error: String(e) }
              : f
          )
        );
      } finally {
        activeCount--;
        await processNext();
      }
    };

    // Start initial batch
    const initialPromises = [];
    for (let i = 0; i < Math.min(CONCURRENCY_LIMIT, newFiles.length); i++) {
      initialPromises.push(processNext());
    }

    await Promise.all(initialPromises);

    if (!overwrite && results.size > 0) {
      if (results.size === 1 && newFiles.length === 1) {
        const file = newFiles[0];
        const outputPath = results.get(file.id);
        if (outputPath) {
            const originalPath = file.path;
            try {
              const savePath = await save({
                defaultPath: originalPath,
                filters: [{
                  name: 'Image',
                  extensions: ['png', 'jpg', 'jpeg', 'webp']
                }]
              });
    
              if (savePath) {
                 await invoke("save_file", { srcPath: outputPath, destPath: savePath });
              }
            } catch (e) {
              console.error("Failed to save file:", e);
            }
        }
      } else {
        try {
          const savePath = await save({
            defaultPath: 'sqsh.zip',
            filters: [{
              name: 'Zip Archive',
              extensions: ['zip']
            }]
          });

          if (savePath) {
            const filesToZip: [string, string][] = [];
            
            for (const file of newFiles) {
                const outputPath = results.get(file.id);
                if (!outputPath) continue;
                
                const originalPath = file.path;
                let nameInZip = originalPath.split(/[\\/]/).pop() || "image";

                for (const droppedPath of droppedPaths) {
                   if (originalPath.startsWith(droppedPath)) {
                       if (originalPath === droppedPath) {
                           nameInZip = originalPath.split(/[\\/]/).pop() || "image";
                       } else {
                           const lastSep = Math.max(droppedPath.lastIndexOf('/'), droppedPath.lastIndexOf('\\'));
                           if (lastSep !== -1) {
                               nameInZip = originalPath.substring(lastSep + 1);
                           } else {
                               nameInZip = originalPath;
                           }
                       }
                       break;
                   }
               }
               
               // Normalize to forward slashes for zip spec
               nameInZip = nameInZip.replace(/\\/g, '/');
               
               const newExt = outputPath.split('.').pop();
               const lastDot = nameInZip.lastIndexOf('.');
               if (lastDot !== -1) {
                   nameInZip = nameInZip.substring(0, lastDot) + '.' + newExt;
               } else {
                   nameInZip = nameInZip + '.' + newExt;
               }
               
               filesToZip.push([outputPath, nameInZip]);
            }

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
    <main className="h-screen flex flex-col bg-background text-foreground transition-colors duration-300 overflow-hidden">
      {/* Header & Controls */}
      <div className="p-8 pb-4 shrink-0">
        <div className="flex justify-between items-center mb-8">
          <div className="flex items-baseline gap-2">
            <h1 className="text-4xl font-bold text-primary">sqsh</h1>
            <span className="text-xs text-muted-foreground">v1.0.0</span>
          </div>
          <button 
            onClick={() => setDarkMode(!darkMode)}
            className="p-2 rounded-full hover:bg-muted transition-colors"
            title="Toggle Dark Mode"
          >
            {darkMode ? (
              <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z" />
              </svg>
            ) : (
              <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z" />
              </svg>
            )}
          </button>
        </div>
        
        <div className="flex flex-col items-center gap-4 mb-8">
          {/* Overwrite Toggle */}
          <div className="flex items-center gap-4 bg-card p-3 rounded-lg border border-border shadow-sm w-full max-w-md">
            <label className="flex items-center cursor-pointer">
              <div className="relative">
                <input 
                  type="checkbox" 
                  checked={overwrite} 
                  onChange={(e) => setOverwrite(e.target.checked)}
                  className="sr-only"
                />
                <div className={`block w-10 h-6 rounded-full transition-colors duration-200 ease-in-out ${overwrite ? 'bg-primary' : 'bg-gray-400'}`}></div>
                <div className={`dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform duration-200 ease-in-out shadow ${overwrite ? 'transform translate-x-4' : ''}`}></div>
              </div>
              <span className="ml-2 text-sm font-medium text-foreground">Overwrite Original Files</span>
            </label>
          </div>

          {/* Convert Controls */}
          <div className="flex items-center gap-4 bg-card p-3 rounded-lg border border-border shadow-sm w-full max-w-md">
            <label className="flex items-center cursor-pointer">
              <div className="relative">
                <input 
                  type="checkbox" 
                  checked={convertEnabled} 
                  onChange={(e) => setConvertEnabled(e.target.checked)}
                  className="sr-only"
                />
                <div className={`block w-10 h-6 rounded-full transition-colors duration-200 ease-in-out ${convertEnabled ? 'bg-primary' : 'bg-gray-400'}`}></div>
                <div className={`dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform duration-200 ease-in-out shadow ${convertEnabled ? 'transform translate-x-4' : ''}`}></div>
              </div>
              <span className="ml-2 text-sm font-medium text-foreground">Automatically convert images</span>
            </label>

            <select
              value={convertFormat}
              onChange={(e) => setConvertFormat(e.target.value)}
              disabled={!convertEnabled}
              className="bg-background border border-border text-foreground text-sm rounded-md focus:ring-primary focus:border-primary block p-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <option value="jpg">JPEG</option>
              <option value="png">PNG</option>
              <option value="webp">WEBP</option>
            </select>
          </div>

          {/* Quality Slider */}
          <div className="flex flex-col gap-2 bg-card p-3 rounded-lg border border-border shadow-sm w-full max-w-md">
            <div className="flex justify-between items-center">
              <span className="text-sm font-medium text-foreground">Quality</span>
              <span className="text-xs text-muted-foreground">{quality} / 7</span>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-xs text-muted-foreground">Min</span>
              <input 
                type="range" 
                min="0" 
                max="7" 
                step="1"
                value={quality}
                onChange={(e) => setQuality(parseInt(e.target.value))}
                className="w-full h-2 bg-muted rounded-lg appearance-none cursor-pointer accent-primary"
              />
              <span className="text-xs text-muted-foreground">Max</span>
            </div>
          </div>
        </div>

        <div className="border-4 border-dashed border-muted-foreground/20 rounded-xl p-12 transition-colors hover:border-primary/50 text-center">
          <p className="text-xl text-muted-foreground mb-2">
            Drag & drop images or folders here
          </p>
          <p className="text-sm text-muted-foreground/60">
            Supports PNG, JPG, WEBP, BMP, TIFF, GIF, ICO, TGA, DDS, PNM, QOI
          </p>
        </div>
      </div>

      {/* Scrollable History Area */}
      {files.length > 0 && (
        <div className="px-8 pb-4 shrink-0 flex justify-between items-center">
          <h2 className="text-2xl font-semibold text-primary">Session History</h2>
          <button 
            onClick={() => setFiles([])}
            className="text-sm text-muted-foreground hover:text-destructive transition-colors"
          >
            Clear History
          </button>
        </div>
      )}

      {/* Scrollable History Area */}
      <div className="flex-1 overflow-y-auto p-8 pt-0 space-y-4">
        {files.map((file) => (
          <div key={file.id} className="bg-card p-4 rounded-lg shadow border border-border flex items-center gap-4">
            {/* Thumbnail */}
            <div className="w-16 h-16 shrink-0 bg-muted rounded overflow-hidden flex items-center justify-center">
              <img 
                src={convertFileSrc(file.path)} 
                alt="Thumbnail" 
                className="w-full h-full object-cover"
                onError={(e) => {
                    (e.target as HTMLImageElement).style.display = 'none';
                }}
              />
            </div>

            {/* File Info */}
            <div className="flex-1 min-w-0">
              <p className="font-medium truncate" title={file.path}>{file.path.split(/[\\/]/).pop()}</p>
              {file.error ? (
                <p className={`text-sm ${file.error.includes("Skipped") ? "text-yellow-500" : "text-destructive"}`}>
                  {file.error}
                </p>
              ) : (
                <div className="text-sm text-muted-foreground flex items-center gap-2">
                  {file.status === "done" && file.result ? (
                    file.result.skipped ? (
                      <span>No savings possible</span>
                    ) : (
                      <>
                        <span>{formatBytes(file.result.original_size)}</span>
                        <span>→</span>
                        <span>{formatBytes(file.result.new_size)}</span>
                        <span className="text-xs bg-muted/50 px-1.5 py-0.5 rounded ml-2">
                          {formatDuration(file.result.duration_ms)}
                        </span>
                      </>
                    )
                  ) : (
                    <span>{file.status === "optimizing" ? "Optimizing..." : "Pending"}</span>
                  )}
                </div>
              )}
            </div>

            {/* Savings */}
            <div className="text-right shrink-0">
              {file.status === "done" && file.result && (
                file.result.skipped ? (
                  <span className="text-sm font-bold text-muted-foreground">Skipped</span>
                ) : (
                  <span className="text-2xl font-bold text-green-500">
                    -{((file.result.saved_bytes / file.result.original_size) * 100).toFixed(0)}%
                  </span>
                )
              )}
            </div>
          </div>
        ))}
      </div>
    </main>
  );
}

export default App;
