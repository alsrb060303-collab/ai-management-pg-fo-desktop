import type { Messages } from "./types";

export const ko = {
  "tools.label": "도구",
  "tools.auto": "자동",
  "tools.autoWith": "자동 ({tool})",
  "tools.reason.files": "프로젝트 파일로 감지: {files}",
  "tools.reason.installed": "설치된 CLI로 감지 ({tool}만 설치됨)",
  "tools.reason.fallback": "감지할 단서가 없어 기본 도구를 사용",
  "tools.picked": "직접 고름. 자동 감지 결과: {tool}",
  "tools.reason.main": "메인 저장소에서 고른 도구를 따름",
  "tools.default": "기본 도구",
  "tools.defaultHint":
    "프로젝트에 설정 파일이 없고 CLI가 둘 다(또는 하나도) 설치되지 않았을 때 쓰는 도구예요.",
} as const;

export const en: Messages<typeof ko> = {
  "tools.label": "Tool",
  "tools.auto": "Auto",
  "tools.autoWith": "Auto ({tool})",
  "tools.reason.files": "Detected from project files: {files}",
  "tools.reason.installed": "Detected from the installed CLI (only {tool} is installed)",
  "tools.reason.fallback": "Nothing to detect from, so the default tool is used",
  "tools.picked": "Picked by you. Detected: {tool}",
  "tools.reason.main": "Follows the tool picked for the main repository",
  "tools.default": "Default tool",
  "tools.defaultHint":
    "Used when a project has no config files and both CLIs (or neither) are installed.",
};
