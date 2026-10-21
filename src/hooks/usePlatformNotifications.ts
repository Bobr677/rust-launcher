import { useCallback, useEffect, useState } from "react";
import {
  listNotifications,
  markAllNotificationsRead,
  markNotificationRead,
  type PlatformNotification,
} from "../api/notifications";
import { ApiError } from "../api/client";
import { WS_EVENT, type WsEvent } from "../api/ws";
import { usePlatformAccessToken } from "./usePlatformAccessToken";

export type PlatformNotificationsState = {
  signedIn: boolean;
  items: PlatformNotification[];
  unread: number;
  loading: boolean;
  error: string;
  reload: () => Promise<void>;
  markRead: (id: string) => Promise<void>;
  markAllRead: () => Promise<void>;
};

function errorText(e: unknown): string {
  return e instanceof ApiError ? e.message : e instanceof Error ? e.message : String(e);
}

export function usePlatformNotifications({
  enabled = true,
  onError,
}: { enabled?: boolean; onError?: (message: string) => void } = {}): PlatformNotificationsState {
  const signedIn = usePlatformAccessToken() !== "";
  const [items, setItems] = useState<PlatformNotification[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const reload = useCallback(async () => {
    if (!enabled || !signedIn) {
      setItems([]);
      return;
    }
    setLoading(true);
    setError("");
    try {
      setItems(await listNotifications());
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  }, [enabled, signedIn]);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    if (!enabled || !signedIn) return;
    const onWs = (ev: Event) => {
      const detail = (ev as CustomEvent<WsEvent>).detail;
      if (detail?.type === "notification") void reload();
    };
    window.addEventListener(WS_EVENT, onWs);
    return () => window.removeEventListener(WS_EVENT, onWs);
  }, [enabled, signedIn, reload]);

  const markRead = useCallback(
    async (id: string) => {
      const readAt = new Date().toISOString();
      setItems((prev) => prev.map((n) => (n.id === id ? { ...n, read_at: n.read_at ?? readAt } : n)));
      try {
        await markNotificationRead(id);
      } catch (e) {
        onError?.(errorText(e));
        void reload();
      }
    },
    [onError, reload],
  );

  const markAllRead = useCallback(async () => {
    const readAt = new Date().toISOString();
    setItems((prev) => prev.map((n) => (n.read_at ? n : { ...n, read_at: readAt })));
    try {
      await markAllNotificationsRead();
    } catch (e) {
      onError?.(errorText(e));
      void reload();
    }
  }, [onError, reload]);

  const unread = items.filter((n) => !n.read_at).length;

  return { signedIn, items, unread, loading, error, reload, markRead, markAllRead };
}
