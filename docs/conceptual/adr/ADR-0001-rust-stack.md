# ADR-0001 — Rust + axum/tokio stack

**Status:** Accepted  
**Date:** 2026-08-02

## Context

Anchor is a long-running orchestrator on a VPS: HTTP API, subprocess control (`git`, `tmux`), and GitHub HTTPS calls. We need a small, reliable binary suitable for Docker.

## Decision

Use **Rust** with **tokio** async runtime and **axum** for HTTP. Supporting crates as listed in `PROJECT.md` §7 (`serde`, `reqwest`, `tracing`, `anyhow`/`thiserror`, Askama + htmx for UI).

## Consequences

- Matches existing Dockerfile (`cargo build --release`, binary `anchor`).
- Strong concurrency model for parallel git status + HTTP.
- Team must be comfortable with Rust; no Node orchestrator alternative in v1.
