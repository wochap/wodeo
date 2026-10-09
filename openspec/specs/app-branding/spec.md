# app-branding Specification

## Purpose

Define the application icon artwork and the rule that every packaged icon size is generated from one vector source.

## Requirements

### Requirement: Icon artwork
The application icon SHALL be the "Clip" design: a rounded-square tile with a horizontal filmstrip, dimmed frames outside a selection, an accent-colored trim bracket with grip lines, and a light playhead with a diamond head, using the Nocturne palette.

#### Scenario: Icon at large size
- **WHEN** the icon is rendered at 256px or larger
- **THEN** the filmstrip sprocket holes, bracket grips, and playhead diamond are all visible

#### Scenario: Icon at small size
- **WHEN** the icon is rendered at 32px or 16px
- **THEN** the accent bracket and light playhead remain distinguishable against the dark tile

### Requirement: Generated icon sizes
The system SHALL derive every icon file referenced by the Tauri configuration from one committed SVG through a repeatable script, and the generated files SHALL be committed.

#### Scenario: Regenerate icons
- **WHEN** a developer runs the icon script after editing the SVG
- **THEN** every file under `src-tauri/icons/` is rewritten from the SVG and `tauri build` uses the new artwork

#### Scenario: Window and desktop entry
- **WHEN** the application runs under Wayland
- **THEN** the compositor shows the Clip icon for the window and the desktop entry

### Requirement: Icon resolvable by application id
The Clip icon SHALL be resolvable from the user's icon theme by the name `wodeo`, which matches the Wayland application id, both for the installed package and for development runs.

#### Scenario: Installed package
- **WHEN** the packaged application is installed in the user's profile and a window is open
- **THEN** a shell or bar that looks up the icon theme by the window's application id shows the Clip icon

#### Scenario: Development run
- **WHEN** a developer runs the icon install script and then starts the application in development mode
- **THEN** a shell or bar that looks up the icon theme by the window's application id shows the Clip icon

#### Scenario: Icons regenerated
- **WHEN** a developer regenerates the icons from the SVG and runs the icon install script again
- **THEN** the installed development icons are replaced with the new artwork

### Requirement: Document identity
The web document loaded into the main window SHALL be titled `wodeo` and SHALL NOT reference template assets from the project scaffold (such as the Vite favicon).

#### Scenario: Document title
- **WHEN** the main window's document loads
- **THEN** its title is `wodeo`

#### Scenario: No template favicon
- **WHEN** the document head is inspected
- **THEN** it contains no reference to `/vite.svg` or other scaffold template assets
