import {
  Blocks,
  FolderOpen,
  Library,
  type LucideIcon,
  Map as MapIcon,
  PanelRightOpen,
} from "lucide-react";
import { Logo } from "../../components/Logo";
import { type MessageKey, useT } from "../../i18n";
import { useProjectStore } from "../../store/project";
import { useWorkspaceStore } from "../../store/workspace";
import { switchToSidebar } from "../leave";

function greeting(hour: number): MessageKey {
  if (hour >= 5 && hour < 12) return "shell.home.morning";
  if (hour >= 12 && hour < 18) return "shell.home.afternoon";
  return "shell.home.evening";
}

export function HomePage() {
  const go = useWorkspaceStore((s) => s.go);
  const { recent, select } = useProjectStore();
  const t = useT();

  const actions: { icon: LucideIcon; title: string; description: string; onClick: () => void }[] = [
    {
      icon: Blocks,
      title: t("shell.home.newClaudeMd"),
      description: t("shell.home.newClaudeMdDesc"),
      onClick: () => go("composer"),
    },
    {
      icon: MapIcon,
      title: t("shell.home.config"),
      description: t("shell.home.configDesc"),
      onClick: () => go("config"),
    },
    {
      icon: Library,
      title: t("shell.nav.library"),
      description: t("shell.home.libraryDesc"),
      onClick: () => go("library"),
    },
    {
      icon: PanelRightOpen,
      title: t("shell.switchToSidebar"),
      description: t("shell.home.sidebarDesc"),
      onClick: () => void switchToSidebar(),
    },
  ];

  return (
    <div className="mx-auto flex min-h-full max-w-2xl flex-col justify-center px-8 py-16">
      <div className="mb-2 flex items-center gap-3">
        <Logo size={40} className="shrink-0 drop-shadow-sm" />
        <h1 className="font-serif text-fs-32 leading-tight font-medium tracking-tight">
          {t(greeting(new Date().getHours()))}
        </h1>
      </div>
      <p className="mb-8 text-text-muted">{t("shell.home.prompt")}</p>

      <div className="grid grid-cols-2 gap-3">
        {actions.map(({ icon: Icon, title, description, onClick }) => (
          <button
            key={title}
            type="button"
            onClick={onClick}
            className="group rounded-xl border border-border bg-surface-raised p-4 text-left shadow-card transition-colors hover:border-border-strong"
          >
            <Icon
              size={18}
              className="mb-3 text-text-muted transition-colors group-hover:text-accent"
            />
            <div className="text-sm font-medium">{title}</div>
            <div className="mt-1 text-fs-13 leading-snug text-text-muted">{description}</div>
          </button>
        ))}
      </div>

      {recent.length > 0 && (
        <section className="mt-10">
          <h2 className="mb-2 px-1 text-xs font-medium text-text-subtle">
            {t("shell.home.recentProjects")}
          </h2>
          <ul className="space-y-0.5">
            {recent.slice(0, 5).map((path) => (
              <li key={path}>
                <button
                  type="button"
                  onClick={() => {
                    void select(path);
                    go("config");
                  }}
                  className="flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left text-sm hover:bg-hover"
                >
                  <FolderOpen size={15} className="shrink-0 text-text-subtle" />
                  <span className="max-w-[45%] shrink-0 truncate">{baseName(path)}</span>
                  <span className="min-w-0 flex-1 truncate text-right text-xs text-text-subtle">
                    {path}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
