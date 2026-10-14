---
kind: rule
title: Go 규칙
title_en: Go rules
summary: Go 파일에 gofmt·에러 처리 규칙을 적용해요
summary_en: Go conventions for .go files
detail: .go 파일을 다룰 때만 적용되는 규칙이에요. gofmt 형식, 에러를 감싸서 문맥 남기기, 작은 인터페이스, 고루틴 누수 주의 같은 Go 관례를 따르게 해요.
detail_en: Applies only when working on .go files: gofmt formatting, wrapping errors with context, small interfaces, and care with goroutine leaks.
category: Go
category_en: Go
paths:
  - "**/*.go"
tokens: 0
performance: 0
quality: 1
---
# Go

- Code must be `gofmt`-formatted and pass `go vet`.
- Wrap errors with context using `fmt.Errorf("...: %w", err)`; never ignore returned errors.
- Accept interfaces, return concrete types; keep interfaces small and defined where they are used.
- Pass `context.Context` as the first parameter to functions that do I/O.
