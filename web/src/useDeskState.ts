import { useEffect, useRef, useState } from 'react';
import { fetchState } from './api';
import type { DeskState } from './types';

export type ConnectionState = 'connecting' | 'online' | 'offline';

export interface DeskStateResult {
  state: DeskState | null;
  connection: ConnectionState;
  lastUpdate: number;
}

/**
 * Subscribes to the desk's SSE stream and falls back to polling when the
 * stream cannot be established. EventSource reconnects on its own, so polling
 * is stopped again as soon as the stream opens.
 */
export function useDeskState(): DeskStateResult {
  const [state, setState] = useState<DeskState | null>(null);
  const [connection, setConnection] = useState<ConnectionState>('connecting');
  const [lastUpdate, setLastUpdate] = useState(0);
  const pollRef = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    let events: EventSource | null = null;

    const apply = (next: DeskState) => {
      if (cancelled) return;
      setState(next);
      setConnection('online');
      setLastUpdate(Date.now());
    };

    const stopPolling = () => {
      if (pollRef.current !== null) {
        window.clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };

    const startPolling = () => {
      if (pollRef.current !== null) return;
      pollRef.current = window.setInterval(async () => {
        try {
          apply(await fetchState());
        } catch {
          if (!cancelled) setConnection('offline');
        }
      }, 1000);
    };

    fetchState()
      .then(apply)
      .catch(() => {
        if (!cancelled) setConnection('offline');
      });

    events = new EventSource('/api/events');
    events.addEventListener('state', (event) => {
      try {
        apply(JSON.parse(event.data) as DeskState);
      } catch {
        // Ignore malformed events. The next state event will correct it.
      }
    });
    events.onopen = () => {
      if (cancelled) return;
      setConnection('online');
      stopPolling();
    };
    events.onerror = () => {
      if (cancelled) return;
      setConnection('offline');
      startPolling();
    };

    return () => {
      cancelled = true;
      stopPolling();
      events?.close();
    };
  }, []);

  return { state, connection, lastUpdate };
}
