---
kind: rule
title: TypeScript 규칙
title_en: TypeScript rules
summary: any 금지, 엄격한 타입, 타입 추론 활용. .ts/.tsx 파일에만 적용돼요.
summary_en: No any, strict types, lean on type inference. Applies to .ts/.tsx files only.
category: TypeScript
category_en: TypeScript
paths:
  - "**/*.ts"
  - "**/*.tsx"
---
# TypeScript

- Keep `strict` type checking passing. Do not use `any`; use `unknown` and narrow it.
- Let inference work for locals; annotate exported functions' parameters and return types.
- Prefer union types and discriminated unions over enums and boolean flags.
- Do not silence errors with `@ts-ignore`; use `@ts-expect-error` with a reason only when unavoidable.
