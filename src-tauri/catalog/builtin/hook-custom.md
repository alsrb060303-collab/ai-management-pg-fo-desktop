---
kind: hook
title: 직접 만드는 hook
title_en: Custom hook
summary: 이벤트와 명령을 직접 정하는 빈 hook이에요
summary_en: An empty hook: pick the event and write the command
detail: 캔버스에서 이벤트(언제)와 matcher(어떤 도구일 때)를 고르고, 미리보기에서 '편집'으로 실행할 명령을 적어요. Claude Code가 그 시점마다 이 명령을 직접 실행하니, 믿을 수 있는 명령만 넣으세요.
detail_en: Pick the event (when) and matcher (for which tool) on the canvas, then write the command under 'Edit' in the preview. Claude Code runs it by itself every time, so only use commands you trust.
category: 직접 작성
category_en: Custom
name: custom-hook
event: PostToolUse
matcher: Edit|Write
tokens: 0
performance: 0
quality: 0
---
