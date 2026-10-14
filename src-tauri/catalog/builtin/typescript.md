---
kind: rule
title: TypeScript 규칙
title_en: TypeScript rules
summary: TS 파일에서 any 없이 엄격한 타입을 써요
summary_en: Strict TypeScript for .ts/.tsx files
detail: .ts/.tsx 파일을 다룰 때만 적용돼요. any를 쓰지 않고, strict 모드에 맞추고, 타입 추론을 살리되 공개 함수엔 타입을 명시하게 해요.
detail_en: Applies only to .ts/.tsx files: no any, strict mode, type inference where it helps and explicit types on exported functions.
category: TypeScript
category_en: TypeScript
paths:
  - "**/*.ts"
  - "**/*.tsx"
tokens: 0
performance: 0
quality: 1
---
# TypeScript

- Keep `strict` type checking passing. Do not use `any`; use `unknown` and narrow it.
- Let inference work for locals; annotate exported functions' parameters and return types.
- Prefer union types and discriminated unions over enums and boolean flags.
- Do not silence errors with `@ts-ignore`; use `@ts-expect-error` with a reason only when unavoidable.
