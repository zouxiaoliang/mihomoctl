# Release Assets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update the release workflow so each tag release uploads six target-triple archives plus matching `.sha256` checksum files and generated release notes.

**Architecture:** Keep release logic in `.github/workflows/release.yaml`. Use one matrix entry per target triple, build with `cargo build --target`, package the single binary into `dist/`, generate a checksum beside it, and let `softprops/action-gh-release@v3` upload all assets with generated notes.

**Tech Stack:** GitHub Actions YAML, Rust nightly toolchain, Cargo target builds, Bash, PowerShell, `tar`, `zip`/`Compress-Archive`, `sha256sum`/`shasum`, `softprops/action-gh-release@v3`.

---

### Task 1: Replace Native Runner Matrix With Target Matrix

**Files:**
- Modify: `.github/workflows/release.yaml`

- [ ] **Step 1: Replace the matrix**

In `.github/workflows/release.yaml`, replace the current `strategy.matrix.os` list with explicit target entries:

```yaml
    strategy:
      fail-fast: false
      matrix:
        include:
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
            archive: tar.gz
            binary: mihomoctl
          - target: aarch64-unknown-linux-musl
            os: ubuntu-latest
            archive: tar.gz
            binary: mihomoctl
          - target: x86_64-apple-darwin
            os: macos-latest
            archive: tar.gz
            binary: mihomoctl
          - target: aarch64-apple-darwin
            os: macos-latest
            archive: tar.gz
            binary: mihomoctl
          - target: x86_64-pc-windows-msvc
            os: windows-latest
            archive: zip
            binary: mihomoctl.exe
          - target: aarch64-pc-windows-msvc
            os: windows-latest
            archive: zip
            binary: mihomoctl.exe
```

- [ ] **Step 2: Update runner selection**

Keep this line directly under `strategy`:

```yaml
    runs-on: ${{ matrix.os }}
```

- [ ] **Step 3: Parse YAML**

Run:

```bash
rtk ruby -e "require 'psych'; Psych.load_file('.github/workflows/release.yaml'); puts 'release yaml ok'"
```

Expected output:

```text
release yaml ok
```

### Task 2: Build The Matrix Target

**Files:**
- Modify: `.github/workflows/release.yaml`

- [ ] **Step 1: Install the requested Rust target**

After `dtolnay/rust-toolchain@nightly`, add:

```yaml
      - name: Install target
        run: rustup target add ${{ matrix.target }}
```

- [ ] **Step 2: Build for the requested target**

Replace the current cargo build step with:

```yaml
      - name: Run cargo build
        run: cargo build --all-features --release --locked --target ${{ matrix.target }}
```

- [ ] **Step 3: Keep release permissions unchanged**

Confirm this block still appears near the top:

```yaml
permissions:
  contents: write
```

- [ ] **Step 4: Local native build smoke test**

Run:

```bash
rtk cargo build --all-features --release --locked
```

Expected: exit code `0`; warnings are acceptable if they match the existing lifetime warnings.

### Task 3: Package Archives And Checksums

**Files:**
- Modify: `.github/workflows/release.yaml`

- [ ] **Step 1: Remove old rename step**

Delete the old step:

```yaml
      - name: Rename build artifacts
        shell: bash
        run: |
          pushd target/release
          rm mihomoctl*.d
          mv mihomoctl* mihomoctl-${{ runner.os }}
          popd
```

- [ ] **Step 2: Add Unix packaging step**

Add this step after the build step:

```yaml
      - name: Package Unix asset
        if: runner.os != 'Windows'
        shell: bash
        run: |
          set -euo pipefail
          tag="${GITHUB_REF_NAME}"
          target="${{ matrix.target }}"
          archive="mihomoctl-${tag}-${target}.tar.gz"
          bin="target/${target}/release/${{ matrix.binary }}"
          test -f "${bin}"
          mkdir -p dist package
          cp "${bin}" package/mihomoctl
          tar -C package -czf "dist/${archive}" mihomoctl
          if command -v sha256sum >/dev/null 2>&1; then
            (cd dist && sha256sum "${archive}" > "${archive}.sha256")
          else
            (cd dist && shasum -a 256 "${archive}" > "${archive}.sha256")
          fi
```

