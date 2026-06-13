# Contributing

Thanks for your interest in improving AI Usage Bar! This is a small, focused
project — contributions that keep it lean and privacy-respecting are very
welcome.

## Ground rules

- **License**: by contributing you agree your work is licensed under
  [CC BY-NC-SA 4.0](LICENSE). Note the **non-commercial** clause.
- **No telemetry, ever.** The app must never phone home or add third-party
  network calls. The only outbound requests allowed are to the providers' own
  documented endpoints.
- **Credentials are read-only.** Don't add code paths that write credential
  files except the existing, intentional token-refresh write-back.

## Development setup

See [README → Build from source](README.md#build-from-source). In short:

```bash
pnpm install
pnpm tauri dev
```

## Before opening a PR

CI runs these — please run them locally first:

```bash
# Frontend
pnpm build                       # typecheck + bundle

# Rust (from src-tauri/)
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

For changes to provider parsing, validate against a real account:

```bash
cd src-tauri
cargo run --example live_fetch
```

## Adding a provider

1. Create `src-tauri/src/providers/<name>/` with `auth.rs`, `client.rs`,
   `normalize.rs`, and a `mod.rs` implementing the `Provider` trait.
2. Normalize into the shared `UsageSnapshot` — never leak provider-specific
   shapes past `normalize.rs`.
3. Add the provider to `ProviderId`, the selector, the tray, and the settings UI.
4. Add unit tests with a captured fixture response (no live calls in tests).
5. Document its credential paths and endpoints in the README table.

## Code style

- Rust: `rustfmt` defaults, `clippy` clean.
- TypeScript: keep the UI minimal and dependency-light.
- Match the surrounding code's naming and comment density. Comment the *why*,
  not the *what*.
