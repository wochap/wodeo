# trim-interaction Specification

## Purpose
Define the editor workspace, precise range selection, accessibility, and cancellation interactions.

## Requirements

### Requirement: QuickTime-like trim workspace
The system SHALL display the video prominently above a transport bar and a compact thumbnail timeline containing a ruler, a playhead, start handle, end handle, current selection, and time labels, with a settings sidebar holding the selection fields, output options, `Cancel`, and `Trim & save`.

#### Scenario: Editor becomes ready
- **WHEN** a video's metadata and preview are available, whether or not its thumbnails have finished generating
- **THEN** the trim range spans the full video and the workspace shows the video, transport, timeline (with placeholders for thumbnails not yet written), sidebar, `Cancel`, and `Trim & save`

### Requirement: Complete mouse operation
The system SHALL allow playback, seeking, range selection, cancellation, and export initiation using only a pointing device.

#### Scenario: Adjust trim range with the mouse
- **WHEN** the user drags either timeline handle
- **THEN** the corresponding boundary updates continuously, remains within the media duration, and cannot cross the other handle

#### Scenario: Seek with the mouse
- **WHEN** the user clicks or drags within the timeline selection
- **THEN** the playhead and preview seek to the corresponding media time without moving a trim boundary

#### Scenario: Control playback with the mouse
- **WHEN** the user activates the visible play/pause control or clicks the video
- **THEN** the preview toggles between playing and paused

### Requirement: Complete keyboard operation
The system SHALL allow every trimming workflow action using the keyboard, SHALL avoid overriding keystrokes used by a focused native dialog or text-like control, and SHALL surface the shortcuts through the transport key bar and the `?` shortcut reference (see `keyboard-shortcut-hints`). Navigation keys SHALL act on the focused trim handle when one has keyboard focus, and on the playhead otherwise. A single keypress MUST NOT move both a trim handle and the playhead.

#### Scenario: Playback and seek shortcuts
- **WHEN** the editor has focus, no trim handle is focused, and the user presses `Space`, `Left`, `Right`, `Shift+Left`, or `Shift+Right`
- **THEN** the system respectively toggles playback, seeks approximately one probed frame backward or forward, or seeks one second backward or forward

#### Scenario: Coarse playhead navigation
- **WHEN** the editor has focus, no trim handle is focused, and the user presses `PageUp`, `PageDown`, `Alt+Left`, `Alt+Right`, or a digit `0`–`9` on the main row or numpad
- **THEN** the system respectively seeks ten seconds forward or backward, seeks to the previous or next source keyframe, or seeks to that digit's tenth of the duration (`0` = start, `9` = 90%), clamped to the media duration

#### Scenario: Set boundaries from playhead
- **WHEN** the editor has focus and the user presses `I` or `O`
- **THEN** the system sets the start or end boundary to the current playhead time subject to range validity

#### Scenario: Adjust a focused boundary
- **WHEN** a trim handle has keyboard focus and the user presses `Left`, `Right`, `Shift+Left`, `Shift+Right`, `PageUp`, `PageDown`, `Alt+Left`, `Alt+Right`, a digit `0`–`9` on the main row or numpad, `Home`, or `End`
- **THEN** the system moves that boundary by one probed frame, one second, ten seconds, to the previous or next source keyframe, to that digit's tenth of the duration, or to its allowed duration endpoint, clamped so it never crosses the other handle or leaves its valid range

#### Scenario: Keyframe navigation without an index
- **WHEN** the source keyframe index is empty, or no keyframe exists in the requested direction, and the user presses `Alt+Left` or `Alt+Right`
- **THEN** neither the playhead nor any boundary moves, and the webview does not navigate history

#### Scenario: One target per keypress
- **WHEN** a trim handle has keyboard focus and the user presses a navigation key
- **THEN** only that boundary changes, and the playhead is not separately moved by the same keypress

