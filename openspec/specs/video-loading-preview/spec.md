# video-loading-preview Specification

## Purpose
Define secure selection, validation, preview, and degraded thumbnail behavior for supported video containers (ISO-BMFF and Matroska).

## Requirements

### Requirement: Empty-state video selection
The system SHALL present the undecorated window as a clear file-selection target when no video is loaded, with a `Choose video…` action in the drop target and an `Open…` action in the header, and SHALL allow a user to invoke a file picker filtered to supported video containers using either mouse or keyboard. Once a video is loaded the header action reads `Replace` and opens the same picker.

#### Scenario: Select with the mouse
- **WHEN** the user clicks `Choose video…` or `Open…` and chooses a valid supported video
- **THEN** the system loads that file for inspection and preview

#### Scenario: Select with the keyboard
- **WHEN** the user focuses and activates the empty-state selection target, presses `Enter` in the empty state, or presses `Ctrl+O`
- **THEN** the system opens a keyboard-operable file picker filtered to supported video containers

#### Scenario: Cancel selection
- **WHEN** the user cancels the file picker
- **THEN** the system remains in the empty state without reporting an error

#### Scenario: Replace a loaded video
- **WHEN** the user activates `Replace` and chooses another valid supported video
- **THEN** the system loads the new file, resets the selection to its full duration, and derives a new default output name

### Requirement: Native file drag and drop
The system SHALL accept exactly one local supported video path delivered by Tauri's native drag-and-drop event and SHALL provide visible feedback while an acceptable file is over the window.

#### Scenario: Drop one supported video
- **WHEN** the user drops exactly one valid supported video anywhere on the window while no export is running
- **THEN** the system loads the dropped file and resets the trim selection to its full duration

#### Scenario: Drop an unsupported selection
- **WHEN** the user drops multiple paths, a directory, or a file outside the supported containers
- **THEN** the system rejects the drop with an actionable message and retains the currently loaded video, if any

### Requirement: Input validation and probing
The system MUST use ffprobe to validate that the selected local file is a readable supported video container containing at least one video stream and SHALL obtain its duration, dimensions, codec, frame-rate information, and audio-stream presence before entering the ready state.

#### Scenario: Valid supported video
- **WHEN** ffprobe successfully identifies a video stream in a supported container
- **THEN** the system exposes the probed metadata to the editor and enters the ready state

#### Scenario: Invalid or unreadable input
- **WHEN** the selected path is missing, unreadable, malformed, not a supported container, or contains no video stream
- **THEN** the system displays a concise error and allows the user to choose another file

### Requirement: Supported input containers
The system SHALL accept as input only files whose extension is one of `mp4`, `m4v`, `mov`, `mkv`, `webm` (case-insensitive) and whose ffprobe `format_name` includes `mov` (ISO-BMFF family) or `matroska` (Matroska family). The file picker filter, drag-and-drop acceptance, extension validation, and shell completion SHALL derive from this single allowlist.

#### Scenario: Supported extension and container
- **WHEN** the user opens `clip.MKV` and ffprobe reports `matroska,webm` with a video stream
- **THEN** the system accepts the file and proceeds with inspection

#### Scenario: Unsupported extension
- **WHEN** the user opens `clip.avi` or `clip.ts`
- **THEN** the system rejects it as unsupported before running ffprobe

#### Scenario: Extension and container disagree
- **WHEN** the user opens `clip.mp4` whose ffprobe `format_name` is `avi`
- **THEN** the system rejects the file as an unsupported container

#### Scenario: Fallback preview content type
- **WHEN** proxy generation fails and the original file is served
- **THEN** the response `Content-Type` is `video/mp4` for `mp4`/`m4v`, `video/quicktime` for `mov`, `video/x-matroska` for `mkv`, and `video/webm` for `webm`

### Requirement: Secure local preview
The system SHALL preview the currently selected file through a loopback HTTP endpoint managed by the application, bound to `127.0.0.1` on an ephemeral port, and SHALL authorize only that file's generated preview proxy (or the file itself when proxy generation fails) and its generated thumbnails for serving. The endpoint MUST NOT accept filesystem paths in request URLs and MUST NOT grant the webview blanket access to the user's home directory. Generated thumbnails MAY continue to use the Tauri asset protocol.

#### Scenario: Preview a selected file
- **WHEN** a valid file finishes probing
- **THEN** the system authorizes that file, exposes its loopback preview URL to the editor, loads it into the video element, and provides mouse-operable playback controls

#### Scenario: Replace the selected file
- **WHEN** the user successfully opens or drops a different valid file
- **THEN** the system revokes or stops using the previous preview and displays the new file

