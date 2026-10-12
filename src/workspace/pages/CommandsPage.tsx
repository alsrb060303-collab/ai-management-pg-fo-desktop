import { PageHeader } from "../../components/ui";
import { CommandsPanel } from "../../features/commands/CommandsPanel";
import { useT } from "../../i18n";

export function CommandsPage() {
  const t = useT();
  return (
    <div className="mx-auto max-w-3xl px-8 py-10">
      <PageHeader title={t("commands.title")} description={t("commands.description")} />
      <CommandsPanel />
    </div>
  );
}
