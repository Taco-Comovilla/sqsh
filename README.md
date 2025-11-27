# sqsh – Squish your images

**sqsh** is a lightweight desktop application built with Tauri, React, and TypeScript that compresses image files and can optionally convert them to another format. Drag‑and‑drop images onto the window; the app automatically processes them, shows size savings, and lets you save each result or export all as a ZIP archive.

## Features

- Lossless & lossy compression for PNG, JPEG, WebP, BMP, TIFF, GIF, ICO, TGA, DDS, PNM, QOI.
- Optional format conversion (e.g., PNG → JPEG) with selectable target format.
- Batch processing with configurable concurrency.
- Overwrite control – keep originals or replace them.
- Dark‑mode toggle and modern UI built with Tailwind CSS.
- Cross‑platform: Windows, macOS, Linux.

## Quick Start

```bash
# Clone the repository
git clone https://github.com/Taco-Comovilla/sqsh.git
cd sqsh

# Install dependencies
npm install

# Build and run the Tauri app (requires Rust toolchain)
npm run tauri dev
```

## Usage

- Drag and drop image files onto the application window.
- Adjust settings (overwrite, conversion format, dark mode) using the controls at the top.
- The app automatically processes the files and displays size savings.
- Save each optimized file individually or export all results as a ZIP archive via the save dialog.

## License

This project is licensed under the MIT License – see the [LICENSE](LICENSE) file for details.
