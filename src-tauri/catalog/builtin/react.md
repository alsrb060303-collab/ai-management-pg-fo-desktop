---
kind: rule
title: React 규칙
title_en: React rules
summary: 함수 컴포넌트와 훅 규칙, 상태 최소화. .tsx/.jsx 파일에만 적용돼요.
summary_en: Function components, the rules of hooks, minimal state. Applies to .tsx/.jsx files only.
category: React
category_en: React
paths:
  - "**/*.tsx"
  - "**/*.jsx"
---
# React

- Use function components and hooks. Follow the rules of hooks.
- Keep state minimal: derive values during render instead of syncing them with effects.
- Use effects only to sync with external systems; clean up subscriptions and timers.
- Give list items stable keys from data, not array indexes.
- Keep components small; move non-UI logic into plain functions or stores.
