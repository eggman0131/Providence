//! Port interfaces (docs/20-architecture.md §2.4).
//!
//! Placeholder: the port traits (`ConfigPort`, `RendererPort`,
//! `LLMOpponentPort`, …) are designed and land in Phase 4 of the
//! bootstrapping order (contract §7.4). This crate exists now to pin the
//! workspace crate graph that the boundary checker enforces (ADR 0009).
//! Adding a port is an architectural change and requires an ADR
//! (docs/20-architecture.md §5 rule 5).

#![no_std]
#![forbid(unsafe_code)]
