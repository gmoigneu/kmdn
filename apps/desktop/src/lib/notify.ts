// OS notifications for three events only (D54): agent needs approval, agent finished while
// unfocused, review requested. Everything else stays a badge. Toggles are per-viewer conveniences.
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";

export type NotifyEvent = "agent_approval" | "agent_done" | "review_requested";

const KEY = "kmdn.notify";
export function getNotifyPrefs(): Record<NotifyEvent, boolean> {
  try { return { agent_approval: true, agent_done: true, review_requested: true, ...JSON.parse(localStorage.getItem(KEY) ?? "{}") }; }
  catch { return { agent_approval: true, agent_done: true, review_requested: true }; }
}
export function setNotifyPref(ev: NotifyEvent, on: boolean) {
  try { localStorage.setItem(KEY, JSON.stringify({ ...getNotifyPrefs(), [ev]: on })); } catch { /* storage may be unavailable */ }
}

let granted: boolean | null = null;
async function ensure(): Promise<boolean> {
  if (granted !== null) return granted;
  try {
    granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
  } catch { granted = false; }
  return granted;
}

/** Fires only when the window is unfocused for agent events; review requests always fire. */
export async function notify(ev: NotifyEvent, title: string, body: string) {
  if (!getNotifyPrefs()[ev]) return;
  if (ev !== "review_requested" && document.hasFocus()) return;
  if (!(await ensure())) return;
  try { sendNotification({ title, body }); } catch { /* best effort */ }
}
