# Controller Kind Detection Design

## Goal

Support both Clash and Mihomo external controller APIs in one tool. When adding a server, `mihomoctl` should call `/version` to detect the controller kind automatically, then ask the user to confirm or correct the detected kind before saving it.

## Context

The project already exposes a broad Mihomo API surface through `mihomoctl-core/src/api.rs` and the TUI API catalog in `mihomoctl/src/ui/api.rs`. Clash support is currently implicit: the shared endpoints work because Mihomo evolved from Clash, but the tool has no stored controller kind and therefore cannot hide or guard Mihomo-only endpoints for plain Clash servers.

The referenced Clash API includes the common external controller endpoints: logs, traffic, version, configs, proxies, rules, connections, proxy providers, and DNS query. The referenced Mihomo API keeps those common endpoints and adds endpoints such as memory, cache flushes, group APIs, restart and upgrade operations, websocket variants, rule providers, storage, and debug pprof.

## Architecture

Add a small controller-kind model to the CLI crate:

- `ControllerKind::Clash`
- `ControllerKind::Mihomo`

`Server` stores this kind next to `url` and `secret`. The serde default is `Mihomo` so existing `config.ron` files load without migration and keep the current full API behavior.

The HTTP client remains the existing `Clash` type in `mihomoctl-core`; this avoids a broad public API rename. Compatibility is enforced at the CLI/TUI layer by filtering available operations and by adding a guard before invoking API operations.

## Detection Flow

During `mihomoctl server add`:

1. Ask for URL and secret as today.
2. Build a temporary controller client with those values.
3. Call `GET /version`.
4. If the response shape or version text clearly identifies Mihomo, store `ControllerKind::Mihomo`.
5. If the response matches only Clash's common version shape and has no Mihomo marker, use `ControllerKind::Clash` as the suggested value.
6. If the request fails, authentication fails, the body is malformed, or the response is unknown, use `ControllerKind::Mihomo` as the fallback suggested value.
7. Always ask the user to confirm the controller kind with a `clash` or `mihomo` selection before saving. The detected or fallback value is the default choice, and the user can change it if the probe was wrong.

The confirmation prompt should be presented as a normal prompt, not as an error. A failed probe should not prevent adding a server because users may add offline or firewalled controllers.

## API Capability Model

Each `ApiOperation` gets a minimum required controller kind or a support predicate:

- Clash-supported operations are available for both `Clash` and `Mihomo`.
- Mihomo-only operations are available only for `Mihomo`.

The Clash-compatible set includes:

- `GET /logs`
- `GET /traffic`
- `GET /version`
- `GET`, `PUT`, and `PATCH /configs`
- `GET /proxies`
- `GET`, `PUT`, and delay test under `/proxies/:name`
- `GET /rules`
- `GET` and `DELETE /connections`
- `DELETE /connections/:id`
- `GET`, `PUT`, and healthcheck under `/providers/proxies`
- `GET /dns/query`

The Mihomo-only set includes current extended operations such as memory, websocket operations, cache flushes, `/group`, config geo update, restart, upgrade, rule disable, rule providers, storage, and debug.

## UI And CLI Behavior

`server list` shows the stored controller kind next to each URL.

The proxy CLI commands continue to work for both controller kinds because they only use shared proxy endpoints.

The TUI API catalog is built from the active server's controller kind:

- Clash mode shows only common Clash-compatible operations.
- Mihomo mode shows the full existing catalog.

If an unsupported operation is invoked programmatically or through stale state, the invoke path returns a clear message such as `operation requires mihomo controller` and does not send an HTTP request.

The main TUI status jobs should stay on shared endpoints where possible. If the active controller is Clash, jobs for Mihomo-only streams such as memory should be skipped or disabled so a Clash TUI session does not fail immediately because `/memory` is unavailable.

## Error Handling

Detection failures are non-fatal during `server add`; they only trigger manual selection.

Runtime unsupported-operation errors are explicit and local. They should not be confused with HTTP 404 responses from the controller, because the tool knows from configuration that the endpoint should not be called.

Existing HTTP and parsing errors remain unchanged for supported operations.

## Testing

Use test-first implementation.

Add tests for:

- Deserializing an old `Server` value without `kind` defaults to `Mihomo`.
- Serializing a new `Server` includes the selected kind.
- Version detection identifies a Mihomo response.
- Version detection identifies a Clash response.
- Version detection returns an unknown result on request or parse failure so the caller can present a manual confirmation with a fallback default.
- The detected controller kind is used as the default confirmation choice, but both `mihomo` and `clash` remain selectable.
- The Clash API catalog excludes Mihomo-only operations such as memory, cache, group, storage, debug, and websocket operations.
- The Mihomo API catalog still contains the full current catalog.
- Invoking a Mihomo-only operation while configured as Clash returns a local unsupported-operation message without making a request.

Run focused package tests first, then the workspace test suite if dependencies are already available.

## Out Of Scope

This change does not rename the public `Clash` client type, split crates, remove existing Mihomo wrappers, or add new CLI subcommands for every Mihomo endpoint. It only adds controller-kind awareness, detection, filtering, and guards.
