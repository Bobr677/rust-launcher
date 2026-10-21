import { listen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Safe replacement for `unlisten = await listen(...)` inside effects.
 * If cleanup runs before the subscription resolves (fast tab switching remounts the tab),
 * the listener is dropped as soon as it resolves instead of leaking.
 */
export function subscribeTauriEvent<T>(
  event: string,
  handler: EventCallback<T>,
): () => void {
  let disposed = false;
  let unlisten: UnlistenFn | null = null;

  listen<T>(event, (e) => {
    if (!disposed) handler(e);
  })
    .then((fn) => {
      if (disposed) {
        try {
          fn();
        } catch {
        }
      } else {
        unlisten = fn;
      }
    })
    .catch((e) => console.error(`Failed to subscribe to "${event}":`, e));

  return () => {
    disposed = true;
    if (unlisten) {
      try {
        unlisten();
      } catch {
      }
      unlisten = null;
    }
  };
}

/** Same guarantee for subscriptions that resolve to an unlisten function (e.g. `onDragDropEvent`). */
export function disposeWhenResolved(subscription: Promise<UnlistenFn>): () => void {
  let disposed = false;
  let unlisten: UnlistenFn | null = null;

  subscription
    .then((fn) => {
      if (disposed) {
        try {
          fn();
        } catch {
        }
      } else {
        unlisten = fn;
      }
    })
    .catch(console.error);

  return () => {
    disposed = true;
    if (unlisten) {
      try {
        unlisten();
      } catch {
      }
      unlisten = null;
    }
  };
}
