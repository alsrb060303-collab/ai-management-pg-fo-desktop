import { PageHeader } from "../../components/ui";
import { MemoPanel } from "../../features/memo/MemoPanel";
import { useT } from "../../i18n";

export function MemoPage() {
  const t = useT();
  return (
    <div className="mx-auto max-w-2xl px-8 py-10">
      <PageHeader title={t("memo.page.title")} description={t("memo.page.description")} />
      <MemoPanel />
    </div>
  );
}
