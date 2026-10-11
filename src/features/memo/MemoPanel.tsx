import { Check, GitBranch, GripVertical, Plus, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { type BranchInfo, shownBranch, type Todo, useMemoStore } from "../../store/memo";
import { useProjectStore } from "../../store/project";

/** How often the checked-out branch is re-read while the panel is visible. */
const POLL_MS = 4000;

/**
 * Branch note and TODOs for the selected project. Shared by the sidebar
 * (`compact`) and the main window's memo page.
 */
export function MemoPanel({ compact = false }: { compact?: boolean }) {
  const project = useProjectStore((s) => s.current);
  const choose = useProjectStore((s) => s.choose);
  const { context, picked, loading, error, load, poll, flushNote } = useMemoStore();
  const branch = shownBranch({ context, picked });

  useEffect(() => {
    void load(project);
  }, [project, load]);

  // Follow `git switch` in the terminal while the panel is on screen.
  useEffect(() => {
    const tick = () => {
      if (document.visibilityState === "visible") void poll();
    };
    const timer = setInterval(tick, POLL_MS);
    window.addEventListener("focus", tick);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", tick);
      void flushNote();
    };
  }, [poll, flushNote]);

  if (!project) {
    return (
      <div className="space-y-3 text-sm">
        <p className="text-text-muted">
          프로젝트 폴더를 고르면 브랜치마다 메모와 할 일을 남길 수 있어요.
        </p>
        <button
          type="button"
          onClick={() => void choose()}
          className="rounded-lg border border-border bg-surface-raised px-3 py-1.5 text-sm hover:border-border-strong"
        >
          폴더 선택
        </button>
      </div>
    );
  }

  return (
    <div className={compact ? "space-y-3" : "space-y-5"}>
      <BranchBar compact={compact} />
      {error && <p className="rounded-lg bg-danger-soft px-3 py-2 text-xs text-danger">{error}</p>}
      {loading && !context && <p className="text-xs text-text-muted">불러오는 중…</p>}
      {context && branch !== null && (
        <>
          <NoteEditor compact={compact} />
          <TodoList compact={compact} />
        </>
      )}
    </div>
  );
}

function branchLabel(b: BranchInfo, isGit: boolean, detachedAt: string | null): string {
  const name = !isGit ? "이 폴더" : b.name || `분리된 HEAD${detachedAt ? ` (${detachedAt})` : ""}`;
  const counts = b.total > 0 ? ` · ${b.open}/${b.total}` : "";
  const tags = [b.current ? "현재" : "", b.exists ? "" : "삭제됨"].filter(Boolean).join(", ");
  return `${name}${tags ? ` (${tags})` : ""}${counts}`;
}

