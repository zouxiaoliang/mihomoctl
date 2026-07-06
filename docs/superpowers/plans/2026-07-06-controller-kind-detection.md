# Controller Kind Detection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement automatic Clash/Mihomo controller kind detection, require user confirmation before saving the kind, and use the stored kind to filter and guard API operations.

**Architecture:** Store `ControllerKind` on `Server` with a serde default of `Mihomo`. Detect kind during `server add` by probing `/version`, use the result as the default confirmation choice, and expose kind-aware TUI API catalogs plus runtime guards so Clash servers do not call Mihomo-only endpoints.

**Tech Stack:** Rust workspace, `serde`/RON config, `clap`, `requestty`, `ureq`, existing `mihomoctl-core::Clash` HTTP client, cargo tests.

---

### Task 1: Server Controller Kind And Detection

**Files:**
- Modify: `mihomoctl/src/interactive/config.rs`
- Modify: `mihomoctl/src/command/server.rs`
- Test: `mihomoctl/src/interactive/config.rs`

- [ ] **Step 1: Write failing tests for server kind config and detection**

Add tests near the existing `test_config` in `mihomoctl/src/interactive/config.rs`:

```rust
#[test]
fn old_server_config_defaults_to_mihomo_kind() {
    let server: Server = ron::from_str(
        r#"(
            url: "http://127.0.0.1:9090/",
            secret: None,
        )"#,
    )
    .unwrap();

    assert_eq!(server.kind, ControllerKind::Mihomo);
}

#[test]
fn new_server_config_serializes_kind() {
    let server = Server {
        url: url::Url::parse("http://127.0.0.1:9090/").unwrap(),
        secret: Some("token".to_owned()),
        kind: ControllerKind::Clash,
    };

    let serialized = ron::to_string(&server).unwrap();
    assert!(serialized.contains("kind"));
    assert!(serialized.contains("clash"));
}

#[test]
fn detect_controller_kind_from_version_payloads() {
    assert_eq!(
        ControllerKind::detect_from_version_body(r#"{"version":"Mihomo v1.18.0"}"#),
        Some(ControllerKind::Mihomo)
    );
    assert_eq!(
        ControllerKind::detect_from_version_body(r#"{"version":"1.18.0","meta":true}"#),
        Some(ControllerKind::Mihomo)
    );
    assert_eq!(
        ControllerKind::detect_from_version_body(r#"{"version":"1.18.0"}"#),
        Some(ControllerKind::Clash)
    );
    assert_eq!(ControllerKind::detect_from_version_body("not json"), None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `rtk cargo test -p mihomoctl interactive::config::`

Expected: FAIL because `ControllerKind` and `Server.kind` do not exist.

- [ ] **Step 3: Implement minimal config model**

Add `ControllerKind`, serde helpers, `Server.kind`, and detection helpers in `mihomoctl/src/interactive/config.rs`. Keep `Server::into_clash_builder` unchanged except for carrying the new field.

Add `detect_controller_kind(url, secret, timeout)` that builds a temporary `Clash`, calls raw `version`, and maps unrecognized errors to `None`.

- [ ] **Step 4: Run test to verify it passes**

Run: `rtk cargo test -p mihomoctl interactive::config::`

Expected: PASS.

- [ ] **Step 5: Wire detection into server add/list**

In `mihomoctl/src/command/server.rs`, after reading URL and secret:

- Call `detect_controller_kind(&url, secret.as_deref(), Duration::from_millis(flags.timeout))`.
- If `Some(kind)`, prompt a `select` question with `mihomo` and `clash`, defaulting to the detected kind.
- If `None`, prompt a `select` question with `mihomo` and `clash`, defaulting to `mihomo`.
- Store `Server { url, secret, kind }`.
- Show kind in the server list table.

### Task 2: Kind-Aware API Catalog And Invocation Guard

**Files:**
- Modify: `mihomoctl/src/ui/api.rs`
- Modify: `mihomoctl/src/ui/state.rs`
- Test: `mihomoctl/src/ui/api.rs`

- [ ] **Step 1: Write failing tests for API support filtering**

Add tests in the existing `#[cfg(test)]` module in `mihomoctl/src/ui/api.rs`:

