# Release Assets Design

## Goal

Make GitHub releases look like a polished multi-platform release: one compressed binary archive per supported target, one `.sha256` checksum per archive, generated release notes, and stable asset names that include the release tag and Rust target triple.

## Context

The current release workflow builds on three native runners and uploads broad runner-named binaries such as `mihomoctl-Linux`. That proves the project can release, but it does not give users target-specific downloads or checksums like the reference release page.

The workflow already has release write permission, modern Node-based actions, and a working `cargo build --all-features --release --locked` step. The next change should focus only on artifact shape and platform matrix.

## Supported Targets

The release should produce these six binary archives:

- `mihomoctl-${tag}-x86_64-unknown-linux-musl.tar.gz`
- `mihomoctl-${tag}-aarch64-unknown-linux-musl.tar.gz`
- `mihomoctl-${tag}-x86_64-apple-darwin.tar.gz`
- `mihomoctl-${tag}-aarch64-apple-darwin.tar.gz`
- `mihomoctl-${tag}-x86_64-pc-windows-msvc.zip`
- `mihomoctl-${tag}-aarch64-pc-windows-msvc.zip`

Each archive should have a matching checksum file named `${archive}.sha256`.

## Workflow Design

Use a target matrix with explicit fields:

- `target`: Rust target triple.
- `os`: GitHub runner image.
- `archive`: `tar.gz` for Linux and macOS, `zip` for Windows.
- `bin`: binary path suffix, with `.exe` for Windows.

Install the requested target with `rustup target add`. Build with:

```bash
cargo build --all-features --release --locked --target "$target"
```

Package only the final binary, not the full target directory. Put release assets in `dist/`.

Use `softprops/action-gh-release@v3` to upload `dist/*` and set `generate_release_notes: true` so the GitHub release includes the changelog section.

## Error Handling

The workflow should fail if an expected archive is missing. Checksum generation should run after packaging and before upload. The release upload should keep `fail_on_unmatched_files: true`.

If a target cannot be built on the selected runner, fail that matrix entry instead of uploading partial or mislabeled assets.

## Testing

Local verification:

- Parse `.github/workflows/release.yaml` as YAML.
- Run the native release build command locally.
- Run a local packaging smoke test for the current host target if practical.

CI verification:

- Trigger a tag release and confirm six archives plus six checksum files are uploaded.
- Confirm the release page has generated notes and the assets use tag plus target triple naming.

## Out Of Scope

This design does not add installers, Homebrew formula updates, code signing, notarization, SBOMs, or container images. It also does not add extra targets beyond the six approved target triples.
