---
kind: hook
title: 답을 마치면 린트 실행
title_en: Lint when Claude finishes
summary: Claude가 답을 끝낼 때마다 npm 린트를 돌려요
summary_en: Runs the npm lint script each time Claude finishes
detail: Stop hook이라 Claude가 응답을 마칠 때마다 `npm run lint --if-present`를 실행해요. lint 스크립트가 없는 프로젝트에서는 아무것도 하지 않아요. pnpm·yarn을 쓰면 미리보기의 '편집'에서 명령을 바꾸세요. 큰 프로젝트에서는 매번 시간이 걸릴 수 있어요.
detail_en: A Stop hook: runs `npm run lint --if-present` each time Claude finishes a reply, and does nothing without a lint script. For pnpm or yarn, change the command under 'Edit' in the preview. Can take a while on large projects.
category: 품질
category_en: Quality
name: stop-lint
event: Stop
tokens: 0
performance: 0
quality: 0
---
npm run lint --if-present
