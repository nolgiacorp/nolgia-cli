# Nolgia CLI

Rust CLI for the Nolgia generation platform.

## Installation

You can install the CLI directly using Cargo:

```bash
cargo install nolgia-cli
```

Alternatively, download the latest binary from the [GitHub Releases](https://github.com/nolgiacorp/nolgia-cli/releases) page.

## Authentication

The CLI uses device-code OAuth for secure authentication.

```bash
# Log in to your account
nolgia login

# Check your current authentication status
nolgia auth status
```

## Commands

### Generation

Generate media using various modalities:

```bash
# Generate an image
nolgia gen image --prompt "A serene mountain lake"

# Generate audio
nolgia gen audio --prompt "Lofi hip hop beats for studying"

# Generate video
nolgia gen video --prompt "A drone shot over a coastline"
```

### Management

List and retrieve details for your jobs and assets:

```bash
# Jobs
nolgia jobs list
nolgia jobs get <id>

# Assets
nolgia assets list
nolgia assets get <id>
```

### Account and Billing

Manage your Nolgia account:

```bash
# Open the Stripe billing portal
nolgia billing portal

# View account details
nolgia account
```

## Development Quickstart

### Build

Ensure you have the Rust 2024 edition toolchain installed.

```bash
# Build the project
cargo build --release

# Run tests across the workspace
cargo test --workspace
```

The CLI is built with `tokio`, `reqwest`, and `clap`.
