---
kind: rule
title: TypeScript 규칙
summary: any 금지, 엄격한 타입, 타입 추론 활용. .ts/.tsx 파일에만 적용돼요.
category: TypeScript
paths:
  - "**/*.ts"
  - "**/*.tsx"
---
# TypeScript

- Keep `strict` type checking passing. Do not use `any`; use `unknown` and narrow it.
- Let inference work for locals; annotate exported functions' parameters and return types.
- Prefer union types and discriminated unions over enums and boolean flags.
- Do not silence errors with `@ts-ignore`; use `@ts-expect-error` with a reason only when unavoidable.