```rust
#[test]
fn clash_api_catalog_excludes_mihomo_only_operations() {
    let clash = api_state_for_kind(ControllerKind::Clash)
        .iter()
        .map(|item| item.operation)
        .collect::<Vec<_>>();

    for operation in [
        ApiOperation::Memory,
        ApiOperation::MemoryWs,
        ApiOperation::FlushFakeIpCache,
        ApiOperation::GetGroups,
        ApiOperation::Restart,
        ApiOperation::DisableRules,
        ApiOperation::GetStorage,
        ApiOperation::DebugPprof,
        ApiOperation::ConnectionsWs,
    ] {
        assert!(!clash.contains(&operation), "{operation:?} should be hidden");
    }

    assert!(clash.contains(&ApiOperation::Version));
    assert!(clash.contains(&ApiOperation::GetProxies));
    assert!(clash.contains(&ApiOperation::DnsQuery));
}

#[test]
fn mihomo_api_catalog_keeps_full_catalog() {
    let mihomo = api_state_for_kind(ControllerKind::Mihomo)
        .iter()
        .map(|item| item.operation)
        .collect::<Vec<_>>();
    let full = default_api_state()
        .iter()
        .map(|item| item.operation)
        .collect::<Vec<_>>();

    assert_eq!(mihomo, full);
}

#[test]
fn unsupported_mihomo_operation_is_guarded_before_request() {
    let clash = Clash::builder("http://127.0.0.1:1").unwrap().build();
    let result = ApiOperation::Memory.invoke_for_kind(
        &ApiParams::default(),
        &clash,
        ControllerKind::Clash,
        "http://example.com",
        2000,
    );

    assert!(result.contains("requires mihomo controller"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `rtk cargo test -p mihomoctl ui::api::`

Expected: FAIL because kind-aware catalog helpers and invocation guard do not exist.

- [ ] **Step 3: Implement API support model**

Import `ControllerKind` into `mihomoctl/src/ui/api.rs`.

Add `ApiOperation::supports(self, kind: ControllerKind) -> bool`, treating the documented Clash-compatible operations as shared and everything else as Mihomo-only.

Add `ApiItem::catalog_for_kind(kind)`, `api_state_for_kind(kind)`, `config_core_api_state_for_kind(kind)`, and `dns_api_state_for_kind(kind)`. Keep existing `default_api_state`, `config_core_api_state`, and `dns_api_state` as Mihomo defaults for backward tests.

Add `ApiOperation::invoke_for_kind(...)` that returns `operation requires mihomo controller` before calling `invoke` when unsupported.

- [ ] **Step 4: Run test to verify it passes**

Run: `rtk cargo test -p mihomoctl ui::api::`

Expected: PASS.

### Task 3: TUI Uses Active Server Kind

**Files:**
- Modify: `mihomoctl/src/ui/app.rs`
- Modify: `mihomoctl/src/ui/state.rs`
- Modify: `mihomoctl/src/ui/servo.rs`
- Test: `mihomoctl/src/ui/state.rs` or `mihomoctl/src/ui/api.rs`

- [ ] **Step 1: Write failing tests for TUI state initialization**

Add a test showing Clash state does not include Mihomo-only API operations and Mihomo state still does. Use the existing state tests as the local pattern.

- [ ] **Step 2: Run test to verify it fails**

Run the focused state/API test with `rtk cargo test -p mihomoctl ui::state::` or `rtk cargo test -p mihomoctl ui::api::`, depending on where the test lands.

Expected: FAIL because TUI state is always initialized with default Mihomo catalogs.

- [ ] **Step 3: Implement TUI kind propagation**

In `ui/app.rs`, read the active server kind before calling `init_config`, then initialize `TuiStates` with that kind instead of `TuiStates::default()`.

In `ui/state.rs`, add a constructor such as `TuiStates::for_controller_kind(kind)` and use kind-aware API state builders for the Core, DNS, and full API lists.

In `ui/servo.rs`, pass the active kind into action handling and call `operation.invoke_for_kind(...)`. Skip `memory_job` when the active server is `ControllerKind::Clash`.

- [ ] **Step 4: Run focused tests**

Run: `rtk cargo test -p mihomoctl ui::api`

Expected: PASS.

### Task 4: Verification And Cleanup

**Files:**
- Modify as needed from earlier tasks.

- [ ] **Step 1: Format**

Run: `rtk cargo fmt --all`

Expected: no formatting errors.

- [ ] **Step 2: Focused tests**

Run: `rtk cargo test -p mihomoctl`

Expected: PASS.

- [ ] **Step 3: Workspace tests**

Run: `rtk cargo test --workspace`

Expected: PASS.

- [ ] **Step 4: Requirement audit**

Check the implementation against every requirement in `docs/superpowers/specs/2026-07-06-controller-kind-detection-design.md`:

- Server stores kind with old config default.
- `server add` detects via `/version`, then always lets the user confirm or correct the kind before saving.
- `server list` displays kind.
- Clash catalog hides Mihomo-only operations.
- Mihomo catalog remains full.
- Unsupported operation guard returns a local message before HTTP.
- Clash TUI does not fail on `/memory`.
