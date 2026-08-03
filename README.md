# Granite CLI

Granite CLI is a Rust command-line tool for discovering, configuring, and eventually launching Granite-powered local and remote AI workflows.

Today, the project already supports:
- browsing a catalog of Granite models
- configuring models and providers
- checking provider health
- recommending models based on local hardware
- pulling model weights for supported local providers
- rendering output in the terminal, as plain text, JSON, and Markdown

The longer-term goal is for Granite CLI to become the control plane for Granite-based developer tooling: a single place to manage models, providers, capabilities, and launcher integrations for tools like coding assistants and agentic workflows.

## Current direction

The project is currently focused on building a strong foundation in a few areas:

- **Model management**: catalog, search, recommendation, setup, and pull flows for Granite models
- **Provider management**: support for local and OpenAI-compatible providers, including health checks and model compatibility
- **Capability architecture**: describing higher-level Granite capabilities that can be attached to tools and workflows
- **Launcher architecture**: eventually launching external tools with Granite-aware configuration and overlays
- **Multiple output modes**: terminal-first UX with machine-readable output formats for scripting and automation

## What the project wants to be

Granite CLI is aiming to become:

1. a **developer-facing interface** for working with Granite models and capabilities
2. a **local setup and orchestration tool** for providers like Ollama, LM Studio, llama.cpp, and compatible servers
3. a **bridge layer** between Granite model/capability definitions and downstream tools such as assistants, launchers, and IDE workflows
4. a **repeatable automation surface** that works well both interactively and in scripts/CI

## Near-term roadmap themes

Based on the current codebase and open issues, the near-term direction includes:

- improving CI and contributor workflows
- expanding provider support
- expanding model metadata and registry coverage
- building out TUI setup and browsing flows
- implementing capability and launcher architecture
- improving documentation and release automation

## Status

This repository is still under active construction. The architecture is taking shape, but some areas are intentionally incomplete while core foundations are being built.

## Contributing

Contributions are welcome. If you are picking up an issue, it is usually best to:

- start with a focused, reviewable change
- align with the existing registry/factory-driven architecture
- keep changes minimal and task-specific
- run formatting, linting, and tests before opening a PR
