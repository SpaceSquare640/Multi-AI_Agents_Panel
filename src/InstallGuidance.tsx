import { useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";

/** Tells the user how to install an external program, instead of
 *  installing it for them.
 *
 *  The app used to download vendor installers itself and launch them.
 *  That was removed deliberately: it fetched a 1.57 GB executable over
 *  plain `reqwest` with no hash or signature check, buffered the whole
 *  thing in memory, wrote it to the temp directory and executed it. An
 *  app that silently downloads and runs binaries is precisely the shape
 *  its own Guardrails warn about, and none of the verification that
 *  would make it safe was there.
 *
 *  So this component does two things and nothing else: it shows the
 *  official package-manager command to copy, and it opens the vendor's
 *  own download page. Both leave the user in control of what actually
 *  runs on their machine, and both are things they can verify first. */
export default function InstallGuidance({
  command,
  url,
}: {
  /** The winget line to run. Shown verbatim, never executed by the app. */
  command: string;
  /** The vendor's official download page. */
  url: string;
}) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      // Long enough to read, short enough that a second copy still
      // gives visible feedback rather than looking like nothing happened.
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard access can be refused. The command is on screen and
      // selectable either way, so there is nothing to recover from and
      // nothing worth interrupting the user about.
    }
  }

  return (
    <div className="install-guidance">
      <p className="acc-hint">{t("install.intro")}</p>
      <div className="install-guidance-command">
        <code>{command}</code>
        <button type="button" onClick={() => void copy()}>
          {copied ? t("install.copied") : t("install.copy")}
        </button>
      </div>
      <p className="acc-hint">
        {t("install.orDownload")}{" "}
        <button type="button" className="link-button" onClick={() => void openUrl(url)}>
          {url}
        </button>
      </p>
      <p className="acc-hint">{t("install.afterwards")}</p>
    </div>
  );
}
