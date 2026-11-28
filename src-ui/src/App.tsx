import { useEffect, useState } from "react";
import { Activity, Battery, Cpu, Wifi, WifiOff } from "lucide-react";
import type { AppSnapshot } from "./types";

export default function App() {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;

    fetch("/api/state")
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json() as Promise<AppSnapshot>;
      })
      .then((data) => {
        if (active) {
          setSnapshot(data);
          setError(null);
        }
      })
      .catch((err) => {
        if (active) setError(`初期取得エラー: ${String(err)}`);
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
      } catch (e) {
        setError(`SSE パースエラー: ${String(e)}`);
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

  return (
    <div className="min-h-screen p-6 max-w-5xl mx-auto space-y-6">
      <header className="flex items-center justify-between border-b border-slate-800 pb-4">
        <div className="flex items-center gap-3">
          <div className="h-9 w-9 rounded-lg bg-slate-900 border border-sky-400/70 flex items-center justify-center font-bold text-sky-400">
            F
          </div>
          <div>
            <h1 className="text-xl font-bold tracking-tight">F/HUB</h1>
            <p className="text-xs text-slate-400">
              Step 3A-3 Vite + SSE 疎通確認
            </p>
          </div>
        </div>
        <span
          className={`inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium ${
            connected
              ? "bg-emerald-500/15 text-emerald-300 border border-emerald-500/30"
              : "bg-rose-500/15 text-rose-300 border border-rose-500/30"
          }`}
        >
          {connected ? (
            <Wifi className="w-3.5 h-3.5" />
          ) : (
            <WifiOff className="w-3.5 h-3.5" />
          )}
          {connected ? "SSE 接続中" : "未接続"}
        </span>
      </header>

      {error && (
        <div className="rounded-lg border border-rose-500/40 bg-rose-950/40 p-3 text-sm text-rose-200">
          {error}
        </div>
      )}

      {snapshot && (
        <>
          <section className="rounded-xl border border-slate-800 bg-slate-900/70 p-4 flex flex-wrap gap-6 text-sm">
            <div className="flex items-center gap-2">
              <Activity className="w-4 h-4 text-sky-400" />
              <span className="text-slate-400">前面アプリ:</span>
              <span className="font-mono font-medium">
                {snapshot.active_exe ?? "-"}
              </span>
            </div>
            <div className="flex items-center gap-2">
              <Cpu className="w-4 h-4 text-sky-400" />
              <span className="text-slate-400">適用ルール:</span>
              <span className="font-medium text-sky-300">
                {snapshot.active_rule}
              </span>
            </div>
            <div className="text-slate-400">
              自動切替:{" "}
              <span
                className={
                  snapshot.auto_switch ? "text-emerald-400" : "text-amber-400"
                }
              >
                {snapshot.auto_switch ? "ON" : "手動固定"}
              </span>
            </div>
          </section>

          <section className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {snapshot.devices.map((dev) => (
              <div
                key={dev.id}
                className="rounded-xl border border-slate-800 bg-slate-900/60 p-4 space-y-2"
              >
                <div className="flex items-center justify-between">
                  <h2 className="font-semibold text-sm">{dev.name}</h2>
                  <span className="text-xs px-2 py-0.5 rounded bg-slate-800 text-slate-300">
                    {dev.mode}
                  </span>
                </div>
                <div className="flex items-center justify-between text-xs text-slate-400">
                  <span className="inline-flex items-center gap-1">
                    <Battery className="w-3.5 h-3.5" />
                    {dev.capabilities.battery
                      ? dev.battery_percent !== null
                        ? `${dev.battery_percent}%`
                        : "取得中..."
                      : "非対応"}
                  </span>
                  <span>
                    Slot:{" "}
                    {dev.capabilities.profile
                      ? (dev.active_slot ?? "-")
                      : "非対応"}
                  </span>
                </div>
              </div>
            ))}
          </section>
        </>
      )}
    </div>
  );
}
