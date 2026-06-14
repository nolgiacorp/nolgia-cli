# nolgia-client

Rust API client for `nolgia-api`, generated from `openapi.yaml` with Progenitor.

## What this crate does

- Re-exports the generated `Client`, `types`, `Error`, and `ResponseValue` types.
- Provides `ClientBuilder` for convenient base URL normalization and optional auth.
- Automatically targets the `/v1` API stem unless the caller already includes it.

## Usage

```rust
use nolgia_client::ClientBuilder;

let client = ClientBuilder::new("http://localhost:8080")
    .bearer_token("nol_test_token")
    .build()?;
```

## Spec version bumps

1. Update `../nolgia-api/api/openapi.yaml`.
2. Bump `spec_version` in `openapi-version.toml`.
3. Publish a matching `v<spec_version>` release in `nolgia-api` with `openapi.yaml` attached.
4. Rebuild this crate so `build.rs` can pull the new release asset in release builds.

## Local development

- Debug builds read the sibling spec file directly.
- Release builds fetch the release asset from GitHub unless `NOLGIA_OPENAPI_RELEASE_URL` is set.
