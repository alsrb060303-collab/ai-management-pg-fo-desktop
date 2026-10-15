import { ask } from "@tauri-apps/plugin-dialog";
import {
  AlarmClock,
  Blocks,
  CircleUserRound,
  FileText,
  GitBranch,
  GitGraph,
  House,
  Library,
  type LucideIcon,
  Map as MapIcon,
  NotebookPen,
  PanelLeft,
  PanelRightOpen,
  Plug,
  Plus,
  Settings,
  SquareSlash,
  Trash2,
} from "lucide-react";
import { useRef, useState } from "react";
import { Logo } from "../components/Logo";
import { ResizeHandle, usePanelWidth } from "../components/Resizable";
import { IconButton } from "../components/ui";
import { UpdateIndicator } from "../features/update/UpdateIndicator";
import { type MessageKey, useT } from "../i18n";
import { useAccountStore } from "../store/account";
import { useComposerStore } from "../store/composer";
import { type Page, useWorkspaceStore } from "../store/workspace";
import { switchToSidebar } from "./leave";

const NAV: { page: Page; label: MessageKey; icon: LucideIcon; soon?: boolean }[] = [
  { page: "home", label: "shell.nav.home", icon: House },
  { page: "composer", label: "shell.nav.composer", icon: Blocks },
  { page: "library", label: "shell.nav.library", icon: Library },
  { page: "config", label: "shell.nav.config", icon: MapIcon },
  { page: "files", label: "shell.nav.files", icon: FileText },
  { page: "mcp", label: "shell.nav.mcp", icon: Plug },
  { page: "commands", label: "shell.nav.commands", icon: SquareSlash },
  { page: "work", label: "shell.nav.work", icon: GitBranch },
  { page: "trees", label: "shell.nav.trees", icon: GitGraph },
  { page: "memo", label: "shell.nav.memo", icon: NotebookPen },
  { page: "automations", label: "shell.nav.automations", icon: AlarmClock },
];

