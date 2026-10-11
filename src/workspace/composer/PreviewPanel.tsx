import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AlertTriangle, ExternalLink, RotateCcw } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "../../components/ui";
import { useT } from "../../i18n";
import {
  type BlockNode,
  CLAUDE_MD_LINE_BUDGET,
  originUrl,
  useComposerStore,
} from "../../store/composer";
import { KIND_LABEL, kindColor } from "./kinds";

type Tab = "preview" | "block";

export function PreviewPanel({ onWrite }: { onWrite: () => void }) {
  const compiled = useComposerStore((s) => s.compiled);
  const selected = useComposerStore(
    (s) => s.doc?.nodes.find((n) => n.selected && n.type === "block") as BlockNode | undefined,
  );
  const [tab, setTab] = useState<Tab>("preview");
  const [path, setPath] = useState<string | null>(null);
  const t = useT();

  // Show the block editor when a block gets selected.
  const selectedId = selected?.id;
  useEffect(() => {
    setTab(selectedId ? "block" : "preview");
  }, [selectedId]);

  const files = compiled?.files ?? [];
  const file = files.find((f) => f.path === path) ?? files[0];
  const warnings = compiled?.warnings ?? [];

  return (
    <aside className="flex w-[360px] shrink-0 flex-col border-l border-border bg-surface">
      <div className="flex h-11 shrink-0 items-center gap-1 border-b border-border px-2">
        <TabButton active={tab === "preview"} onClick={() => setTab("preview")}>
          {t("composer.preview.tab")}
        </TabButton>
        <TabButton active={tab === "block"} onClick={() => setTab("block")} disabled={!selected}>
          {t("composer.preview.blockTab")}
        </TabButton>
      </div>

      {tab === "block" && selected ? (
        <BlockEditor key={selected.id} node={selected} />
      ) : (
        <>
          {warnings.length > 0 && (
            <ul className="space-y-1 border-b border-border bg-sunken/60 px-3 py-2">
              {warnings.map((w) => (
                <li key={`${w.node}:${w.message}`} className="flex gap-1.5 text-xs text-warning">
                  <AlertTriangle size={13} className="mt-0.5 shrink-0" />
                  <span>{w.message}</span>
                </li>
              ))}
            </ul>
          )}

          {files.length === 0 ? (
            <div className="flex flex-1 flex-col items-center justify-center px-8 text-center">
              <p className="text-sm font-medium">{t("composer.preview.empty")}</p>
              <p className="mt-1 text-xs text-text-muted">{t("composer.preview.emptyHint")}</p>
            </div>
          ) : (
            <>
              <div className="flex shrink-0 gap-1 overflow-x-auto border-b border-border px-2 py-1.5">
                {files.map((f) => (
                  <button
                    key={f.path}
                    type="button"
                    onClick={() => setPath(f.path)}
                    title={f.path}
                    className={`shrink-0 rounded-md px-2 py-1 font-mono text-[11px] ${
                      f.path === file?.path
                        ? "bg-active text-text"
                        : "text-text-muted hover:bg-hover"
                    }`}
                  >
                    {f.path.split("/").slice(-2).join("/")}
                  </button>
                ))}
              </div>
              {file && (
                <div className="flex min-h-0 flex-1 flex-col">
                  <div className="flex shrink-0 items-center gap-2 px-3 py-2 text-[11px] text-text-muted">
                    <span className="truncate font-mono">{file.path}</span>
                    {file.personal && (
                      <span className="rounded bg-sunken px-1 text-text-subtle">
                        {t("composer.personal")}
                      </span>
                    )}
                    <span
                      className={`ml-auto shrink-0 ${
                        file.section && file.lines > CLAUDE_MD_LINE_BUDGET
                          ? "font-medium text-danger"
                          : ""
                      }`}
                    >
                      {t("composer.preview.lines", { n: file.lines })}
                    </span>
                  </div>
                  {file.section && (
                    <p className="mx-3 mb-2 rounded-md bg-accent-soft px-2.5 py-1.5 text-[11px] text-text-muted">
                      {t("composer.preview.section", { path: file.path })}
                    </p>
                  )}
                  <CodeView text={file.content} />
                </div>
              )}
            </>
          )}

          <div className="shrink-0 border-t border-border p-3">
            <Button
              variant="primary"
              className="w-full"
              disabled={files.length === 0}
              onClick={onWrite}
            >
              {t("composer.write.title")}
            </Button>
          </div>
        </>
      )}
    </aside>
  );
}

