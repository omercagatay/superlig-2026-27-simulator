import { Component, type ReactNode } from "react";
import { useT } from "../i18n";

function Crashed() {
  const t = useT();
  return (
    <main>
      <p role="alert">{t("crashed")}</p>
      <button type="button" className="btn btn-primary" onClick={() => window.location.reload()}>
        {t("reload")}
      </button>
    </main>
  );
}

/** A render error anywhere below shows a reload prompt instead of a blank
 *  page. React still logs the error itself. */
export class ErrorBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  render() {
    return this.state.failed ? <Crashed /> : this.props.children;
  }
}
