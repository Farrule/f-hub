import { useEffect, useState } from "react";
import { fetchState } from "../api";
import type { AppSnapshot } from "../types";

export function useHub() {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;

    fetchState()
      .then((data) => {
        if (active) {
          setSnapshot(data);
          setError(null);
        }
      })
      .catch((err) => {
        if (active) setError(`初期状態の取得に失敗しました: ${String(err)}`);
      });

    const es = new EventSource("/api/events");
    es.onopen = () => {
      if (active) {
        setConnected(true);
        setError(null);
      }
    };
    es.addEventListener("state", (ev) => {
      if (!active) return;
      try {
        const data = JSON.parse((ev as MessageEvent).data) as AppSnapshot;
        setSnapshot(data);
        setConnected(true);
        setError(null);
      } catch (e) {
        setError(`SSE データの解析に失敗しました: ${String(e)}`);
      }
    });
    es.onerror = () => {
      if (active) setConnected(false);
    };

    return () => {
      active = false;
      es.close();
    };
  }, []);

  return { snapshot, connected, error };
}
