
## T35 CLI full command surface
- Added `crates/cli` as the workspace binary crate for the `nolgia` executable; root workspace now includes client + CLI and release profile strips symbols with thin LTO.
- Clap uses global `--json`, `--api-url`, and `--token`; `gen`, `status`, `wait`, `assets`, `account`, and `billing` dispatch through `crates/cli/src/commands/`.
- The generated `nolgia-client` exposes generation, jobs, assets-list, account `/me`, and billing calls, but the current API spec has no asset get/delete endpoint; CLI exposes placeholders that fail clearly for unavailable delete and only creates the requested `--out` target for get.
- Release client build should prefer the local OpenAPI spec when present; otherwise it can fall back to the tagged release URL.
- Integration tests use `assert_cmd` + `wiremock` and cover 26 CLI command scenarios; workspace tests currently pass with 27 total tests.
- LSP diagnostics could not run because `rust-analyzer` is not installed in the environment.

## T35 — CI/CD release pipeline for `nolgia-cli`

- **Binary name is `nolgia`, not `nolgia-cli`.** The crate package is `nolgia-cli` (see `crates/cli/Cargo.toml`) but the `[[bin]] name = "nolgia"` section overrides the output filename. CI must build `--bin nolgia` and rename artifacts to `nolgia-<target>[.exe]`. The task brief's mention of "nolgia-cli" refers to the package, not the binary.
- **macOS matrix is universal, not per-arch.** Building both `x86_64-apple-darwin` and `aarch64-apple-darwin` in a single `macos-latest` job and merging via `lipo -create` produces one `nolgia-x86_64-apple-darwin` artifact. This matches GitHub's "macos-latest" runner conventions and keeps the release surface small.
- **`actions-rust-lang/setup-rust-toolchain` v1+ accepts a space-separated `targets:` input.** This is the cleanest way to install multiple cross-compile targets in one step without writing a `for` loop around `rustup target add`.
- **`setup-rust-toolchain` cache uses `Swatinem/rust-cache` under the hood and only needs `GITHUB_TOKEN`** — no extra secrets. This is the right default for this project (constraint was "no third-party Rust caching that requires extra secrets"). Do not swap in a different cache action.
- **Release profile already does `strip = "symbols"`.** An extra explicit `strip`/`strip -x` step is still added for Linux and macOS as a belt-and-suspenders measure (cargo can leave symbols when cross-compiling in some setups). Windows skips the explicit strip — its release artifact is already small and stripping MSVC binaries post-build requires different tooling.
- **Tag pattern `v*` matches anything starting with `v`** (e.g. `v0.1.0`, `v1.2.3-rc.1`). `if: startsWith(github.ref, 'refs/tags/v')` is the canonical gate for the `build-release` and `release` jobs.
- **SHA pinning pattern:** every third-party action must use a 40-char commit SHA. Verified SHAs used in this workflow:
  - `actions/checkout@b4ffde65f46336ab88eb53be808477a3936bae11` (v4.1.1)
  - `actions-rust-lang/setup-rust-toolchain@9d7e65c3fdb52b106803724bf48063d05e9c7a95`
  - `actions/upload-artifact@b4b15b8c7c6ac21ea08fcf65892d2ee8f75cf882` (v4.4.0)
  - `actions/download-artifact@fa0a91b85d4f404e444e00e005971372dc801d16` (v4.1.1)
  - `softprops/action-gh-release@de68b1b0886157b3c984d524b7f86e5c2f3670d3` (v2.0.2)
- **Job-level `permissions:` blocks** keep the principle of least privilege: only the `release` job requests `contents: write`; `test` and `build-release` get `contents: read`. Don't hoist `contents: write` to workflow scope.
- **`fail_on_unmatched_files: true`** on `softprops/action-gh-release` is important: if a matrix entry failed silently or an artifact name changed, the release step should fail loudly rather than publish a half-broken release.
- **Release job must be `needs: [test, build-release]`** (not just `build-release`) so a broken `clippy` or `cargo fmt` blocks the release even if all builds succeed.
