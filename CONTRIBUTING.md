# Contributing to Granite CLI

Thank you for your interest in contributing! This guide covers the basic setup and development workflow.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable version)
  - Verify installation: `rustup --version`

## Getting Started

1. Clone the repository
2. Build the project for the first time to resolve dependencies:

```bash
cargo build
```

## Common Commands

| Command | Description |
|---------|-------------|
| `cargo build` | Compile the project (in debug mode) |
| `cargo run` | Run the CLI tool locally |
| `cargo test` | Run all tests in dev-mode only |
| `cargo clippy` | Lint code for style and potential issues |

## Development Workflow

Before submitting a change:

1. Ensure your feature builds successfully:
   ```bash
    cargo build
```

2. If you added new functionality or modified existing behavior, add tests in the relevant test files and run `cargo test` to verify everything passes.

3. Run clippy to check for linting issues (catches common mistakes):
   ```bash
    cargo clippy