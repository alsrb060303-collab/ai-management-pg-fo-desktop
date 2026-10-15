import { PageHeader } from "../../components/ui";
import { LibraryPanel } from "../../features/docs/LibraryPanel";
import { ExtensionGallery } from "../../features/extensions/ExtensionGallery";
import { useT } from "../../i18n";

export function LibraryPage() {
  const t = useT();
  return (
    <div className="mx-auto max-w-3xl px-8 py-10">
      <PageHeader title={t("docs.view.library")} description={t("docs.libraryPage.description")} />
      <ExtensionGallery />
      <LibraryPanel />
    </div>
  );
}
