# nix-packaging Specification

## Purpose

Define how the Nix flake packages wodeo, including optional zsh completion installation and the JavaScript lockfile policy.

## Requirements

### Requirement: Package defined in nix/package.nix
The flake SHALL build `wodeo` from a callPackage-style derivation in `nix/package.nix` and SHALL expose it as `packages.x86_64-linux.default` and as the program of `apps.x86_64-linux.default`.

#### Scenario: Build the default package
- **WHEN** a user runs `nix build` in the repository
- **THEN** the result contains `bin/wodeo`, wrapped with `GDK_BACKEND=wayland`, FFmpeg on `PATH`, and the GStreamer plugin path

#### Scenario: Package is overridable
- **WHEN** a user evaluates `wodeo.packages.x86_64-linux.default.override { withShellCompletions = false; }`
- **THEN** Nix builds the package with that argument applied

### Requirement: Optional zsh completion installation
`nix/package.nix` SHALL accept `withShellCompletions ? true`. When true, the package MUST install `src-tauri/completions/_wodeo` as `share/zsh/site-functions/_wodeo` without executing the built binary. When false, the package MUST NOT install any shell completion files.

#### Scenario: Completions enabled by default
- **WHEN** the package is built without overrides
- **THEN** `$out/share/zsh/site-functions/_wodeo` exists and is identical to `src-tauri/completions/_wodeo`

#### Scenario: Completions disabled
- **WHEN** the package is built with `withShellCompletions = false`
- **THEN** `$out/share/zsh` does not exist

### Requirement: Nix build skips the Rust test phase
The package SHALL set `doCheck = false` because the test suite needs a Wayland session and media tooling unavailable in the build sandbox.

#### Scenario: Build in sandbox
- **WHEN** the package builds in the Nix sandbox without a Wayland display
- **THEN** the build succeeds without running `cargo test`

### Requirement: npm is the only JavaScript package manager
The repository SHALL track only `package-lock.json` for JavaScript dependencies and MUST NOT contain `bun.lock` or `bun.nix`.

#### Scenario: Lockfiles in repository
- **WHEN** the repository's tracked files are listed
- **THEN** `package-lock.json` is present and neither `bun.lock` nor `bun.nix` is present
