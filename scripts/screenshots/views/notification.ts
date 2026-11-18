import type { Fixture } from "../mocks";

export const label = "notification";

interface PendingNotification {
  id: string;
  level: "success" | "error" | "info" | "warning";
  title: string;
  message: string | null;
  monochromatic: boolean;
  action: string | null;
}

type PendingHolder = { pendingNotifications: PendingNotification[] };

export const globals: PendingHolder = { pendingNotifications: [] };

export const fixture: Fixture = {
  drain_pending_notifications: () => {
    const holder = window as unknown as PendingHolder;
    const queued = holder.pendingNotifications;
    holder.pendingNotifications = [];
    return queued;
  },
  update_notification_window: null,
};
