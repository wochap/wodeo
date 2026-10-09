# color-theme Specification

## Purpose

Define the Catppuccin Mocha (dark) and Latte (light) color themes, how the editor picks and switches between them from the system preference, and the contrast rules for the light-theme accent.

## Requirements

### Requirement: Catppuccin palettes
The editor SHALL take every color from the Catppuccin palette: Mocha in the dark theme and Latte in the light theme. Semantic tokens (background, surface, text, accent ramp, secondary accent ramp, neutral ramp, divider, danger, shadows) SHALL be defined once against the raw palette, so switching the palette switches every screen. Component code MUST NOT contain literal color values except black for the video letterbox.

#### Scenario: Dark theme colors
- **WHEN** the dark theme is active
- **THEN** the background is Mocha base `#1e1e2e`, the text is Mocha text `#cdd6f4`, and the accent is Mocha lavender `#b4befe`

#### Scenario: Light theme colors
- **WHEN** the light theme is active
- **THEN** the background is Latte base `#eff1f5`, the text is Latte text `#4c4f69`, and panels use Latte mantle `#e6e9ef` as the surface color

#### Scenario: Shadows and divider follow the theme
- **WHEN** the theme switches between dark and light
- **THEN** shadow rings, shadow shade opacity, and the divider color change with it, with no fixed hex or black shadow color left from the other theme

### Requirement: Theme follows the system
The editor SHALL choose the dark or light theme from the webview's `prefers-color-scheme` media query, which reflects the system color-scheme preference that the window toolkit copies from the desktop portal into the GTK dark preference, and SHALL switch themes while running when that preference changes, without a restart. The frontend MUST NOT pin the theme by setting `data-theme` from a value read at startup. `data-theme="light"` or `data-theme="dark"` on the document SHALL remain available only as a manual override. When no preference is reported, the editor SHALL use the dark theme.

#### Scenario: System prefers light at launch
- **WHEN** the application starts while the system color-scheme preference is light
- **THEN** the editor renders in the Latte theme

#### Scenario: System preference changes while running
- **WHEN** the system color-scheme preference changes from dark to light, or from light to dark, while the editor is open
- **THEN** the editor switches to the matching theme without reloading or losing the loaded video, selection, or playhead

#### Scenario: No preference reported
- **WHEN** neither the portal nor `prefers-color-scheme` reports a light preference
- **THEN** the editor renders in the Mocha theme

#### Scenario: Startup theme does not pin the editor
- **WHEN** the application started in one theme and the system preference later changes
- **THEN** no `data-theme` attribute set by the application prevents the `prefers-color-scheme` rules from applying

### Requirement: Native controls match the theme
The editor SHALL declare the active color scheme to the webview, so native scrollbars, form controls, and default canvas colors match the active theme.

#### Scenario: Light theme native controls
- **WHEN** the light theme is active
- **THEN** the document's `color-scheme` is `light` and native controls render in their light appearance

### Requirement: Readable light-theme accent
In the light theme the accent SHALL be Catppuccin Latte blue `#1e66f5` instead of lavender, and the accent ramp steps SHALL be mixed from that per-theme accent base, so text drawn in the accent color on the background or surface reaches at least 3:1 contrast.

#### Scenario: Primary button text in light theme
- **WHEN** the light theme is active and a primary or ghost button labels its text in the accent color
- **THEN** that text is Latte blue and has at least 3:1 contrast against the background and the surface

#### Scenario: Accent ramp matches the accent
- **WHEN** the light theme is active
- **THEN** every accent ramp step (100 to 900) is derived from Latte blue, not from lavender
