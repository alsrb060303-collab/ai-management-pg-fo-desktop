import type { Messages } from "./types";

export const ko = {
  "common.resize": "너비 조절",
  "common.resizeHint": "끌어서 너비 조절 · 더블클릭하면 원래대로",
  "common.crash.title": "이 화면을 그리다 오류가 났어요",
  "common.crash.hint":
    "다른 화면은 그대로 쓸 수 있어요. 아래 내용을 알려 주시면 고치는 데 도움이 돼요.",
  "common.crash.retry": "다시 시도",
};

export const en: Messages<typeof ko> = {
  "common.resize": "Resize",
  "common.resizeHint": "Drag to resize · double-click to reset",
  "common.crash.title": "Something went wrong showing this screen",
  "common.crash.hint": "The other screens still work. Reporting the message below helps us fix it.",
  "common.crash.retry": "Try again",
};