function TabButton({
  active,
  disabled,
  onClick,
  children,
}: {
  active: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      aria-pressed={active}
      onClick={onClick}
      className={`rounded-md px-2.5 py-1 text-sm transition-colors disabled:opacity-40 ${
        active ? "bg-active font-medium text-text" : "text-text-muted hover:text-text"
      }`}
    >
      {children}
    </button>
  );
}

function CodeView({ text }: { text: string }) {
  const lines = text.split("\n");
  return (
    <div className="selectable min-h-0 flex-1 overflow-auto px-1 pb-3 font-mono text-[12px] leading-[1.6]">
      <table className="border-collapse">
        <tbody>
          {lines.map((line, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: lines have no identity besides position
            <tr key={i}>
              <td className="pr-3 pl-2 text-right align-top text-text-subtle select-none">
                {i + 1}
              </td>
              <td className="pr-3 break-words whitespace-pre-wrap">{line || " "}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function BlockEditor({ node }: { node: BlockNode }) {
  const block = useComposerStore((s) => s.blocks.get(node.data.blockId));
  const setBody = useComposerStore((s) => s.setBody);
  const original = block?.body ?? "";
  const [draft, setDraft] = useState(node.data.body ?? original);
  const t = useT();

  // Follow undo/redo and catalog reloads (a new node remounts via `key`).
  useEffect(() => {
    setDraft(node.data.body ?? original);
  }, [node.data.body, original]);

  const commit = () => {
    const body = draft === original ? undefined : draft;
    if (body !== node.data.body) setBody(node.id, body);
  };

  if (!block) {
    return <p className="p-4 text-sm text-danger">{t("composer.block.missing")}</p>;
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 p-3">
      <div>
        <div className="text-[11px] font-medium" style={{ color: kindColor(block.kind) }}>
          {t(KIND_LABEL[block.kind])} · {block.category}
        </div>
        <div className="mt-0.5 font-medium">{block.title}</div>
        <div className="mt-0.5 text-xs text-text-muted">{block.summary}</div>
        {block.origin && (
          <button
            type="button"
            onClick={() => {
              if (block.origin && isTauri()) void openUrl(originUrl(block.origin));
            }}
            className="mt-1.5 flex max-w-full items-center gap-1 text-left text-[11px] text-text-muted hover:text-text"
            title={originUrl(block.origin)}
          >
            <ExternalLink size={11} className="shrink-0" />
            <span className="truncate">
              {block.origin.repo} · {block.origin.license} · {block.origin.copyright}
            </span>
          </button>
        )}
        {block.paths.length > 0 && (
          <div className="mt-1.5 flex flex-wrap gap-1">
            {block.paths.map((p) => (
              <code
                key={p}
                className="rounded bg-sunken px-1 font-mono text-[11px] text-text-muted"
              >
                {p}
              </code>
            ))}
          </div>
        )}
      </div>
      <textarea
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        spellCheck={false}
        className="selectable min-h-0 flex-1 resize-none rounded-lg border border-border bg-surface-raised p-3 font-mono text-[12px] leading-[1.6] outline-none focus:border-border-strong"
      />
      <div className="flex items-center gap-2">
        <span className="text-[11px] text-text-subtle">
          {node.data.body !== undefined ? t("composer.block.edited") : t("composer.block.original")}
        </span>
        <Button
          size="sm"
          variant="ghost"
          className="ml-auto"
          disabled={node.data.body === undefined && draft === original}
          onClick={() => {
            setDraft(original);
            setBody(node.id, undefined);
          }}
        >
          <RotateCcw size={13} />
          {t("composer.block.reset")}
        </Button>
        <Button size="sm" onClick={commit} disabled={draft === (node.data.body ?? original)}>
          {t("composer.block.apply")}
        </Button>
      </div>
    </div>
  );
}
