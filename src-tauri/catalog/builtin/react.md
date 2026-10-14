---
kind: rule
title: React 규칙
title_en: React rules
summary: React 파일에 함수 컴포넌트·훅 규칙을 적용해요
summary_en: React conventions for .tsx/.jsx files
detail: .tsx/.jsx 파일을 다룰 때만 적용돼요. 함수 컴포넌트와 훅 규칙을 지키고, 상태는 꼭 필요한 만큼만 두고, 파생 값은 계산해서 쓰게 해요.
detail_en: Applies only to .tsx/.jsx files: function components, the rules of hooks, minimal state, and derived values computed instead of stored.
category: React
category_en: React
paths:
  - "**/*.tsx"
  - "**/*.jsx"
tokens: 0
performance: 0
quality: 1
---
# React

- Use function components and hooks. Follow the rules of hooks.
- Keep state minimal: derive values during render instead of syncing them with effects.
- Use effects only to sync with external systems; clean up subscriptions and timers.
- Give list items stable keys from data, not array indexes.
- Keep components small; move non-UI logic into plain functions or stores.
