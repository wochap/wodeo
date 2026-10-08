# exact-video-export Specification

## Purpose
Define exact, responsive, and safely finalized MP4 exports with layered hardware acceleration.

## Requirements

### Requirement: Exact MP4 export
The system SHALL, for every format other than `copy`, transcode the selected interval so that the media starts at timestamp zero and the visible boundaries match the selected source interval to within one source frame. For `mp4` the output is H.264 video and AAC audio.

#### Scenario: Export a selected interval
- **WHEN** the user activates `Trim` with a valid selection and a re-encoding format
- **THEN** FFmpeg accurately seeks, decodes, and re-encodes the selected interval rather than performing keyframe-limited stream copying

#### Scenario: Input has no audio
- **WHEN** the selected MP4 contains no audio stream
- **THEN** the system exports a valid video-only file in the chosen format without treating absent audio as an error

### Requirement: Layered hardware acceleration
The system SHALL, for `mp4` exports, attempt a full VA-API decode/encode path first, then software decode with VA-API encode, then software decode with libx264, and SHALL use the first path that initializes and completes successfully. `webm`, `gif`, and `copy` exports SHALL use a single software path and report export acceleration as software (or not applicable for `copy`).

#### Scenario: Full VA-API succeeds
- **WHEN** the format is `mp4` and the source decoder, DRM render node, VA-API driver, and H.264 VA-API encoder initialize successfully
- **THEN** the system performs hardware decode and hardware encode and reports both as active

#### Scenario: Hardware decode is unavailable
- **WHEN** VA-API encoding initializes but hardware decoding is unsupported for the source
- **THEN** the system retries with software decode and VA-API encode and reports their distinct states

#### Scenario: Hardware encode is unavailable
- **WHEN** no VA-API export path initializes or a hardware attempt fails
- **THEN** the system removes the failed temporary output, retries with libx264, and reports software fallback

#### Scenario: Non-MP4 format
- **WHEN** the format is `webm`, `gif`, or `copy`
- **THEN** the system runs exactly one FFmpeg attempt without VA-API options

### Requirement: Export progress and responsiveness
The system SHALL parse machine-readable FFmpeg progress, update the UI during export with the processed time, percent complete, bytes written, an estimated final size, an estimated time remaining, and the active pipeline step, and keep the application responsive enough to request cancellation.

#### Scenario: Export advances
- **WHEN** FFmpeg reports processed output time
- **THEN** the system displays progress relative to the selected duration, the bytes written so far, and a time-remaining estimate, without writing progress to application stdout

#### Scenario: Export fails
- **WHEN** all configured export paths fail
- **THEN** the system retains the editor and selection, removes temporary output, and displays a concise error with diagnostic log location

#### Scenario: Copy size estimate
- **WHEN** the format is `copy`
- **THEN** the estimated size equals the source bitrate multiplied by the effective duration and is shown before the first progress event

#### Scenario: Re-encode size estimate
- **WHEN** the format re-encodes
- **THEN** the estimated size is marked approximate and derived from the tier's nominal bitrate

#### Scenario: Pipeline steps
- **WHEN** the export moves from encoding to validation to finalization
- **THEN** the dialog marks the previous step complete and the next step active

