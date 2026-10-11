---
kind: rule
title: Rust 규칙
summary: clippy 경고 0, 라이브러리 코드에서 unwrap 지양, 명확한 에러 타입. .rs 파일에만 적용돼요.
category: Rust
paths:
  - "**/*.rs"
---
# Rust

- Keep `cargo fmt` and `cargo clippy -- -D warnings` clean.
- Avoid `unwrap()` and `expect()` outside tests and startup code; propagate errors with `?`.
- Prefer borrowing over cloning; clone only when ownership is really needed.
- Keep `unsafe` blocks minimal and document the invariant that makes each one sound.