### Requirement: Seek-friendly preview proxy
The system SHALL derive a preview proxy from the selected video by re-encoding it into an MP4 with dense keyframes, a leading moov atom, and fresh timing metadata, and SHALL serve that proxy at the loopback media endpoint. The proxy SHALL preserve the source timestamps so that trim positions chosen against the preview remain valid, and export SHALL continue to read the original file.

#### Scenario: Proxy generation succeeds
- **WHEN** a valid supported video finishes probing and FFmpeg builds the proxy
- **THEN** the system plays the proxy in place of the raw file so that seeking lands on a nearby keyframe without flushing the displayed frame

#### Scenario: Proxy generation fails
- **WHEN** FFmpeg cannot build the proxy for a probed video
- **THEN** the system serves the original file at the preview endpoint and records the degraded state

### Requirement: Loopback media streaming
The loopback preview endpoint SHALL support HTTP Range requests so that playback can start and seek without buffering the entire file, SHALL keep a client connection open for further requests unless the client asks to close it, and SHALL serve nothing other than the currently loaded media's preview proxy (or the file itself when proxy generation fails) and its thumbnails.

#### Scenario: Full request
- **WHEN** a client requests the media endpoint without a Range header while a file is loaded
- **THEN** the system responds 200 with `Accept-Ranges: bytes`, the correct `Content-Type`, and the complete file body

#### Scenario: Partial request
- **WHEN** a client requests a satisfiable byte range of the loaded media
- **THEN** the system responds 206 with `Content-Range: bytes <start>-<end>/<length>` and exactly the requested slice

#### Scenario: Unsatisfiable range
- **WHEN** a client requests a range beginning at or beyond the end of the loaded media
- **THEN** the system responds 416 with `Content-Range: bytes */<length>` and no body

#### Scenario: No media loaded
- **WHEN** a client requests the media endpoint while no file is loaded
- **THEN** the system responds with a client error status and no file content

#### Scenario: Unknown endpoint
- **WHEN** a client requests any path other than the media or thumbnail endpoints
- **THEN** the system responds 404 and serves no content

#### Scenario: Sequential requests on one connection
- **WHEN** a client sends a second range request on the same connection after the first response completes
- **THEN** the system answers the second request on that connection without the client reconnecting

#### Scenario: Client asks to close
- **WHEN** a request carries `Connection: close`
- **THEN** the system sends the response with `Connection: close` and closes the connection afterwards

#### Scenario: Idle connection
- **WHEN** a kept-open connection receives no request for a bounded idle period
- **THEN** the system closes it

### Requirement: Preview and thumbnail failure handling
The system SHALL keep file inspection, video playback, and thumbnail generation as separately reportable operations so that a thumbnail failure does not invalidate an otherwise playable video.

#### Scenario: Thumbnail generation fails
- **WHEN** the selected MP4 can be previewed but FFmpeg cannot generate the filmstrip
- **THEN** the system retains playback and trim controls, substitutes a non-thumbnail timeline, and reports the degraded state

#### Scenario: WebKit cannot preview a probed MP4
- **WHEN** FFmpeg accepts the MP4 but the WebKit/GStreamer video element cannot decode it
- **THEN** the system explains that the file cannot be previewed and does not enable trimming

### Requirement: Keyframe index
The system SHALL collect the presentation timestamps of the video stream's keyframes during inspection without decoding frames and SHALL expose them to the editor with the probed metadata.

#### Scenario: Keyframes indexed
- **WHEN** a valid MP4 finishes probing
- **THEN** the metadata delivered to the editor includes an ascending list of keyframe timestamps in microseconds beginning with the first keyframe

#### Scenario: Indexing fails
- **WHEN** keyframe timestamps cannot be read
- **THEN** the editor still enters the ready state with an empty keyframe list and copy exports report the effective start only after export

### Requirement: Inspection progress reporting
The system SHALL report inspection as an ordered sequence of named steps (`Reading container`, `Indexing keyframes`, `Building preview`, `Building thumbnails`) with a completion fraction, and the editor SHALL show completed, active, and pending steps with a progress bar while inspecting.

#### Scenario: Step advances
- **WHEN** the container probe finishes and keyframe indexing starts
- **THEN** the editor marks `Reading container` complete and `Indexing keyframes` active

#### Scenario: Step fails
- **WHEN** a non-fatal step such as thumbnail generation fails
- **THEN** the remaining steps still complete, the editor enters the ready state, and the degraded state is reported as today

### Requirement: Progressive thumbnails
The system SHALL deliver each generated thumbnail to the editor as soon as it is written, and the timeline strip SHALL fill from left to right while the remaining cells show placeholders.

#### Scenario: Thumbnails arrive
- **WHEN** the fifth of fourteen thumbnails is written
- **THEN** the strip shows five images and nine placeholders

#### Scenario: Load replaced mid-inspection
- **WHEN** the user opens another file before inspection completes
- **THEN** thumbnails from the previous file are discarded and never appear in the new strip