export function NavSidebar() {
  const { page, collapsed, go, toggleCollapsed } = useWorkspaceStore();
  const docs = useComposerStore((s) => s.docs);
  const openId = useComposerStore((s) => s.doc?.id);
  const { newDoc, openDoc, deleteDoc, renameDoc } = useComposerStore.getState();
  const [renaming, setRenaming] = useState<string | null>(null);
  const t = useT();
  const panel = usePanelWidth("nav", 256, 200, 420);
  const account = useAccountStore((s) => s.info);
  const openAccount = useAccountStore((s) => s.openDialog);
  const accountLabel = account?.uid
    ? (account.profile?.email ?? account.profile?.name ?? t("account.title"))
    : t("account.nav.signIn");

  return (
    <aside
      className={`relative flex shrink-0 flex-col border-r border-border bg-sidebar ${
        panel.dragging ? "" : "transition-[width] duration-200"
      }`}
      style={{ width: collapsed ? 52 : panel.width }}
    >
      {!collapsed && <ResizeHandle side="right" {...panel.handle} />}
      <div className="flex h-12 shrink-0 items-center gap-1 px-2.5">
        {!collapsed && (
          <button
            type="button"
            onClick={() => go("home")}
            className="flex min-w-0 items-center gap-2 rounded-md px-1.5 font-serif text-fs-17 font-semibold tracking-tight"
          >
            <Logo size={20} />
            <span className="truncate">Encela</span>
          </button>
        )}
        <IconButton
          label={collapsed ? t("shell.nav.expand") : t("shell.nav.collapse")}
          onClick={toggleCollapsed}
          className={collapsed ? "" : "ml-auto"}
        >
          <PanelLeft size={18} />
        </IconButton>
      </div>

      <div className="px-2">
        <NavButton
          icon={Plus}
          label={t("shell.nav.newDoc")}
          collapsed={collapsed}
          accent
          onClick={() => {
            newDoc();
            go("composer");
          }}
        />
      </div>

      <nav className="mt-2 space-y-0.5 px-2">
        {NAV.map((item) => (
          <NavButton
            key={item.page}
            icon={item.icon}
            label={t(item.label)}
            collapsed={collapsed}
            active={page === item.page}
            badge={item.soon ? t("shell.nav.soon") : undefined}
            onClick={() => go(item.page)}
          />
        ))}
      </nav>

      <div className="mt-5 min-h-0 flex-1 overflow-y-auto px-2">
        {!collapsed && (
          <>
            <div className="flex items-center pr-1 pb-1 pl-2">
              <h3 className="flex-1 text-xs font-medium text-text-subtle">
                {t("shell.nav.recentDocs")}
              </h3>
              <IconButton
                label={t("shell.nav.newDoc")}
                onClick={() => {
                  void newDoc();
                  go("composer");
                }}
                className="size-6! rounded-md text-text-subtle"
              >
                <Plus size={14} />
              </IconButton>
            </div>
            {docs.length === 0 ? (
              <p className="px-2 text-xs text-text-subtle">{t("shell.nav.noDocs")}</p>
            ) : (
              <ul className="space-y-px">
                {docs.slice(0, 20).map((d) => {
                  const active = page === "composer" && d.id === openId;
                  if (renaming === d.id) {
                    return (
                      <li key={d.id}>
                        <RenameField
                          name={d.name}
                          label={t("shell.nav.renameDoc")}
                          onDone={(name) => {
                            setRenaming(null);
                            if (name && name !== d.name) void renameDoc(d.id, name);
                          }}
                        />
                      </li>
                    );
                  }
                  return (
                    <li key={d.id} className="group relative">
                      <button
                        type="button"
                        title={t("shell.nav.renameHint")}
                        onClick={() => {
                          void openDoc(d.id);
                          go("composer");
                        }}
                        onDoubleClick={() => setRenaming(d.id)}
                        onKeyDown={(e) => {
                          if (e.key === "F2") {
                            e.preventDefault();
                            setRenaming(d.id);
                          }
                        }}
                        className={`w-full truncate rounded-lg py-1.5 pr-8 pl-2 text-left text-fs-13 transition-colors ${
                          active
                            ? "bg-active text-text"
                            : "text-text-muted hover:bg-hover hover:text-text"
                        }`}
                      >
                        {d.name}
                      </button>
                      <button
                        type="button"
                        aria-label={t("shell.nav.deleteDoc", { name: d.name })}
                        title={t("shell.nav.delete")}
                        onClick={async () => {
                          const ok = await ask(t("shell.nav.deleteDocConfirm", { name: d.name }), {
                            title: t("shell.nav.deleteDocTitle"),
                            kind: "warning",
                          }).catch(() => false);
                          if (ok) void deleteDoc(d.id);
                        }}
                        className="absolute top-1/2 right-1 -translate-y-1/2 rounded-md p-1 text-text-subtle opacity-0 group-hover:opacity-100 hover:bg-hover hover:text-danger focus-visible:opacity-100"
                      >
                        <Trash2 size={13} />
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </>
        )}
      </div>

      <div className="space-y-0.5 border-t border-border p-2">
        <UpdateIndicator variant="nav" collapsed={collapsed} />
        {account?.configured && (
          <NavButton
            icon={CircleUserRound}
            label={accountLabel}
            collapsed={collapsed}
            onClick={openAccount}
          />
        )}
        <NavButton
          icon={PanelRightOpen}
          label={t("shell.switchToSidebar")}
          collapsed={collapsed}
          onClick={() => void switchToSidebar()}
        />
        <NavButton
          icon={Settings}
          label={t("shell.nav.settings")}
          collapsed={collapsed}
          active={page === "settings"}
          onClick={() => go("settings")}
        />
      </div>
    </aside>
  );
}

/** The document name being edited in place: Enter or leaving saves, Escape cancels. */
function RenameField({
  name,
  label,
  onDone,
}: {
  name: string;
  label: string;
  onDone: (name: string | null) => void;
}) {
  const [value, setValue] = useState(name);
  const done = useRef(false);
  const finish = (next: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(next);
  };
  return (
    <input
      // biome-ignore lint/a11y/noAutofocus: opened on purpose by double-click or F2
      autoFocus
      aria-label={label}
      title={label}
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onFocus={(e) => e.currentTarget.select()}
      onBlur={() => finish(value.trim() || null)}
      onKeyDown={(e) => {
        if (e.key === "Enter") finish(value.trim() || null);
        else if (e.key === "Escape") finish(null);
      }}
      className="w-full rounded-lg border border-accent bg-surface-raised px-2 py-1 text-fs-13 text-text outline-none"
    />
  );
}

interface NavButtonProps {
  icon: LucideIcon;
  label: string;
  collapsed: boolean;
  active?: boolean;
  accent?: boolean;
  badge?: string;
  onClick: () => void;
}

function NavButton({
  icon: Icon,
  label,
  collapsed,
  active,
  accent,
  badge,
  onClick,
}: NavButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={collapsed ? label : undefined}
      aria-label={collapsed ? label : undefined}
      aria-current={active ? "page" : undefined}
      className={`flex h-9 w-full items-center gap-2.5 rounded-lg px-2 text-sm transition-colors ${
        active
          ? "bg-active font-medium text-text"
          : "text-text-muted hover:bg-hover hover:text-text"
      }`}
    >
      <span
        className={`flex shrink-0 items-center justify-center ${
          accent
            ? "size-6 -ml-0.5 rounded-full bg-linear-to-br from-brand-from via-brand-via to-brand-to text-white"
            : "size-5"
        }`}
      >
        <Icon size={accent ? 15 : 17} strokeWidth={accent ? 2.5 : 2} />
      </span>
      {!collapsed && (
        <span className={`truncate ${accent ? "font-medium text-text" : ""}`}>{label}</span>
      )}
      {!collapsed && badge && (
        <span className="ml-auto shrink-0 rounded-full bg-sunken px-1.5 py-px text-fs-10 text-text-subtle">
          {badge}
        </span>
      )}
    </button>
  );
}
