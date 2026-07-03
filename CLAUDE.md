See AGENTS.md for the full project knowledge base (structure, cross-repo flow, conventions).

Essentials for working in this repo:

- The API client crate (`crates/client`) is generated from `../nolgia-api/api/openapi.yaml` at build time (`crates/client/build.rs`). Never hand-edit generated shapes; change the spec in nolgia-api first, then rebuild. Spec changes that add required response fields (e.g. `JobPage.total`) break test fixtures here — update `crates/cli/tests/cli_commands.rs` mocks to match.
- The generated client appends `/v1` to the base URL. The API serves both `/` and `/v1`; the spec's canonical servers URL is `https://api.nolgia.ai/v1`.
- Auth: device-code login stores JWTs in the system keyring; `--token`/`NOLGIA_TOKEN` accepts a PAT (`nol_...`) or JWT and must be honored by every command, including `auth status`.
- Credit semantics the CLI must not misrepresent: device-login JWT requests spend subscription credits (`app_subscription`); PAT requests spend only prepaid API credits (`shared_topup`) and get `402 Payment Required` when that pool is empty, regardless of tier.
- To verify against staging: `NOLGIA_API_URL=https://api.stg.nolgia.ai` plus a staging token. `cargo test --workspace` runs offline against wiremock fixtures.
