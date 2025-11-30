# Proposed Improvements

Here are several suggestions to enhance the sqsh application, focusing on usability, visual feedback, and utility.

## User Experience (UX) & Utility

### ~~1. Global Progress & Summary~~ (completed)

* **Feature:** Add a global progress bar for batch operations.
* **Benefit:** Users can instantly see how far along a large batch process is.
* **Addition:** Display a "Session Summary" showing total bytes saved and total percentage reduction across all files.

### ~~2. Custom Quality Control~~ (completed)

* **Feature:** Add a slider to adjust compression quality (e.g., 1-100 for JPEG/WebP).
* **Benefit:** Currently, quality is fixed (e.g., 80). Power users often need to balance quality vs. size manually.

### 3. Output Folder Selection

* **Feature:** Add a third option besides "Overwrite" and "Zip": "Save to Folder".
* **Benefit:** Allows users to process a folder structure and save the optimized version to a new directory while preserving the structure, without modifying the originals or dealing with a zip file.

### 4. Image Comparison (Before/After)

* **Feature:** Click on a processed image to open a modal with a "slider" view comparing the original and optimized versions.
* **Benefit:** Builds trust. Users can verify that the visual quality loss is acceptable before committing.

### ~~5. Backups~~ (completed)

* **Feature:** Add a backup option to save the original files before processing. There should be a toggle to enable this option. The option should be disabled by default. The option should be named "Backup Original Files". The backup should be a zip file named "sqsh-backup-{timestamp}.zip" (do not include the {} in the name). The backup should be created in the same directory as the original files. The option should be placed under the "Overwrite" and above the "Automatically convert" options. The option should persist to the user's preferences.
* **Benefit:** Prevents accidental loss of original files.

## User Interface (UI) & Design

### ~~6. Interactive Drop Zone~~ (completed)

* **Improvement:** The drop zone currently looks static.
* **Suggestion:** Add visual feedback (border highlight, color change, or animation) when the user drags files over the window.
* Tech: Listen to dragenter and dragleave events on the window/container.

### ~~7. Toast Notifications~~ (completed)

* **Improvement:** Better feedback for completed actions.
* **Suggestion:** Show a small toast notification when a batch finishes or when files are successfully saved/zipped.

### 8. Empty State Polish

* **Improvement:** The initial screen is functional but could be more inviting.
* **Suggestion:** Use a more illustrative icon or graphic for the empty state/drop zone instead of just text and a dashed border.

### 9. File List Grouping

* **Improvement:** If a user drops a folder, group those files visually in the list under a folder header, rather than a flat list of hundreds of files.
* **Benefit:** Makes the history list much cleaner and easier to scan.

## Development Tasks

### ~~10. Implement cargo-bump for versioning~~ (completed)

* **Benefit:** Easy way to bump the version number.
* **Suggestion:** Use cargo-bump to bump the version number.

### 11. Github Release Action

* **Benefit:** Easy way to create a release.
* **Suggestion:** Use Github Actions to create a release when a new version is pushed to the main branch. Use git tags (vx.y.z) to trigger the release.