### Requirement: Safe destination handling
The system MUST render to a uniquely named temporary file in the destination directory and SHALL finalize the destination only after FFmpeg exits successfully and the temporary output passes validation. The destination extension MUST match the effective format (`.mp4` for `mp4`, `.webm`, `.gif`, and for `copy` the source file's extension, compared case-insensitively). The system MUST refuse a destination that resolves to the source file.

#### Scenario: Destination does not exist
- **WHEN** a valid export completes
- **THEN** the system atomically renames the validated temporary file to the requested destination where supported

#### Scenario: Destination exists without overwrite permission
- **WHEN** the requested destination already exists and is not the source file
- **THEN** the system starts the export without asking for confirmation, because the destination was visible and editable before trimming

#### Scenario: Authorized replacement
- **WHEN** an export to an existing destination succeeds
- **THEN** the system safely replaces the destination by renaming the validated temporary file, without ever running FFmpeg directly against the source or final destination

#### Scenario: Destination is the source
- **WHEN** the requested destination resolves to the loaded source file
- **THEN** the system does not start export and reports that source and destination are the same file

#### Scenario: Extension mismatch
- **WHEN** the destination extension does not match the effective format
- **THEN** the system does not start export and reports the required extension

#### Scenario: Copy destination extension
- **WHEN** the format is `copy`, the source is `in.mkv`, and the destination is `out.mp4`
- **THEN** the system does not start export and reports that `.mkv` is required

#### Scenario: Export is cancelled
- **WHEN** the user confirms cancellation during export
- **THEN** the system terminates FFmpeg, removes the temporary file, preserves any pre-existing destination, and exits without printing a path

### Requirement: Successful completion
The system SHALL validate that the completed output is readable and contains a video stream in the chosen format, report its canonical absolute path, and then either close the application (on-done `exit`) or return to the editor (on-done `stay`).

#### Scenario: Export completes successfully
- **WHEN** FFmpeg exits successfully, output validation passes, and on-done is `exit`
- **THEN** the system logs completion, prints the canonical absolute destination path to application stdout, and exits successfully

### Requirement: Output format selection
The system SHALL export the selected interval in one of four formats chosen by the user: `mp4` (H.264 video, AAC audio), `webm` (VP9 video, Opus audio), `gif` (animated GIF, no audio), or `copy` (stream copy of the source video and audio into the source file's container without re-encoding). The system SHALL request MP4 fast-start layout only when the output container is ISO-BMFF (`mp4`, `m4v`, `mov`).

#### Scenario: WebM export
- **WHEN** the format is `webm`
- **THEN** the output is a WebM file containing a VP9 video stream and, when the source has audio, an Opus audio stream, with boundaries exact to within one source frame

#### Scenario: GIF export
- **WHEN** the format is `gif`
- **THEN** the output is an animated GIF built with a generated palette, containing no audio, whose boundaries are exact to within one output frame

#### Scenario: Copy export
- **WHEN** the format is `copy`
- **THEN** the system copies the source streams without decoding, the output ends at the selected end within one source frame, and the output begins at the nearest keyframe at or before the selected start

#### Scenario: Copy keeps the source container
- **WHEN** the format is `copy` and the source is `clip.mkv`
- **THEN** the output is a Matroska file named with the `.mkv` extension and no fast-start option is passed to FFmpeg

### Requirement: Quality tiers
The system SHALL apply the selected quality tier to re-encoding formats as a resolution cap and encoder rate control, and SHALL ignore quality for `copy`. Tiers: `original` (source resolution), `high` (longest side capped at 1080 for video, width capped at 720 for GIF), `small` (longest side capped at 720 for video, width capped at 480 for GIF). GIF frame rates are 15, 12, and 10 frames per second for `original`, `high`, and `small`.

#### Scenario: Downscale on high
- **WHEN** a 3840x2160 source is exported as `mp4` with quality `high`
- **THEN** the output is 1920x1080 and aspect ratio is preserved

#### Scenario: Never upscale
- **WHEN** a 1280x720 source is exported with quality `high`
- **THEN** the output keeps 1280x720

#### Scenario: Quality with copy
- **WHEN** the format is `copy`
- **THEN** the quality value has no effect on the output

### Requirement: Effective start reporting for copy
The system SHALL report the effective start timestamp of a copy export, derived from the keyframe index, in the export result and in logs.

#### Scenario: Start moved to keyframe
- **WHEN** the selected start is 2.500 s and the preceding keyframe is at 2.190 s in copy mode
- **THEN** the export result reports an effective start of 2.190 s

### Requirement: Stay-open completion
The system SHALL, when the on-done policy is `stay`, return the editor to the ready state after a successful export, keep the loaded video and selection, and allow further exports.

#### Scenario: Export completes with stay policy
- **WHEN** an export succeeds and on-done is `stay`
- **THEN** the application prints the canonical output path to stdout, shows the saved path in the editor, and does not exit
