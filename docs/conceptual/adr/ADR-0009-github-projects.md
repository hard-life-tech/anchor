# ADR-0009 — GitHub Projects for roadmap

**Status:** Accepted  
**Date:** 2026-08-02

## Context

Hard Life Tech needs a public-friendly way to track OSS Core work, and a private place for SaaS work. GitHub Projects (and Enterprise Projects where applicable) fit the GitHub-centric workflow.

## Decision

- Track **OSS Core** on a public GitHub Project board once the org/repo exist.
- Track **Management SaaS** on a **private** project/repo.
- Use issues linked to timeline backlog IDs; keep secrets out of project notes.

## Consequences

- Blocked until org/repo creation (`gh` / credentials).
- Docs timeline remains the in-repo source until the board exists.
