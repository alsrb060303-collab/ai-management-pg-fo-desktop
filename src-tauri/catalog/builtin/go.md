---
kind: rule
title: Go 규칙
summary: gofmt, 에러 감싸기, 작은 인터페이스. .go 파일에만 적용돼요.
category: Go
paths:
  - "**/*.go"
---
# Go

- Code must be `gofmt`-formatted and pass `go vet`.
- Wrap errors with context using `fmt.Errorf("...: %w", err)`; never ignore returned errors.
- Accept interfaces, return concrete types; keep interfaces small and defined where they are used.
- Pass `context.Context` as the first parameter to functions that do I/O.
