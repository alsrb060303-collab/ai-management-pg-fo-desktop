import { ask } from "@tauri-apps/plugin-dialog";
import {
  Blocks,
  FileText,
  GitBranch,
  House,
  Library,
  type LucideIcon,
  Map as MapIcon,
  NotebookPen,
  PanelLeft,
  PanelRightOpen,
  Plus,
  Settings,
  Trash2,
} from "lucide-react";
import { Logo } from "../components/Logo";
import { IconButton } from "../components/ui";
import { type MessageKey, useT } from "../i18n";
import { useComposerStore } from "../store/composer";
import { type Page, useWorkspaceStore } from "../store/workspace";
import { switchToSidebar } from "./leave";

const NAV: { page: Page; label: MessageKey; icon: LucideIcon; soon?: boolean }[] = [
  { page: "home", label: "shell.nav.home", icon: House },
  { page: "composer", label: "shell.nav.composer", icon: Blocks },
  { page: "library", label: "shell.nav.library", icon: Library },
  { page: "config", label: "shell.nav.config", icon: MapIcon },
  { page: "files", label: "shell.nav.files", icon: FileText },
  { page: "work", label: "shell.nav.work", icon: GitBranch },
  { page: "memo", label: "shell.nav.memo", icon: NotebookPen },
];

export function NavSidebar() {
  const { page, collapsed, go, toggleCollapsed } = useWorkspaceStore();
  const docs = useComposerStore((s) => s.docs);
  const openId = useComposerStore((s) => s.doc?.id);
  const { newDoc, openDoc, deleteDoc } = useComposerStore.getState();
  const t = useT();

  return (
    <aside
      className={`flex shrink-0 flex-col border-r border-border bg-sidebar transition-[width] duration-200 ${
        collapsed ? "w-[52px]" : "w-64"
      }`}
    >
      <div className="flex h-12 shrink-0 items-center gap-1 px-2.5">
        {!collapsed && (
          <button
            type="button"
            onClick={() => go("home")}
            className="flex min-w-0 items-center gap-2 rounded-md px-1.5 font-serif text-[17px] font-semibold tracking-tight"
          >
            <Logo size={20} />
            <span className="truncate">AI Management</span>
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
            <h3 className="px-2 pb-1 text-xs font-medium text-text-subtle">
              {t("shell.nav.recentDocs")}
            </h3>
            {docs.length === 0 ? (
              <p className="px-2 text-xs text-text-subtle">{t("shell.nav.noDocs")}</p>
            ) : (
              <ul className="space-y-px">
                {docs.slice(0, 20).map((d) => {
                  const active = page === "composer" && d.id === openId;
                  return (
                    <li key={d.id} className="group relative">
                      <button
                        type="button"
                        onClick={() => {
                          void openDoc(d.id);
                          go("composer");
                        }}
                        className={`w-full truncate rounded-lg py-1.5 pr-8 pl-2 text-left text-[13px] transition-colors ${
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
        <span className="ml-auto shrink-0 rounded-full bg-sunken px-1.5 py-px text-[10px] text-text-subtle">
          {badge}
        </span>
      )}
    </button>
  );
}
