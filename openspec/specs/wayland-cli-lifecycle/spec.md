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
The executable SHALL accept `wodeo [INPUT]`, `-o|--output <PATH>`, `--format <mp4|webm|gif|copy>`, `--quality <original|high|small>`, `--on-done <exit|stay>`, `-v|--verbose`, `--completions <zsh>`, `-h|--help`, and `-V|--version` and SHALL reject unknown or malformed arguments before opening the window.

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

### Requirement: Shell completion output
The executable SHALL accept `--completions <SHELL>`, where `SHELL` is `zsh`, and SHALL print the embedded zsh completion script to stdout and exit 0 without verifying Wayland, reading configuration, initializing logging to files, or creating a window. Unsupported shell names MUST be rejected with usage on stderr and exit code 2.

#### Scenario: Print zsh completion
- **WHEN** the user runs `wodeo --completions zsh`
- **THEN** stdout contains a script beginning with `#compdef wodeo` and the process exits 0

#### Scenario: Completion without Wayland
- **WHEN** the user runs `wodeo --completions zsh` with no Wayland display available
- **THEN** the script is printed, no Wayland diagnostic is written, and the process exits 0

#### Scenario: Unsupported shell
- **WHEN** the user runs `wodeo --completions fish`
- **THEN** clap prints usage to stderr and the process exits 2

#### Scenario: Load completion via eval
- **WHEN** a zsh user with `compinit` loaded runs `eval "$(wodeo --completions zsh)"` or `zsh-defer eval "$(wodeo --completions zsh)"`
- **THEN** `wodeo <TAB>` completes flags, enum values for `--format`, `--quality`, `--on-done`, and input video files

#### Scenario: Load completion via fpath
- **WHEN** `_wodeo` is in a directory on `fpath` before `compinit`
- **THEN** `wodeo <TAB>` completes the same flags and values as the eval path

### Requirement: Completion script covers the CLI
The zsh completion script SHALL mention every long and short flag defined by the CLI parser, and its `INPUT` file pattern SHALL match exactly the extensions accepted by input validation.

#### Scenario: Flag added without completion
- **WHEN** a developer adds a CLI flag without updating `_wodeo`
- **THEN** the Rust test suite fails naming the missing flag

#### Scenario: Input extension allowlist
- **WHEN** input validation accepts only `mp4`
- **THEN** the completion completes `INPUT` with `*.mp4` files (case-insensitive) and directories

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

### Requirement: Window reveal without unstyled content
The system SHALL create the main window hidden and SHALL show it only after the editor's first render has been committed, so that the first frame the compositor displays is the styled editor rather than a blank or white surface. The native window and the document SHALL both carry a background matching the active theme's base color (Catppuccin Mocha base `#1e1e2e` in dark mode, Latte base `#eff1f5` in light mode) before any application stylesheet loads. Readiness MUST NOT depend on `requestAnimationFrame` or any other callback that is suspended while the window is hidden. If the frontend has not shown the window within one second of application setup, the backend SHALL show it anyway.

#### Scenario: Normal launch
- **WHEN** the application starts and the editor completes its first render
- **THEN** the frontend shows the main window and the first visible frame is the styled editor with no white flash

#### Scenario: Frontend fails to signal readiness
- **WHEN** the frontend does not show the window within one second of setup (for example, a script error before the first render)
- **THEN** the backend shows the main window so the process is never left running with no visible window

#### Scenario: Background before stylesheet
- **WHEN** the window becomes visible before the application stylesheet has applied
- **THEN** the visible background is the theme's base color, not white

#### Scenario: Show permission
- **WHEN** the frontend requests that the main window be shown
- **THEN** the request is permitted by the main window's capability set

#### Scenario: HiDPI first frame
- **WHEN** the window is first shown on an output with a scale factor of 2
- **THEN** the first visible frame is rendered at the output's scale, not at 1× and then corrected
