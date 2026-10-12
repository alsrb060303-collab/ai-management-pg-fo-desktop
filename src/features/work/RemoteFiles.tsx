import { ask } from "@tauri-apps/plugin-dialog";
import {
  ChevronRight,
  FilePlus,
  FileText,
  Folder,
  RefreshCw,
  RotateCcw,
  Search,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useT } from "../../i18n";
import { type Draft, useRemoteStore } from "../../store/remote";
import { inputClass } from "./GitHubSection";

/** Files listed at once when searching. */
const SEARCH_LIMIT = 200;

const kindOf = (d: Draft) => (d.text === null ? "D" : d.original === null ? "A" : "M");
const KIND_COLOR: Record<string, string> = {
  M: "text-warning",
  A: "text-success",
  D: "text-danger",
};

/**
 * The shown branch's files on GitHub: browse, open, edit, add and delete
 * them, then commit every change at once to the branch or a new one.
 */
export function RemoteFiles({ compact }: { compact: boolean }) {
  const t = useT();
  const { data, tree, treeLoading, drafts, openPath, loadTree, openFile, newFile } =
    useRemoteStore();
  const [shown, setShown] = useState(false);
  const [dir, setDir] = useState("");
  const [query, setQuery] = useState("");
  const [adding, setAdding] = useState(false);
  const [newPath, setNewPath] = useState("");
  const changed = Object.keys(drafts).length;

  // Loaded when opened, and again after a branch switch cleared it.
  useEffect(() => {
    if (shown && !tree && !treeLoading && data) void loadTree();
  }, [shown, tree, treeLoading, data, loadTree]);

  // A new branch starts at its root.
  useEffect(() => {
    void data?.branch;
    setDir("");
  }, [data?.branch]);

  const listing = useMemo(() => {
    const paths = [
      ...(tree?.files.map((f) => f.path) ?? []),
      ...Object.keys(drafts).filter((p) => drafts[p].original === null),
    ];
    const q = query.trim().toLowerCase();
    if (q) {
      return {
        dirs: [] as string[],
        files: paths.filter((p) => p.toLowerCase().includes(q)).slice(0, SEARCH_LIMIT),
      };
    }
    const prefix = dir ? `${dir}/` : "";
    const dirs = new Set<string>();
    const files: string[] = [];
    for (const p of paths) {
      if (!p.startsWith(prefix)) continue;
      const rest = p.slice(prefix.length);
      const slash = rest.indexOf("/");
      if (slash >= 0) dirs.add(prefix + rest.slice(0, slash));
      else files.push(p);
    }
    return { dirs: [...dirs].sort(), files: files.sort() };
  }, [tree, drafts, query, dir]);

  if (!data) return null;
  const crumbs = dir ? dir.split("/") : [];

  return (
    <section className="space-y-1.5">
      <h3 className="flex h-6 items-center gap-2 text-xs font-semibold">
        <button
          type="button"
          onClick={() => setShown(!shown)}
          className="flex items-center gap-1 rounded px-1 py-0.5 hover:bg-hover"
        >
          <ChevronRight size={13} className={shown ? "rotate-90" : ""} />
          {t("work.files.title", { branch: data.branch })}
        </button>
        {changed > 0 && (
          <span className="rounded-full bg-warning/15 px-1.5 py-px text-[10px] font-normal text-warning">
            {t("work.files.changed", { n: changed })}
          </span>
        )}
        {shown && (
          <span className="ml-auto flex items-center gap-0.5 font-normal">
            <button
              type="button"
              onClick={() => setAdding(!adding)}
              title={t("work.files.new")}
              className="rounded p-1 text-text-subtle hover:bg-hover hover:text-text"
            >
              <FilePlus size={13} />
            </button>
            <button
              type="button"
              onClick={async () => {
                if (
                  changed &&
                  !(await ask(t("work.files.reloadConfirm"), {
                    title: t("work.files.title", { branch: data.branch }),
                    kind: "warning",
                  }).catch(() => false))
                )
                  return;
                useRemoteStore.setState({ drafts: {}, loaded: {}, openPath: null });
                void loadTree();
              }}
              title={t("work.refresh")}
              className="rounded p-1 text-text-subtle hover:bg-hover hover:text-text"
            >
              <RefreshCw size={12} className={treeLoading ? "animate-spin" : ""} />
            </button>
          </span>
        )}
      </h3>
      {shown && (
        <>
          {adding && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                const path = dir && !newPath.includes("/") ? `${dir}/${newPath}` : newPath;
                if (newFile(path)) {
                  setAdding(false);
                  setNewPath("");
                }
              }}
              className="flex items-center gap-1.5"
            >
              <input
                // biome-ignore lint/a11y/noAutofocus: opened on purpose
                autoFocus
                value={newPath}
                onChange={(e) => setNewPath(e.target.value)}
                placeholder={t("work.files.newPath")}
                className={inputClass}
              />
              <button
                type="submit"
                disabled={!newPath.trim()}
                className="shrink-0 rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
              >
                {t("work.create")}
              </button>
            </form>
          )}
          <div className="flex items-center gap-1.5">
            <Search size={12} className="shrink-0 text-text-subtle" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("work.files.search")}
              className={inputClass}
            />
          </div>
          {!query && (
            <div className="flex flex-wrap items-center gap-0.5 text-[11px] text-text-muted">
              <button type="button" onClick={() => setDir("")} className="hover:text-text">
                {data.branch}
              </button>
              {crumbs.map((part, i) => (
                <span key={crumbs.slice(0, i + 1).join("/")} className="flex items-center">
                  <span className="px-0.5">/</span>
                  <button
                    type="button"
                    onClick={() => setDir(crumbs.slice(0, i + 1).join("/"))}
                    className="hover:text-text"
                  >
                    {part}
                  </button>
                </span>
              ))}
            </div>
          )}
          {treeLoading && !tree ? (
            <p className="text-xs text-text-muted">{t("work.loading")}</p>
          ) : (
            <ul className={`space-y-px overflow-y-auto ${compact ? "max-h-56" : "max-h-72"}`}>
              {listing.dirs.map((d) => (
                <li key={d}>
                  <button
                    type="button"
                    onClick={() => setDir(d)}
                    className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-xs hover:bg-hover"
                  >
                    <Folder size={12} className="shrink-0 text-text-subtle" />
                    <span className="min-w-0 flex-1 truncate">
                      {d.slice(d.lastIndexOf("/") + 1)}
                    </span>
                  </button>
                </li>
              ))}
              {listing.files.map((p) => {
                const draft = drafts[p];
                const kind = draft ? kindOf(draft) : null;
                return (
                  <li key={p}>
                    <button
                      type="button"
                      onClick={() => void openFile(p)}
                      title={p}
                      className={`flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-xs hover:bg-hover ${openPath === p ? "bg-hover font-medium" : ""}`}
                    >
                      <FileText size={12} className="shrink-0 text-text-subtle" />
                      <span
                        className={`min-w-0 flex-1 truncate ${kind === "D" ? "line-through text-text-muted" : ""}`}
                      >
                        {query ? p : p.slice(p.lastIndexOf("/") + 1)}
                      </span>
                      {kind && (
                        <span className={`shrink-0 font-mono text-[10px] ${KIND_COLOR[kind]}`}>
                          {kind}
                        </span>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
          {tree?.truncated && (
            <p className="text-[11px] text-warning">{t("work.files.truncated")}</p>
          )}
          {openPath && <FileEditor path={openPath} compact={compact} />}
        </>
      )}
      {changed > 0 && <CommitPanel />}
    </section>
  );
}

function FileEditor({ path, compact }: { path: string; compact: boolean }) {
  const t = useT();
  const { loaded, drafts, editFile, deleteFile, discard } = useRemoteStore();
  const draft = drafts[path];
  const text = draft ? draft.text : loaded[path];
  if (text === undefined) return <p className="text-xs text-text-muted">{t("work.loading")}</p>;
  if (text === null) return null;
  const button =
    "flex shrink-0 items-center gap-1 rounded-md border border-border bg-surface-raised px-2 py-0.5 text-[11px] hover:border-border-strong disabled:opacity-50";

  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-1.5">
        <span className="selectable min-w-0 flex-1 truncate font-mono text-[11px]" title={path}>
          {path}
        </span>
        <button type="button" disabled={!draft} onClick={() => discard(path)} className={button}>
          <RotateCcw size={11} />
          {t("files.revert")}
        </button>
        <button
          type="button"
          onClick={() => deleteFile(path)}
          title={t("work.files.delete")}
          className={`${button} hover:text-danger`}
        >
          <Trash2 size={11} />
        </button>
      </div>
      <textarea
        value={text}
        onChange={(e) => editFile(path, e.target.value)}
        spellCheck={false}
        aria-label={path}
        rows={compact ? 12 : 20}
        className="selectable w-full resize-y rounded-lg border border-border bg-surface-raised p-2.5 font-mono text-xs leading-relaxed outline-none focus:border-border-strong"
      />
    </div>
  );
}

function CommitPanel() {
  const t = useT();
  const { data, drafts, busy, discard, openFile, commitFiles } = useRemoteStore();
  const [message, setMessage] = useState("");
  const [toNew, setToNew] = useState(false);
  const [branch, setBranch] = useState("");
  if (!data) return null;
  const entries = Object.entries(drafts);

  return (
    <form
      onSubmit={async (e) => {
        e.preventDefault();
        if (await commitFiles(message, toNew ? branch.trim() : null)) {
          setMessage("");
          setBranch("");
          setToNew(false);
        }
      }}
      className="space-y-1.5 rounded-lg border border-border bg-surface-raised p-3"
    >
      <p className="text-xs font-semibold">{t("work.files.changes", { n: entries.length })}</p>
      <ul className="space-y-px">
        {entries.map(([path, d]) => {
          const kind = kindOf(d);
          return (
            <li key={path} className="flex items-center gap-2 text-xs">
              <span className={`w-3 shrink-0 font-mono text-[10px] ${KIND_COLOR[kind]}`}>
                {kind}
              </span>
              <button
                type="button"
                onClick={() => d.text !== null && void openFile(path)}
                className="min-w-0 flex-1 truncate text-left font-mono text-[11px] hover:underline"
              >
                {path}
              </button>
              <button
                type="button"
                onClick={() => discard(path)}
                title={t("files.revert")}
                className="rounded p-0.5 text-text-subtle hover:bg-hover hover:text-text"
              >
                <RotateCcw size={11} />
              </button>
            </li>
          );
        })}
      </ul>
      <textarea
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        placeholder={t("work.commit.message")}
        rows={2}
        className={`${inputClass} resize-y`}
      />
      <div className="space-y-1 text-xs text-text-muted">
        <label className="flex items-center gap-1.5">
          <input type="radio" checked={!toNew} onChange={() => setToNew(false)} />
          {t("work.files.toBranch", { branch: data.branch })}
        </label>
        <label className="flex items-center gap-1.5">
          <input type="radio" checked={toNew} onChange={() => setToNew(true)} />
          {t("work.files.toNewBranch")}
        </label>
        {toNew && (
          <input
            value={branch}
            onChange={(e) => setBranch(e.target.value)}
            placeholder={t("work.remote.branchName")}
            className={inputClass}
          />
        )}
      </div>
      <div className="flex justify-end">
        <button
          type="submit"
          disabled={busy || !message.trim() || (toNew && !branch.trim())}
          className="rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
        >
          {t(busy ? "work.files.committing" : "work.files.commit")}
        </button>
      </div>
    </form>
  );
}
