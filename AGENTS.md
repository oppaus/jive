# Project Instructions: Rust TUI + J Language Engine

## Project Overview

This project builds a modern Terminal User Interface (TUI) in Rust using the
`ratatui` ecosystem. It acts as an interactive frontend for the J programming
language engine. The TUI accepts user input, pipes it to J via process standard
streams or library bindings, parses the array output, and renders layouts
alongside graphical plots using the Kitty/Sixel protocols via the
`ratatui-image` crate.

## Environment & Tech Stack

- **Frontend:** Rust (Edition 2021), `ratatui` (latest framework), `crossterm` backend.
- **Graphics Pipeline:** `ratatui-image` utilizing the Kitty Graphics protocol
  or Sixel terminal bindings.
- **Backend:** Jsoftware engine interpreter (`ijconsole` binary must be
  accessible in shell).
- **Communication:** Standard I/O asynchronous process spawning or C
  shared-library bindings (`crates/lib`).

## Architecture & Code Guidelines

- Use the Elm Architecture pattern (Model-View-Update) standard to `ratatui`.
- Never hardcode absolute file paths for temporary image storage; handle
  in-memory mapping or use standard OS temp directories (`std::env::temp_dir()`).
- All background J commands should execute in a separate asynchronous thread or
  non-blocking event loop to ensure the TUI rendering loop maintains a
  responsive 60+ FPS rate.
- Error Handling: Do not use `unwrap()` or `expect()`. Gracefully catch broken
  pipes from the J interpreter process and report a visual disconnect inside
  the TUI status bar.

## Verification & Commands

- **Build Layout:** `cargo build`
- **Lint Check:** `cargo clippy --all-targets`
- **Test Suite:** `cargo test`

## External Dependencies & Source Paths

- **Local J Engine Source:** `../../GitHub/jsource`
- **Upstream Repository:** [jsoftware/jsource](https://github.com/jsoftware/jsource "J Engine GitHub Mirror")

## Context Context Rules

1. When generating or auditing FFI bindings in `lib`, the agent should
   inspect the internal C headers inside the local `jsource` directory rather
   than assuming API signatures.
2. Do not commit or modify files within the external `jsource` directory.
   Treat it strictly as a read-only architectural reference.