#### Scenario: Invoke primary actions
- **WHEN** the editor has focus, the shortcut reference is closed, and the user presses `Enter`, `Escape`, or `Ctrl+O`
- **THEN** the system respectively initiates a valid trim, requests cancellation/exit, or opens the input picker

#### Scenario: Open the shortcut reference
- **WHEN** the editor has focus, no text-like control is focused, and the user presses `?`
- **THEN** the system toggles the shortcut reference

#### Scenario: Escape with the reference open
- **WHEN** the shortcut reference is open and the user presses `Escape`
- **THEN** the system closes the reference and does not request cancellation or exit

#### Scenario: Key bar instead of hint chips
- **WHEN** the editor is ready
- **THEN** the transport bar shows the key bar (`Space` Play, `←` `→` Frame, `I` `O` In / out, `?` All keys) and no longer shows the `Shift` 1 s or `Enter` Trim chips

### Requirement: Accessible timeline semantics
The system MUST expose the trim boundaries as independently focusable, labelled slider controls and SHALL announce changing times, validation errors, export progress, and completion through accessible status semantics.

#### Scenario: Navigate with assistive technology
- **WHEN** focus reaches either trim handle
- **THEN** its accessible name, current time, minimum, maximum, and adjustment instructions are available

#### Scenario: Focus visibility
- **WHEN** the user navigates through interactive controls with the keyboard
- **THEN** every focused control has a clearly visible focus indicator

### Requirement: Valid exact selection
The system MUST maintain `0 <= start < end <= duration`, require at least one probed frame between the boundaries, and display normalized time values with millisecond precision.

#### Scenario: Boundary would invalidate the range
- **WHEN** a mouse or keyboard adjustment would cross the other boundary or create a sub-frame selection
- **THEN** the system clamps the adjustment to the nearest valid timestamp and keeps `Trim` valid only for a non-empty range

### Requirement: Cancel behavior
The system SHALL let the user exit from the empty or ready state immediately and SHALL require confirmation before abandoning an export in progress.

#### Scenario: Cancel before export
- **WHEN** the user activates `Cancel` or presses `Escape` before export begins
- **THEN** the application exits without producing output

#### Scenario: Cancel during export
- **WHEN** the user requests cancellation while FFmpeg is running
- **THEN** the system asks for confirmation before terminating the export process

### Requirement: Active boundary frame feedback
The system SHALL seek the video preview and visible playhead to the resulting timestamp of a trim boundary whenever the user adjusts that boundary with its timeline handle.

#### Scenario: Move the start handle
- **WHEN** the user adjusts the start handle with a pointer or keyboard
- **THEN** the start boundary updates within the valid range and the preview seeks to the updated start timestamp

#### Scenario: Move the end handle
- **WHEN** the user adjusts the end handle with a pointer or keyboard
- **THEN** the end boundary updates within the valid range and the preview seeks to the updated end timestamp

#### Scenario: Clamp a boundary adjustment
- **WHEN** a handle adjustment is clamped to preserve a valid selection
- **THEN** the preview seeks to the clamped boundary timestamp rather than the unvalidated requested timestamp

### Requirement: Selection preview playback controls
The system SHALL provide keyboard- and pointer-operable transport controls named `Go to in`, `Previous frame`, `Play`, `Next frame`, `Go to out`, and `Play selection`. `Play selection` SHALL play only the current trim selection before pausing with the preview and playhead at the selection end.

#### Scenario: Preview the selection start
- **WHEN** the user activates `Go to in`
- **THEN** the playhead and preview seek to the selection start without moving a boundary

#### Scenario: Play the full selection
- **WHEN** the user activates `Play selection`
- **THEN** playback starts at the selection start and pauses at the selection end

#### Scenario: Preview the selection end
- **WHEN** the user activates `Go to out`
- **THEN** the playhead and preview seek to the selection end without moving a boundary

