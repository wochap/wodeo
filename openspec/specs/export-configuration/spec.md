# export-configuration Specification

## Purpose
Define the user configuration file that stores default export options and how those defaults combine with command-line flags.

## Requirements

### Requirement: Configuration file location
The system SHALL read defaults from `$XDG_CONFIG_HOME/wodeo/config.toml`, falling back to `~/.config/wodeo/config.toml` when `XDG_CONFIG_HOME` is unset, and SHALL treat a missing file as an empty configuration.

#### Scenario: No configuration file
- **WHEN** neither path exists
- **THEN** the application starts with built-in defaults and reports no error

#### Scenario: XDG override
- **WHEN** `XDG_CONFIG_HOME` is set
- **THEN** the application reads only `$XDG_CONFIG_HOME/wodeo/config.toml`

### Requirement: Configuration schema
The configuration file SHALL accept the optional keys `format` (`mp4`, `webm`, `gif`, `copy`), `quality` (`original`, `high`, `small`), and `on_done` (`exit`, `stay`). Built-in defaults are `mp4`, `original`, and `exit`.

#### Scenario: Partial configuration
- **WHEN** the file sets only `format = "gif"`
- **THEN** the effective defaults are format `gif`, quality `original`, on-done `exit`

#### Scenario: Invalid value or unknown key
- **WHEN** the file contains a value outside the accepted set, an unknown key, or invalid TOML
- **THEN** the application prints a diagnostic naming the file and the offending key to stderr and exits with the startup failure status before creating a window

### Requirement: Option precedence
The system SHALL resolve each option in this order, later sources winning: built-in default, configuration file, output path extension (format only), explicit command-line flag.

#### Scenario: Flag overrides file
- **WHEN** the file sets `quality = "small"` and the command line passes `--quality high`
- **THEN** the effective quality is `high`

#### Scenario: Output extension implies format
- **WHEN** no `--format` flag is given and `--output clip.gif` is given
- **THEN** the effective format is `gif` regardless of the file's `format` value

#### Scenario: Flag overrides output extension
- **WHEN** `--format mp4` and `--output clip.gif` are both given
- **THEN** the effective format is `mp4` and the destination becomes `clip.mp4`

### Requirement: Effective options exposed to the editor
The system SHALL pass the effective format, quality, and on-done policy to the editor at launch so the editor can preselect them.

#### Scenario: Editor opens
- **WHEN** the window is created
- **THEN** the launch options delivered to the editor include the resolved `format`, `quality`, and `onDone` values
