import { TriangleAlert } from "lucide-react";
import { Component, type ErrorInfo, type ReactNode } from "react";
import { t } from "../i18n";
import { Button } from "./ui";

interface Props {
  children: ReactNode;
  /** Changing it clears the error (e.g. the page or tab shown). */
  resetKey?: string;
}

/**
 * Catches a render error in one page, so the rest of the window stays usable
 * instead of going blank, and shows what broke.
 */
export class ErrorBoundary extends Component<Props, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(error, info.componentStack);
  }

  componentDidUpdate(prev: Props) {
    if (this.state.error && prev.resetKey !== this.props.resetKey) this.setState({ error: null });
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div className="m-4 rounded-xl border border-danger/40 bg-danger/5 p-4 text-sm">
        <div className="mb-1 flex items-center gap-1.5 font-semibold text-danger">
          <TriangleAlert size={15} />
          {t("common.crash.title")}
        </div>
        <p className="mb-2 text-text-muted">{t("common.crash.hint")}</p>
        <pre className="mb-3 max-h-40 overflow-auto rounded-lg bg-sunken p-2 text-fs-11 whitespace-pre-wrap">
          {error.message}
        </pre>
        <Button size="sm" onClick={() => this.setState({ error: null })}>
          {t("common.crash.retry")}
        </Button>
      </div>
    );
  }
}
