# keyboard-shortcut-hints Specification

## Purpose

Define how the editor surfaces keyboard shortcuts: the responsive transport key bar and its handle mode, the focused handle tip, the `?` shortcut reference popover, and the single key cap style used across the interface.

## Requirements

### Requirement: Transport key bar
The transport bar SHALL end with an always-on key bar showing `Space` Play, `←`/`→` Frame, `I`/`O` In / out, and a `?` button labelled `All keys`. The key bar SHALL degrade by its own width rather than the window's: key and label above 360px, keys only (labels moved to each hint's tooltip) at 360px or less, `I`/`O` and the `All keys` label hidden at 236px or less, and only the `?` button at 132px or less. The `?` button SHALL always remain visible.

#### Scenario: Wide key bar
- **WHEN** the editor is ready and the key bar is wider than 360px
- **THEN** it shows `Space` Play, `←` `→` Frame, `I` `O` In / out, and `?` All keys, each with its key caps and label

#### Scenario: Narrow key bar
- **WHEN** the key bar is 236px wide or less but wider than 132px
- **THEN** it shows the `Space` and `←` `→` key caps without labels and the `?` button without its label, and hides `I` `O`

#### Scenario: Narrowest key bar
- **WHEN** the key bar is 132px wide or less
- **THEN** only the `?` button is shown

#### Scenario: Label available as tooltip
- **WHEN** a hint's label is hidden for lack of space
- **THEN** the hint's full meaning remains available as its tooltip

### Requirement: Handle mode
While a trim handle has keyboard focus, the key bar SHALL switch to handle mode: it shows an `In handle` or `Out handle` label with an accent dot, followed by `←` `→` Frame, `Shift` ±1 s, `Alt` Keyframe, and the `?` button. In handle mode the `Alt` hint SHALL be hidden at 440px or less, and the mode label and its dot SHALL remain visible above 132px. The key bar SHALL return to its normal hints when the handle loses focus.

#### Scenario: Focus the start handle
- **WHEN** the start trim handle receives keyboard focus
- **THEN** the key bar shows `In handle` followed by the Frame, ±1 s and Keyframe hints and the `?` button

#### Scenario: Focus leaves the handle
- **WHEN** focus moves from a trim handle to any other control
- **THEN** the key bar shows the normal `Space`, `←` `→`, `I` `O` and `?` hints again

### Requirement: Focused handle tip
While a trim handle has keyboard focus, the timeline SHALL show a small tip beneath that handle with the handle kind and its current time (for example `IN 0:44.185` or `OUT 3:26.020`) and the `Home` `End` Limits hint. The tip SHALL follow the handle as it moves and SHALL disappear when the handle loses focus. Pointer hover or drag without keyboard focus SHALL NOT show it.

#### Scenario: Tip follows the boundary
- **WHEN** the focused end handle moves one second later
- **THEN** the tip under it shows `OUT` with the updated time

#### Scenario: No tip without keyboard focus
- **WHEN** no trim handle has focus
- **THEN** no handle tip is shown

### Requirement: Shortcut reference
The system SHALL provide a keyboard shortcut reference, labelled `Keyboard shortcuts`, as a non-modal popover over the bottom right of the preview stage. It SHALL be opened and closed by pressing `?` while the editor has focus or by activating the `?` All keys button, and closed by `Esc`. The button SHALL reflect the state through `aria-expanded`. The reference SHALL group the shortcuts as:
- Playback: `Space` play / pause.
- Navigate (noted as moving the playhead or the focused trim handle): `←` `→` ±1 frame, `Shift` `←` `→` ±1 second, `PgUp` `PgDn` ±10 seconds, `Alt` `←` `→` previous / next keyframe, `0`–`9` jump to 0–90%.
- Trim: `I` `O` set in / out at the playhead; `Home` `End` to its start / end limit, marked as acting on the focused handle only.
- Actions: `Enter` trim & save, `Esc` cancel, `Ctrl` `O` open video, `?` this reference.

The reference SHALL NOT list any binding outside this set.

#### Scenario: Open with the question mark key
- **WHEN** the editor has focus, the reference is closed, and the user presses `?`
- **THEN** the reference opens and the All keys button reports `aria-expanded="true"`

#### Scenario: Toggle with the button
- **WHEN** the reference is open and the user activates the All keys button
- **THEN** the reference closes and the button reports `aria-expanded="false"`

#### Scenario: Escape closes only the reference
- **WHEN** the reference is open and the user presses `Esc`
- **THEN** the reference closes, focus returns to the control that had it before the reference opened (or the All keys button), and no cancel or exit is requested

#### Scenario: Question mark inside a text field
- **WHEN** a text field such as `File name` has focus and the user types `?`
- **THEN** the character is entered into the field and the reference does not open

#### Scenario: Handle-only rows are marked
- **WHEN** the reference is open
- **THEN** the `Home` `End` row carries a solid accent scope marker for the focused handle, and the playhead scope uses a hollow marker

### Requirement: Single key cap style
Every keyboard key shown in the interface (the key bar, handle tip, reference, empty state, header, export dialog, and sidebar actions) SHALL use one key cap style: monospaced 10.5px medium text, an 18px minimum box, a divider-colored border with a 2px bottom edge, and a small radius. No interface text SHALL present the retired bindings `,`, `.`, `[`, `]`, or `⌘↵`.

#### Scenario: Consistent caps
- **WHEN** the empty state, header, export dialog, key bar, and reference are rendered
- **THEN** every key is drawn with the same key cap component and style

#### Scenario: Retired bindings absent
- **WHEN** any editor screen or the reference is rendered
- **THEN** no key cap reads `,`, `.`, `[`, `]`, or `⌘↵`