#### Scenario: Step one frame
- **WHEN** the user activates `Previous frame` or `Next frame`
- **THEN** the playhead moves by one probed frame in that direction

#### Scenario: Preview a short selection edge
- **WHEN** the user activates `Play selection` for a selection shorter than two seconds
- **THEN** playback covers the full selection without seeking outside its boundaries and pauses at the selection end

#### Scenario: Replace an active bounded preview
- **WHEN** the user activates `Play selection`, `Go to in`, or `Go to out` while a bounded preview is active
- **THEN** the prior bounded interval is discarded and the new request determines playback and the next automatic stop

#### Scenario: Preview is unavailable
- **WHEN** no playable video is ready or an export is in progress
- **THEN** all transport controls are disabled

### Requirement: Editable boundary fields
The sidebar SHALL provide `In` and `Out` text fields showing the boundaries in `m:ss.mmm` form. A field SHALL commit on `Enter` or blur, accept `m:ss.mmm`, `ss.mmm`, and whole seconds, clamp the result to a valid frame-aligned selection, and revert on invalid input or `Escape`.

#### Scenario: Commit a typed in-point
- **WHEN** the user types `0:44.185` in `In` and presses `Enter`
- **THEN** the start boundary becomes the nearest frame-aligned time at or before the end minus one frame and the preview seeks there

#### Scenario: Invalid text
- **WHEN** the user types `abc` in `Out` and blurs the field
- **THEN** the field reverts to the current end boundary and no boundary changes

#### Scenario: Keys inside a field
- **WHEN** a boundary field has focus and the user presses `Enter`, `Space`, `I`, or `O`
- **THEN** the key edits or commits the field and does not trigger the editor shortcuts

### Requirement: Selection summary
The sidebar SHALL show the selection duration, its frame count derived from the probed frame rate, and its percentage of the clip.

#### Scenario: Selection changes
- **WHEN** either boundary moves
- **THEN** the duration, frame count, and percentage update immediately

### Requirement: Coalesced scrubbing
While the user seeks continuously (dragging on the timeline, dragging a trim handle, or holding a frame-step key), the system SHALL run at most one preview seek at a time, SHALL move the playhead indicator immediately with the input, and SHALL finish on the most recent requested position.

#### Scenario: Fast drag across the timeline
- **WHEN** the user drags across the timeline faster than the preview can seek
- **THEN** the playhead indicator tracks the pointer without lag, intermediate positions may be skipped by the preview, and the preview ends on the position where the drag stopped

#### Scenario: Drag a trim handle
- **WHEN** the user drags a trim handle
- **THEN** the boundary updates continuously as before, and the preview ends on the frame at the boundary's final position

#### Scenario: Single seek
- **WHEN** the user clicks once on the timeline
- **THEN** exactly one preview seek runs, to the clicked position

### Requirement: Uninterrupted timeline drag
A drag that starts on the timeline track or on a trim handle SHALL NOT select text or images, and SHALL NOT start a native image drag. A drag that starts on the track SHALL keep scrubbing until the pointer button is released, even when the pointer crosses a trim handle or leaves the track; positions beyond the track SHALL clamp to `0` or the duration. Pressing a trim handle SHALL NOT seek.

#### Scenario: Scrub across thumbnails and labels
- **WHEN** the user presses on the timeline track and drags across the thumbnails and ruler labels
- **THEN** nothing becomes highlighted and the playhead follows the pointer

#### Scenario: Scrub over a trim handle
- **WHEN** a drag that started on the track passes over the start or end handle
- **THEN** scrubbing continues and neither trim boundary moves

#### Scenario: Scrub past the track edge
- **WHEN** a drag that started on the track leaves the track to the left or right while the button is held
- **THEN** the playhead clamps to the start or end of the media and scrubbing resumes when the pointer comes back

#### Scenario: Press a trim handle
- **WHEN** the user presses a trim handle without moving
- **THEN** no seek to the pressed position runs from the track
