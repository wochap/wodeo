# editor-layout Specification

## Purpose
Define the Inspector editor layout: its regions, its empty, inspecting, loaded, and trimming states, the output settings controls, and how the layout adapts to narrow windows.

## Requirements

### Requirement: Inspector regions
The editor SHALL consist of a header bar, a preview stage with a transport bar beneath it, a settings sidebar to the right, and a full-width timeline at the bottom.

#### Scenario: Window at design size
- **WHEN** the window is at least 960px wide
- **THEN** the sidebar is a fixed 320px column beside the preview and the timeline spans the full width beneath both

#### Scenario: Narrow window
- **WHEN** the window is narrower than 960px
- **THEN** the sidebar is hidden, a header control toggles it as a floating panel over the preview, and every control remains reachable

### Requirement: Empty state
The editor SHALL, with no video loaded, dim the transport, sidebar, and timeline and show a drop target with a `Choose video…` action, the `Enter` and `Ctrl+O` hints, and an `Open…` action in the header.

#### Scenario: No video open
- **WHEN** the application starts without an input
- **THEN** the header reads `No video open`, the drop target is shown, the sidebar controls are disabled, and `Trim & save` is disabled

### Requirement: Inspecting state
The editor SHALL, while a file is being inspected, show the file name and directory in the header, placeholder tags where metadata will appear, a busy indicator over the preview stage, and an empty timeline strip, with all editing controls disabled.

#### Scenario: Inspection in progress
- **WHEN** a file has been chosen and metadata has not arrived
- **THEN** the busy indicator is visible, the transport reads `0:00.000 / —`, and `Trim & save` is disabled

### Requirement: Loaded state header
The editor SHALL show, once a video is ready, its file name, its directory, tags for resolution, video codec, frame rate, and audio presence, the acceleration indicator, and a `Replace` action that opens the file picker.

#### Scenario: Video ready
- **WHEN** metadata arrives for a 1920x1080 H.264 29.97 fps file with AAC audio
- **THEN** the header shows tags `1920×1080`, `h264`, `29.97 fps`, and `AAC audio`

#### Scenario: Silent video
- **WHEN** the file has no audio stream
- **THEN** the audio tag reads `No audio`

### Requirement: Output settings
The sidebar SHALL provide a `Format` control with `MP4`, `WebM`, `GIF`, and `Copy`, a `Quality` control with `Original`, `High`, and `Small`, a `File name` field holding the stem with the extension displayed after it, and a `Save to` field with a folder picker, all preselected from the launch options.

#### Scenario: Defaults from launch
- **WHEN** a video opens with launch format `gif` and no output path
- **THEN** `GIF` is selected, the file name reads `<source-stem>_trim` with extension `.gif`, and `Save to` is the source directory

#### Scenario: Output path from launch
- **WHEN** the application was launched with `--output /tmp/out/clip.mp4`
- **THEN** `Save to` reads `/tmp/out` and the file name reads `clip` with extension `.mp4`

#### Scenario: Format change updates extension
- **WHEN** the user switches the format from `MP4` to `WebM`
- **THEN** the displayed extension becomes `.webm` and the stem is unchanged

#### Scenario: Copy disables quality
- **WHEN** `Copy` is selected
- **THEN** the `Quality` control is disabled and the extension is the loaded source file's extension in lowercase (for example `.mkv` for `clip.MKV`)

#### Scenario: Choose folder
- **WHEN** the user activates the folder picker and chooses a directory
- **THEN** `Save to` shows that directory and the next trim writes there

### Requirement: Sidebar footer and status
The sidebar footer SHALL show a status line, a `Cancel` action, and a `Trim & save` action. The status line SHALL describe the effect of the selected format: re-encoding formats read as frame-exact, copy reads as fast without re-encoding with the start moved to the nearest keyframe.

#### Scenario: Copy selected with keyframe index
- **WHEN** `Copy` is selected, the selection starts at 2.500 s, and the preceding keyframe is at 2.190 s
- **THEN** the status line states that the output starts at 2.190 s

#### Scenario: Trim saved while staying open
- **WHEN** an export succeeds and the on-done policy is `stay`
- **THEN** the footer shows `Saved <path>` and the editor returns to the ready state with the same selection

### Requirement: Trimming state
The editor SHALL, during export, cover the workspace with a blurred modal showing the destination, the percent complete, the current attempt label, an `Esc` hint, and a `Cancel trim` action.

#### Scenario: Export in progress
- **WHEN** FFmpeg reports progress
- **THEN** the modal percent and bar update and the underlying editor does not accept input

#### Scenario: Cancel from the modal
- **WHEN** the user activates `Cancel trim` or presses `Escape`
- **THEN** the existing cancellation confirmation is shown

### Requirement: Design tokens and typography
The editor SHALL use the Nocturne token set, as defined on the Catppuccin Mocha and Latte palettes (background, surface, text, accent ramp, neutral ramp, divider, radii, shadows), for all colors and shapes, SHALL resolve those tokens for the active color theme, SHALL render text in a bundled Inter font that needs no network access, and SHALL show disabled controls at 45% opacity.

#### Scenario: Offline launch
- **WHEN** the application starts without network access
- **THEN** the interface renders in Inter with no fallback font substitution

#### Scenario: Tokens resolve per theme
- **WHEN** the active color theme changes
- **THEN** every token-colored element, including translucent fills derived from a token, re-renders in that theme's value of the same token

### Requirement: Token-colored outlines
Every border that the editor gives a token color (accent, danger, or a neutral step) SHALL render in that exact token color, not in the default divider color. The selected option of a segmented control SHALL show a complete accent outline that follows the control's rounded outer corners.

#### Scenario: Timeline selection frame
- **WHEN** a video is loaded and a trim range is selected
- **THEN** the frame around the selected range renders as a solid accent-colored border with its accent glow

#### Scenario: Outline and danger buttons
- **WHEN** an outline or danger button is visible
- **THEN** its border renders in the accent or danger color respectively

#### Scenario: Focused text field
- **WHEN** a text field has keyboard focus
- **THEN** its border renders in the accent color

#### Scenario: Selected first or last segment
- **WHEN** the first or last option of the Format or Quality control is selected
- **THEN** the accent outline is continuous along all four sides and curves with the control's rounded corners, with no clipped or missing corner segments

#### Scenario: Selected middle segment
- **WHEN** a middle option of a segmented control is selected
- **THEN** the accent outline is continuous along all four sides of that option
