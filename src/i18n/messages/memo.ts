import type { Messages } from "./types";

export const ko = {
  "memo.page.title": "메모",
  "memo.page.description":
    "브랜치마다 메모와 할 일을 남겨요. 터미널에서 브랜치를 바꾸면 따라 바뀌어요.",
  "memo.pickProject": "프로젝트 폴더를 고르면 브랜치마다 메모와 할 일을 남길 수 있어요.",
  "memo.chooseFolder": "폴더 선택",
  "memo.changeFolder": "폴더 변경",
  "memo.loading": "불러오는 중…",

  // Branch picker
  "memo.sync.syncing": "동기화하는 중",
  "memo.sync.synced": "다른 기기와 동기화됨",
  "memo.sync.failed": "동기화 실패",
  "memo.branch": "브랜치",
  "memo.branch.thisFolder": "이 폴더",
  "memo.branch.detached": "분리된 HEAD",
  "memo.branch.current": "현재",
  "memo.branch.deleted": "삭제됨",
  "memo.branch.follow": "현재 브랜치로",
  "memo.branch.followHint": "체크아웃한 브랜치를 따라가요",

  // Note
  "memo.note.title": "메모",
  "memo.note.saving": "저장 중…",
  "memo.note.placeholder": "이 브랜치에서 하던 일, 다음에 이어서 할 일을 적어 두세요.",
  "memo.note.saveFailed": "메모를 저장하지 못했어요: {error}",

  // TODOs
  "memo.todo.title": "할 일",
  "memo.todo.remaining": "{n}개 남음",
  "memo.todo.allDone": "모두 끝냄",
  "memo.todo.add": "할 일 추가 (Enter)",
  "memo.todo.doneCount": "완료 {n}개",
  "memo.todo.clearDone": "완료 항목 지우기",
  "memo.todo.reorder": "{text} 순서 바꾸기",
  "memo.todo.markOpen": "다시 할 일로",
  "memo.todo.markDone": "완료",
  "memo.todo.edit": "눌러서 고치기",
  "memo.todo.cancel": "취소",
  "memo.todo.delete": "삭제",
} as const;

export const en: Messages<typeof ko> = {
  "memo.sync.syncing": "Syncing",
  "memo.sync.synced": "Synced with your other devices",
  "memo.sync.failed": "Sync failed",
  "memo.page.title": "Notes",
  "memo.page.description":
    "Keep a note and to-dos for each branch. They follow along when you switch branches in the terminal.",
  "memo.pickProject": "Choose a project folder to keep a note and to-dos for each branch.",
  "memo.chooseFolder": "Choose folder",
  "memo.changeFolder": "Change folder",
  "memo.loading": "Loading…",

  "memo.branch": "Branch",
  "memo.branch.thisFolder": "This folder",
  "memo.branch.detached": "Detached HEAD",
  "memo.branch.current": "current",
  "memo.branch.deleted": "deleted",
  "memo.branch.follow": "Current branch",
  "memo.branch.followHint": "Follow the checked-out branch",

  "memo.note.title": "Note",
  "memo.note.saving": "Saving…",
  "memo.note.placeholder": "Jot down what you were doing on this branch and what to pick up next.",
  "memo.note.saveFailed": "Couldn't save the note: {error}",

  "memo.todo.title": "To-dos",
  "memo.todo.remaining": "{n} left",
  "memo.todo.allDone": "All done",
  "memo.todo.add": "Add a to-do (Enter)",
  "memo.todo.doneCount": "{n} done",
  "memo.todo.clearDone": "Clear done",
  "memo.todo.reorder": "Reorder {text}",
  "memo.todo.markOpen": "Mark as not done",
  "memo.todo.markDone": "Mark as done",
  "memo.todo.edit": "Click to edit",
  "memo.todo.cancel": "Cancel",
  "memo.todo.delete": "Delete",
};
