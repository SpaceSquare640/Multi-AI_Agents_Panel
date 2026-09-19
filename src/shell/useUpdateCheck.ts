import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";

/** What the Rust `check_for_update` command returns. */
export interface UpdateCheckResult {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  releaseUrl: string;
  /** Whether the release being offered is marked as a prerelease. The
   *  check offers betas on purpose, so the UI labels them rather than
   *  presenting one as though it were a finished release. */
  isPrerelease: boolean;
}

/** Remembers the one version the user has already said no to.
 *
 *  A single version string rather than a list: dismissing 1.7.0 should
 *  not also silence 1.8.0, and once 1.8.0 exists nobody needs to
 *  remember what was said about 1.7.0. */
const DISMISSED_KEY = "updateCheck.dismissedVersion";

/** Once a day. Releases happen on the order of weeks, so a shorter
 *  interval would spend the user's network to learn nothing; the check
 *  at startup already covers almost every case where something changed
 *  while the app was closed. */
const POLL_INTERVAL_MS = 24 * 60 * 60 * 1000;

/**
 * Checks for a newer release at startup and once a day after that, and
 * reports one only if the user has not already dismissed that exact
 * version.
 *
 * Failures are deliberately silent. Being offline is the ordinary
 * condition of a desktop app, not an error worth a banner — and this
 * check runs on its own schedule rather than because the user asked, so
 * there is nobody waiting on an answer to report a failure to. The
 * manual button in Settings still surfaces its own errors, because there
 * someone did ask.
 *
 * Dismissing hides the notification but keeps `update` set, so the rail
 * can go on showing a quiet marker. Those are two different statements:
 * the notification says "look at this now", the marker says "there is
 * something here when you want it". Collapsing them would mean either
 * nagging on every launch or losing the information the moment the user
 * closes the card once.
 *
 * Nothing is downloaded or installed. The result is a link.
 */
export function useUpdateCheck(): {
  /** The available update, whether or not its notification was
   *  dismissed — this is what the rail marker reads. */
  update: UpdateCheckResult | null;
  /** True only while the notification itself should be on screen. */
  notify: boolean;
  dismiss: () => void;
} {
  const [update, setUpdate] = useState<UpdateCheckResult | null>(null);
  const [notify, setNotify] = useState(false);

  useEffect(() => {
    // Guards against a late response arriving after unmount, and against
    // the daily timer firing into a torn-down component.
    let cancelled = false;

    async function check() {
      try {
        const currentVersion = await getVersion();
        const result = await invoke<UpdateCheckResult>("check_for_update", { currentVersion });
        if (cancelled || !result.updateAvailable) return;

        let dismissed: string | null = null;
        try {
          dismissed = localStorage.getItem(DISMISSED_KEY);
        } catch {
          // Storage can be unavailable or blocked. Losing the dismissal
          // means the user is asked again, which is mildly annoying;
          // failing the whole check here would instead mean they are
          // never told about a new version at all.
        }
        setUpdate(result);
        // Already said no to this exact version: keep the marker, skip
        // the card.
        setNotify(dismissed !== result.latestVersion);
      } catch {
        // Silent on purpose — see this hook's doc comment.
      }
    }

    void check();
    const timer = setInterval(() => void check(), POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, []);

  const dismiss = useCallback(() => {
    setNotify(false);
    setUpdate((current) => {
      if (current) {
        try {
          localStorage.setItem(DISMISSED_KEY, current.latestVersion);
        } catch {
          // Same reasoning as above: an unwritable store costs the user a
          // repeat prompt next launch, nothing worse. Dismissing still
          // works for this session either way.
        }
      }
      // Kept, not cleared — the rail marker outlives the card.
      return current;
    });
  }, []);

  return { update, notify, dismiss };
}