- [ ] **Step 3: Add Windows packaging step**

Add this step after the Unix packaging step:

```yaml
      - name: Package Windows asset
        if: runner.os == 'Windows'
        shell: pwsh
        run: |
          $tag = $env:GITHUB_REF_NAME
          $target = "${{ matrix.target }}"
          $archive = "mihomoctl-$tag-$target.zip"
          $bin = "target/$target/release/${{ matrix.binary }}"
          if (!(Test-Path $bin)) {
            throw "missing binary: $bin"
          }
          New-Item -ItemType Directory -Force -Path dist, package | Out-Null
          Copy-Item $bin package/mihomoctl.exe
          Compress-Archive -Path package/mihomoctl.exe -DestinationPath "dist/$archive" -Force
          $hash = (Get-FileHash "dist/$archive" -Algorithm SHA256).Hash.ToLower()
          "$hash  $archive" | Out-File -FilePath "dist/$archive.sha256" -Encoding ascii -NoNewline
```

- [ ] **Step 4: Validate local packaging syntax**

Run:

```bash
rtk ruby -e "require 'psych'; Psych.load_file('.github/workflows/release.yaml'); puts 'release yaml ok'"
```

Expected output:

```text
release yaml ok
```

### Task 4: Upload Generated Notes And Dist Assets

**Files:**
- Modify: `.github/workflows/release.yaml`

- [ ] **Step 1: Update upload path**

Change release upload from:

```yaml
          files: target/release/mihomoctl*
```

to:

```yaml
          files: dist/*
```

- [ ] **Step 2: Enable generated release notes**

Ensure the release step contains:

```yaml
          generate_release_notes: true
```

The full release step should be:

```yaml
      - name: Release
        uses: softprops/action-gh-release@v3
        with:
          files: dist/*
          fail_on_unmatched_files: true
          generate_release_notes: true
          token: ${{ github.token }}
```

- [ ] **Step 3: Parse YAML**

Run:

```bash
rtk ruby -e "require 'psych'; Psych.load_file('.github/workflows/release.yaml'); puts 'release yaml ok'"
```

Expected output:

```text
release yaml ok
```

### Task 5: Verify, Commit, And Move Tag

**Files:**
- Modify: `.github/workflows/release.yaml`

- [ ] **Step 1: Verify action refs still exist**

Run:

```bash
rtk git ls-remote --tags https://github.com/actions/checkout.git refs/tags/v5
rtk git ls-remote --heads https://github.com/dtolnay/rust-toolchain.git refs/heads/nightly
rtk git ls-remote --tags https://github.com/softprops/action-gh-release.git refs/tags/v3
```

Expected: each command prints one ref.

- [ ] **Step 2: Verify native release build**

Run:

```bash
rtk cargo build --all-features --release --locked
```

Expected: exit code `0`.

- [ ] **Step 3: Inspect workflow diff**

Run:

```bash
rtk git diff -- .github/workflows/release.yaml
```

Expected: diff shows six target matrix entries, target install/build, Unix and Windows packaging, `dist/*`, and `generate_release_notes: true`.

- [ ] **Step 4: Commit workflow**

Run:

```bash
rtk git add .github/workflows/release.yaml
rtk git commit -m "ci: package multi-target release assets"
```

Expected: commit succeeds.

- [ ] **Step 5: Move local release tag**

Run:

```bash
rtk git tag -d v0.1.0
rtk git tag v0.1.0 HEAD
rtk git rev-parse --short HEAD
rtk git rev-parse --short v0.1.0
```

Expected: both hashes match.

- [ ] **Step 6: Final push commands**

After review, push with:

```bash
rtk git push origin main
rtk git push origin --delete v0.1.0
rtk git push origin v0.1.0
```

Expected: tag push triggers release and uploads twelve files: six archives plus six `.sha256` files.
