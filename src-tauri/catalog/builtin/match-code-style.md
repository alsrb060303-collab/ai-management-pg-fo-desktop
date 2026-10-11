---
kind: instruction
title: 기존 코드 스타일 따르기
title_en: Match the existing code style
summary: 새 코드를 주변 코드의 이름 규칙·구조·주석 밀도에 맞추게 해요.
summary_en: Makes new code follow the naming, structure, and comment density of the code around it.
category: 코드 품질
category_en: Code quality
---
## Code style

- Match the surrounding code: naming, file layout, error handling, and comment density.
- Prefer existing helpers and libraries in the repo over adding new dependencies.
- Comments explain why, not what. Do not leave commented-out code.
- Keep functions focused; extract a helper only when it is reused or clearly clarifies intent.
