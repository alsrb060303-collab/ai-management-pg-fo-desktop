---
kind: rule
title: Rust 규칙
title_en: Rust rules
summary: Rust 파일에 clippy·에러 타입 규칙을 적용해요
summary_en: Rust conventions for .rs files
detail: .rs 파일을 다룰 때만 적용돼요. clippy 경고 0개, 라이브러리 코드에서 unwrap 대신 에러 반환, 의미 있는 에러 타입을 쓰게 해요.
detail_en: Applies only to .rs files: zero clippy warnings, returning errors instead of unwrap in library code, and meaningful error types.
category: Rust
category_en: Rust
paths:
  - "**/*.rs"
tokens: 0
performance: 0
quality: 1
---
# Rust

- Keep `cargo fmt` and `cargo clippy -- -D warnings` clean.
- Avoid `unwrap()` and `expect()` outside tests and startup code; propagate errors with `?`.
- Prefer borrowing over cloning; clone only when ownership is really needed.
- Keep `unsafe` blocks minimal and document the invariant that makes each one sound.
