# wayland-cli-lifecycle Specification

## Purpose
Define the Wayland-only window lifecycle and command-line process, stream, and exit behavior.

## Requirements

### Requirement: Wayland-only runtime
The system MUST set GTK's backend to Wayland before initializing Tauri, MUST verify that a Wayland display is available, and MUST NOT fall back to X11 or XWayland.

#### Scenario: Launch under Wayland
- **WHEN** a reachable Wayland display is available
- **THEN** the application initializes with `GDK_BACKEND=wayland` and logs the selected Wayland display

#### Scenario: Launch without Wayland
- **WHEN** no usable Wayland display is available
- **THEN** the application prints a clear diagnostic to stderr and exits nonzero before creating a window

### Requirement: Undecorated single-purpose window
The system SHALL create one resizable main window without server-side or client-side title-bar decorations, minimize controls, maximize controls, or close controls.

#### Scenario: Window opens
- **WHEN** Tauri creates the main window
- **THEN** the video workspace occupies an undecorated window whose application-level exit action is `Cancel`

### Requirement: CLI input and output options
The executable SHALL accept `wodeo [INPUT]`, `-o|--output <PATH>`, `--format <mp4|webm|gif|copy>`, `--quality <original|high|small>`, `--on-done <exit|stay>`, `-v|--verbose`, `-h|--help`, and `-V|--version` and SHALL reject unknown or malformed arguments before opening the window.

#### Scenario: Input argument supplied
- **WHEN** the user launches with one valid positional input path
- **THEN** the application loads that path without first opening a picker

#### Scenario: No input argument supplied
- **WHEN** the user launches without an input path
- **THEN** the application displays the empty-state file-selection target

#### Scenario: Output argument supplied
- **WHEN** `--output` identifies a destination
- **THEN** its directory and file name are preselected in the editor's output fields, the user may still edit them, and no save picker is shown

#### Scenario: Output argument omitted
- **WHEN** the user opens a video without `--output`
- **THEN** the editor preselects the source directory and the derived name `<source-stem>_trim.<ext>` where `<ext>` follows the effective format

#### Scenario: Export option flags
- **WHEN** `--format`, `--quality`, or `--on-done` is given
- **THEN** the corresponding option overrides the configuration file value and is preselected in the editor

#### Scenario: Force supplied
- **WHEN** the user passes `-f` or `--force`
- **THEN** the application rejects the unknown flag with usage text on stderr and exits nonzero before creating a window

#### Scenario: Invalid option value
- **WHEN** a flag receives a value outside its accepted set
- **THEN** the application prints usage to stderr and exits nonzero before creating a window

### Requirement: Process ownership
The system SHALL keep each invocation attached to the window and export it started and SHALL NOT forward arguments or completion to a pre-existing single-instance process.

#### Scenario: Launch while another instance is running
- **WHEN** the executable is invoked while another wodeo process exists
- **THEN** the new process opens its own window and retains its own stdout, stderr, and exit status

### Requirement: CLI stream and exit semantics
The system MUST reserve application stdout for canonical absolute destination paths of successful exports, one per line; normal and verbose diagnostics MUST go to stderr and log files.

#### Scenario: Successful terminal invocation
- **WHEN** export succeeds with on-done `exit`
- **THEN** stdout contains only the canonical absolute output path followed by a newline and the process exits with status zero

#### Scenario: Multiple exports while staying open
- **WHEN** on-done is `stay` and the user completes several exports before closing
- **THEN** stdout contains one canonical path line per successful export, in order, and the process exits with status zero when closed

#### Scenario: Failure
- **WHEN** validation, startup, or export fails and no export has succeeded
- **THEN** stdout remains empty, a diagnostic is written to stderr, and the process exits nonzero

#### Scenario: User cancellation
- **WHEN** the user exits or confirms export cancellation before any successful completion
- **THEN** stdout remains empty and the process exits with the documented cancellation status
