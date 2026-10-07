# acceleration-observability Specification

## Purpose
Define how playback and export acceleration are detected, classified, surfaced, and logged.

## Requirements

### Requirement: Separate acceleration dimensions
The system SHALL track playback decoding, playback rendering, export decoding, and export encoding as separate acceleration dimensions and MUST distinguish capability availability from actual active use.

#### Scenario: Hardware is compiled but not active
- **WHEN** FFmpeg lists VA-API support but device or codec initialization has not succeeded
- **THEN** the system reports hardware as available or unverified rather than active

#### Scenario: Export path starts
- **WHEN** an FFmpeg export path successfully initializes
- **THEN** the system records the actual decoder, encoder, API, DRM render node, and hardware/software classification for that attempt

### Requirement: Visible acceleration status
The system SHALL show a compact, non-blocking acceleration indicator in the editor header as a status dot followed by text (`Hardware acceleration active`, `Software encoding`, or `Hardware acceleration unknown`) and SHALL provide details for playback and export through accessible text or a tooltip.

#### Scenario: Hardware playback is detected
- **WHEN** the active GStreamer pipeline selects a recognized hardware decoder or DMA-BUF rendering path
- **THEN** the UI identifies the detected playback dimensions as GPU accelerated

#### Scenario: Software export fallback is selected
- **WHEN** the application falls back to libx264
- **THEN** the UI visibly reports software export without preventing the trim

#### Scenario: Playback acceleration cannot be proven
- **WHEN** the application cannot reliably identify WebKitGTK's selected decoder or rendering path
- **THEN** the UI reports playback acceleration as unknown and MUST NOT label it hardware or software

### Requirement: Concise normal logs
The system SHALL emit concise structured logs by default containing the Wayland backend, detected GPU API/device information, selected playback decoder when observable, each export acceleration attempt, fallback reason, result, elapsed time, and output path.

#### Scenario: Hardware export succeeds
- **WHEN** a VA-API export completes
- **THEN** normal logs identify the active hardware decode/encode dimensions and device without requiring verbose mode

#### Scenario: Hardware attempt fails
- **WHEN** a VA-API path fails and another path is attempted
- **THEN** normal logs contain a warning with the failed stage and a summary reason followed by the selected fallback

### Requirement: Verbose media diagnostics
The system SHALL make `--verbose` retain detailed application, FFmpeg stderr, and WebKitGTK/GStreamer media diagnostics under the XDG state directory while keeping application stdout clean. Without `--verbose`, the system SHALL still retain a low-volume, per-instance GStreamer trace that records which media elements the pipeline creates, so that playback decoder detection works in normal runs.

#### Scenario: Verbose mode enabled
- **WHEN** the application starts with `--verbose`
- **THEN** it configures media diagnostics before WebKitGTK initializes and records detailed logs in the wodeo state directory

#### Scenario: Verbose mode disabled
- **WHEN** the application starts normally
- **THEN** it writes concise application logs and a low-volume element-creation trace, without retaining high-volume raw media traces

#### Scenario: Two instances run at once
- **WHEN** two instances of the application play different videos at the same time
- **THEN** each instance reports the decoder selected by its own pipeline

### Requirement: Playback decoder classification
The system SHALL classify known GStreamer hardware and software decoder factories from WebKitGTK media diagnostics in both normal and verbose mode, SHALL report the decoder the pipeline created most recently, and SHALL preserve the original decoder name in logs.

#### Scenario: Known decoder selected
- **WHEN** diagnostics show a recognized decoder factory such as a VA-API hardware decoder or FFmpeg software decoder
- **THEN** the application reports the factory name and its hardware/software classification, and the normal application log records the selection once

#### Scenario: Decoder fallback during pipeline setup
- **WHEN** the pipeline creates a hardware decoder, rejects it, and then creates a software decoder
- **THEN** the application reports the software decoder

#### Scenario: Unknown decoder selected
- **WHEN** diagnostics expose a decoder factory that is not in the classification table
- **THEN** the application logs its name, reports acceleration as unknown, and continues playback

### Requirement: Log destinations and stream isolation
The system SHALL write application logs beneath `$XDG_STATE_HOME/wodeo` or the platform-equivalent fallback and MUST direct all application, FFmpeg, and GStreamer diagnostics away from stdout.

#### Scenario: State home is not explicitly set
- **WHEN** `XDG_STATE_HOME` is absent
- **THEN** the system uses the standard per-user state-directory fallback and reports the resolved diagnostic path on stderr when relevant