function BranchBar({ compact }: { compact: boolean }) {
  const { context, picked, pick } = useMemoStore();
  const choose = useProjectStore((s) => s.choose);
  if (!context) return null;
  const branch = shownBranch({ context, picked }) ?? "";

  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-2 text-xs text-text-muted">
        <span className="min-w-0 flex-1 truncate" title={context.repoKey}>
          {context.repoName}
        </span>
        <button
          type="button"
          onClick={() => void choose()}
          className="shrink-0 rounded-md px-1.5 py-0.5 hover:bg-hover hover:text-text"
        >
          폴더 변경
        </button>
      </div>
      {context.isGit && (
        <div className="flex items-center gap-1.5">
          <GitBranch size={compact ? 14 : 15} className="shrink-0 text-text-subtle" />
          <select
            aria-label="브랜치"
            value={branch}
            onChange={(e) => pick(e.target.value)}
            className={`min-w-0 flex-1 truncate rounded-lg border border-border bg-surface-raised px-2 ${
              compact ? "py-1 text-xs" : "py-1.5 text-sm"
            }`}
          >
            {context.branches.map((b) => (
              <option key={b.name} value={b.name}>
                {branchLabel(b, context.isGit, context.detachedAt)}
              </option>
            ))}
          </select>
          {picked !== null && (
            <button
              type="button"
              onClick={() => pick(null)}
              title="체크아웃한 브랜치를 따라가요"
              className="shrink-0 rounded-md px-1.5 py-1 text-xs text-accent hover:bg-hover"
            >
              현재 브랜치로
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function NoteEditor({ compact }: { compact: boolean }) {
  const { note, noteDirty, setNote, flushNote } = useMemoStore();
  const ref = useRef<HTMLTextAreaElement>(null);

  // Grow with the text instead of scrolling inside a small box.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-measure when the text changes
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.max(el.scrollHeight, compact ? 72 : 120)}px`;
  }, [note, compact]);

  return (
    <section>
      <h3 className="mb-1.5 flex items-center gap-2 text-xs font-semibold">
        메모
        <span className="font-normal text-text-subtle">{noteDirty ? "저장 중…" : ""}</span>
      </h3>
      <textarea
        ref={ref}
        value={note}
        onChange={(e) => setNote(e.target.value)}
        onBlur={() => void flushNote()}
        placeholder="이 브랜치에서 하던 일, 다음에 이어서 할 일을 적어 두세요."
        spellCheck={false}
        className={`selectable block w-full resize-none rounded-lg border border-border bg-surface-raised px-3 py-2 leading-relaxed outline-none placeholder:text-text-subtle focus:border-border-strong ${
          compact ? "text-xs" : "text-sm"
        }`}
      />
    </section>
  );
}

function TodoList({ compact }: { compact: boolean }) {
  const { todos, addTodo, moveTodo, clearDone } = useMemoStore();
  const [draft, setDraft] = useState("");
  const [showDone, setShowDone] = useState(false);
  const open = todos.filter((t) => !t.done);
  const done = todos.filter((t) => t.done);

  const submit = () => {
    const text = draft.trim();
    if (!text) return;
    setDraft("");
    void addTodo(text);
  };

  /** Reorders among open items; done items keep their slots in the full order. */
  const reorder = (from: number, to: number) => {
    if (from === to) return;
    const ids = open.map((t) => t.id);
    const [moved] = ids.splice(from, 1);
    ids.splice(to, 0, moved);
    let next = 0;
    const full = todos.map((t) => (t.done ? t.id : ids[next++]));
    void moveTodo(moved, full.indexOf(moved));
  };

  const text = compact ? "text-xs" : "text-sm";

  return (
    <section>
      <h3 className="mb-1.5 flex items-center gap-2 text-xs font-semibold">
        할 일
        <span className="font-normal text-text-subtle">
          {open.length > 0 ? `${open.length}개 남음` : todos.length > 0 ? "모두 끝냄" : ""}
        </span>
      </h3>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
        className="mb-2 flex items-center gap-1.5 rounded-lg border border-border bg-surface-raised px-2 focus-within:border-border-strong"
      >
        <Plus size={14} className="shrink-0 text-text-subtle" />
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="할 일 추가 (Enter)"
          className={`min-w-0 flex-1 bg-transparent py-1.5 outline-none placeholder:text-text-subtle ${text}`}
        />
      </form>

      <Reorderable items={open} onReorder={reorder} compact={compact} />

      {done.length > 0 && (
        <div className="mt-2">
          <div className="flex items-center gap-2 text-[11px] text-text-subtle">
            <button
              type="button"
              onClick={() => setShowDone(!showDone)}
              className="rounded px-1 py-0.5 hover:bg-hover hover:text-text"
            >
              {showDone ? "▾" : "▸"} 완료 {done.length}개
            </button>
            <button
              type="button"
              onClick={() => void clearDone()}
              className="ml-auto rounded px-1 py-0.5 hover:bg-hover hover:text-danger"
            >
              완료 항목 지우기
            </button>
          </div>
          {showDone && (
            <ul className="mt-1 space-y-0.5">
              {done.map((t) => (
                <TodoRow key={t.id} todo={t} compact={compact} />
              ))}
            </ul>
          )}
        </div>
      )}
    </section>
  );
}

/** Open TODOs, reordered by dragging the grip (pointer events, not HTML5 DnD). */
function Reorderable({
  items,
  onReorder,
  compact,
}: {
  items: Todo[];
  onReorder: (from: number, to: number) => void;
  compact: boolean;
}) {
  const listRef = useRef<HTMLUListElement>(null);
  const [drag, setDrag] = useState<{ from: number; to: number } | null>(null);

  const indexAt = (clientY: number) => {
    const rows = Array.from(listRef.current?.children ?? []);
    const i = rows.findIndex((el) => {
      const r = el.getBoundingClientRect();
      return clientY < r.top + r.height / 2;
    });
    return i === -1 ? rows.length - 1 : i;
  };

  return (
    <ul ref={listRef} className="space-y-0.5">
      {items.map((t, i) => (
        <TodoRow
          key={t.id}
          todo={t}
          compact={compact}
          highlight={drag !== null && drag.from !== drag.to && drag.to === i}
          dragging={drag?.from === i}
          grip={
            <button
              type="button"
              tabIndex={-1}
              aria-label={`${t.text} 순서 바꾸기`}
              className="cursor-grab touch-none text-text-subtle opacity-0 group-hover:opacity-100 active:cursor-grabbing"
              onPointerDown={(e) => {
                e.currentTarget.setPointerCapture(e.pointerId);
                setDrag({ from: i, to: i });
              }}
              onPointerMove={(e) => {
                if (drag) setDrag({ ...drag, to: indexAt(e.clientY) });
              }}
              onPointerUp={() => {
                if (drag) onReorder(drag.from, drag.to);
                setDrag(null);
              }}
              onPointerCancel={() => setDrag(null)}
            >
              <GripVertical size={13} />
            </button>
          }
        />
      ))}
    </ul>
  );
}

function TodoRow({
  todo,
  compact,
  grip,
  highlight,
  dragging,
}: {
  todo: Todo;
  compact: boolean;
  grip?: React.ReactNode;
  highlight?: boolean;
  dragging?: boolean;
}) {
  const { toggleTodo, editTodo, deleteTodo } = useMemoStore();
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(todo.text);
  const size = compact ? "text-xs" : "text-sm";

  const save = () => {
    setEditing(false);
    if (text.trim() && text.trim() !== todo.text) void editTodo(todo.id, text);
    else setText(todo.text);
  };

  return (
    <li
      className={`group flex items-center gap-1.5 rounded-lg px-1 py-1 hover:bg-hover ${
        dragging ? "bg-accent-soft" : ""
      } ${highlight ? "ring-1 ring-accent" : ""}`}
    >
      {grip ?? <span className="w-[13px]" />}
      <span className="relative flex size-4 shrink-0">
        <input
          type="checkbox"
          checked={todo.done}
          aria-label={todo.done ? "다시 할 일로" : "완료"}
          onChange={() => void toggleTodo(todo.id)}
          className="peer size-4 cursor-pointer appearance-none rounded border border-border-strong bg-surface-raised transition-colors checked:border-accent checked:bg-accent hover:border-accent"
        />
        <Check
          size={11}
          strokeWidth={3}
          className="pointer-events-none absolute top-0.5 left-0.5 hidden text-accent-fg peer-checked:block"
        />
      </span>
      {editing ? (
        <input
          // biome-ignore lint/a11y/noAutofocus: opened by an explicit click on the item
          autoFocus
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={save}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") {
              setText(todo.text);
              setEditing(false);
            }
          }}
          className={`min-w-0 flex-1 rounded border border-border-strong bg-surface-raised px-1.5 py-0.5 outline-none ${size}`}
        />
      ) : (
        <button
          type="button"
          onClick={() => {
            setText(todo.text);
            setEditing(true);
          }}
          title="눌러서 고치기"
          className={`selectable min-w-0 flex-1 text-left break-words ${size} ${
            todo.done ? "text-text-subtle line-through" : ""
          }`}
        >
          {todo.text}
        </button>
      )}
      {editing ? (
        <button
          type="button"
          aria-label="취소"
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => {
            setText(todo.text);
            setEditing(false);
          }}
          className="shrink-0 rounded p-0.5 text-text-subtle hover:text-text"
        >
          <X size={13} />
        </button>
      ) : (
        <button
          type="button"
          aria-label="삭제"
          onClick={() => void deleteTodo(todo.id)}
          className="shrink-0 rounded p-0.5 text-text-subtle opacity-0 group-hover:opacity-100 hover:text-danger focus-visible:opacity-100"
        >
          <Trash2 size={13} />
        </button>
      )}
    </li>
  );
}
