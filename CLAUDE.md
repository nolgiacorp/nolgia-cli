See AGENTS.md for the full project knowledge base (structure, cross-repo flow, conventions).

Essentials for working in this repo:

- The API client crate (`crates/client`) is generated from `../nolgia-api/api/openapi.yaml` at build time (`crates/client/build.rs`). Never hand-edit generated shapes; change the spec in nolgia-api first, then rebuild. Spec changes that add required response fields (e.g. `JobPage.total`) break test fixtures here — update `crates/cli/tests/cli_commands.rs` mocks to match.
- The generated client appends `/v1` to the base URL. The API serves both `/` and `/v1`; the spec's canonical servers URL is `https://api.nolgia.ai/v1`.
- Auth: device-code login stores JWTs in the system keyring; `--token`/`NOLGIA_TOKEN` accepts a PAT (`nol_...`) or JWT and must be honored by every command, including `auth status`.
- Credit semantics the CLI must not misrepresent: device-login JWT requests spend subscription credits (`app_subscription`); PAT requests spend only prepaid API credits (`shared_topup`) and get `402 Payment Required` when that pool is empty, regardless of tier.
- To verify against staging: `NOLGIA_API_URL=https://api.stg.nolgia.ai` plus a staging token. `cargo test --workspace` runs offline against wiremock fixtures.
- Bundled agent skills live in `crates/cli/skills/*/SKILL.md`, embedded via `include_str!` in `crates/cli/src/commands/skills.rs` (must stay inside the crate dir or cargo publish drops them). Keep frontmatter `name:` equal to the directory name — a unit test enforces it.
- The nolgia-agent film pipeline drives this CLI as its only platform client (`--json` subprocess). Changing JSON output shapes or flag names of `gen video`, `wait`, `status`, `assets upload/get`, `models list` breaks `nolgia_pipeline/api.py` in nolgiacorp/nolgia-agent — update it in the same change.
- Release: bump `[workspace.package]` version + `crates/client` version + the workspace `nolgia-client` dep pin, add a `## vX.Y.Z` section to CHANGELOG.md (it becomes the release notes), tag `vX.Y.Z`, push. CI builds binaries, creates the GitHub release, publishes both crates. The Homebrew tap (`nolgiacorp/homebrew-nolgia`) is bumped MANUALLY: new version + sha256 of the darwin and linux binaries.
- Agent-facing conventions: costly commands carry agent-phrased help (estimate with `--cost-only`, confirm >~2k credits); requests self-identify via `X-Nolgia-Surface` (detection in `main.rs::detect_surface`, override `NOLGIA_SURFACE`).
