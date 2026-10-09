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
The system SHALL derive a preview proxy from the selected video by re-encoding it into an MP4 with dense keyframes, no B-frames, a leading moov atom, and fresh timing metadata, and SHALL serve that proxy at the loopback media endpoint. The proxy SHALL be scaled so that its shorter side is at most 1080 pixels, preserving the aspect ratio and orientation, SHALL never be upscaled, and SHALL have even dimensions. The proxy SHALL preserve the source timestamps so that trim positions chosen against the preview remain valid, and export SHALL continue to read the original file at its original resolution.

#### Scenario: Proxy generation succeeds
- **WHEN** a valid supported video finishes probing and FFmpeg builds the proxy
- **THEN** the system plays the proxy in place of the raw file so that seeking lands on a nearby keyframe without flushing the displayed frame

#### Scenario: Proxy generation fails
- **WHEN** FFmpeg cannot build the proxy for a probed video
- **THEN** the system serves the original file at the preview endpoint and records the degraded state

#### Scenario: Large landscape source is capped
- **WHEN** the source video is 3840×2160
- **THEN** the proxy is 1920×1080 and its frame timestamps match the source

#### Scenario: Large portrait source is capped on its shorter side
- **WHEN** the source video is 1080×1920 or 1440×2560
- **THEN** the proxy's width is at most 1080 pixels, its height keeps the source aspect ratio, and both dimensions are even

#### Scenario: Small source is not upscaled
- **WHEN** the source video's shorter side is 1080 pixels or less
- **THEN** the proxy keeps the source dimensions, rounded down to even values

#### Scenario: Export ignores the proxy
- **WHEN** the user exports a range from a video whose proxy was downscaled
- **THEN** the export reads the original file and its output resolution follows the selected quality, not the proxy's resolution

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
The system SHALL report inspection as an ordered sequence of named steps (`Reading container`, `Indexing keyframes`, `Building preview`) with a completion fraction, and the editor SHALL show completed, active, and pending steps with a progress bar while inspecting. Thumbnail generation SHALL NOT be an inspection step that gates the ready state; it SHALL run alongside proxy generation and continue after the editor becomes ready.

#### Scenario: Step advances
- **WHEN** the container probe finishes and keyframe indexing starts
- **THEN** the editor marks `Reading container` complete and `Indexing keyframes` active

#### Scenario: Preview finishes before thumbnails
- **WHEN** the preview proxy is ready while thumbnails are still being generated
- **THEN** inspection completes, the editor enters the ready state, and the thumbnails keep arriving in the timeline

#### Scenario: Step fails
- **WHEN** a non-fatal step such as proxy generation fails, or thumbnail generation fails
- **THEN** the remaining steps still complete, the editor enters the ready state, and the degraded state is reported as today

### Requirement: Progressive thumbnails
The system SHALL generate thumbnails with at most four concurrent FFmpeg processes, SHALL deliver each generated thumbnail to the editor, tagged with its load id and index, as soon as it is written, both before and after the editor becomes ready, and the timeline strip SHALL show each thumbnail in its cell while the remaining cells show placeholders. When generation finishes, the system SHALL send one completion signal for the load carrying the final ordered thumbnail list and any thumbnail warning, and SHALL make that list available to the loopback thumbnail endpoint.

#### Scenario: Thumbnails arrive
- **WHEN** five of fourteen thumbnails are written
- **THEN** the strip shows those five images in their cells and nine placeholders

#### Scenario: Thumbnails arrive after the editor is ready
- **WHEN** the editor is ready and a further thumbnail for the current load is written
- **THEN** the strip shows it without interrupting playback, seeking, or the trim selection

#### Scenario: Thumbnail generation completes
- **WHEN** the last thumbnail for the current load is written
- **THEN** the editor receives the completion signal with the full ordered list, and the loopback thumbnail endpoint serves every listed thumbnail

#### Scenario: Thumbnail generation fails after ready
- **WHEN** thumbnail generation for the current load fails after the editor is ready
- **THEN** the completion signal carries the warning, the timeline falls back to the non-thumbnail strip, and playback and trimming are unaffected

#### Scenario: Load replaced mid-inspection
- **WHEN** the user opens another file before inspection or thumbnail generation for the previous file completes
- **THEN** the previous file's thumbnail jobs are cancelled or their results are ignored, their thumbnails and completion signal never appear in the new strip, and the previous cache directory is removed only after its jobs have stopped writing to it

### Requirement: Preview revealed at first frame
The system SHALL keep the preview video element in the layout but visually hidden from the moment it is mounted until its first frame is available (the media element's `loadeddata` event), so that no default-sized placeholder box or empty black frame is shown. The element MUST remain in the layout while hidden so that loading and decoding are not throttled. Revealing the video SHALL NOT change when playback and trim controls become enabled, which remains tied to the media's metadata being available.

#### Scenario: Video loads normally
- **WHEN** a probed video's preview URL is assigned to the video element
- **THEN** the element stays invisible until its first frame is decoded, then appears at its final size showing that frame

#### Scenario: No placeholder box
- **WHEN** the video element is mounted before its metadata has loaded
- **THEN** no black or empty rectangle is visible in the stage

#### Scenario: Preview fails
- **WHEN** the video element reports an error before its first frame is available
- **THEN** the existing preview failure handling applies and no blank video box is left visible

#### Scenario: Replace a loaded video
- **WHEN** a different video replaces the loaded one
- **THEN** the new video element is again hidden until its own first frame is available

### Requirement: Per-load preview URL
The system SHALL give every load a distinct preview URL by adding the load id as a query parameter to the loopback media endpoint (`/media?load=<id>`), and the loopback server SHALL route requests by path while ignoring the query string. The editor SHALL treat a new preview URL as a new media source.

#### Scenario: Replacing a video changes the preview URL
- **WHEN** the user loads one video and then replaces it with another
- **THEN** the second load's preview URL differs from the first, the video element is remounted for the new source, and any queued or in-flight seek from the previous video is discarded

#### Scenario: Query string is ignored for routing
- **WHEN** a client requests `/media?load=7` while a file is loaded
- **THEN** the server serves the current preview media exactly as for `/media`

#### Scenario: Unknown path with a query
- **WHEN** a client requests `/other?load=7`
- **THEN** the server responds 404 and serves no content
