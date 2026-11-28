import { useCallback, useEffect, useState } from "react";
import {
  AlertCircle,
  CheckCircle2,
  LayoutDashboard,
  ListFilter,
  Settings,
  SlidersHorizontal,
  Wifi,
  WifiOff,
} from "lucide-react";
import { DashboardTab } from "./components/DashboardTab";
import { RemapPlaceholderTab } from "./components/RemapPlaceholderTab";
import { RulesTab } from "./components/RulesTab";
import { SettingsTab } from "./components/SettingsTab";
import { useHub } from "./hooks/useHub";
import type { ToastMessage } from "./types";

type TabKey = "dashboard" | "rules" | "settings" | "remap";

export default function App() {
  const { snapshot, connected, error } = useHub();
  const [tab, setTab] = useState<TabKey>("dashboard");
  const [toast, setToast] = useState<ToastMessage | null>(null);

  const showToast = useCallback((next: ToastMessage) => {
    setToast(next);
  }, []);

  useEffect(() => {
    if (!toast) return;
    const id = window.setTimeout(() => setToast(null), 4000);
    return () => window.clearTimeout(id);
  }, [toast]);

  const navItems: Array<{ key: TabKey; label: string; icon: React.ReactNode }> =
    [
      {
        key: "dashboard",
        label: "ダッシュボード",
        icon: <LayoutDashboard className="w-4 h-4" />,
      },
      {
        key: "rules",
        label: "ルール設定",
        icon: <ListFilter className="w-4 h-4" />,
      },
      {
        key: "settings",
        label: "一般設定",
        icon: <Settings className="w-4 h-4" />,
      },
      {
        key: "remap",
        label: "リマップ",
        icon: <SlidersHorizontal className="w-4 h-4" />,
      },
    ];

  return (
    <div className="min-h-screen flex flex-col">
      {/* ヘッダー＆ナビゲーション */}
      <header className="sticky top-0 z-20 border-b border-slate-800 bg-slate-950/90 backdrop-blur">
        <div className="max-w-6xl mx-auto px-5 h-14 flex items-center justify-between gap-4">
          <div className="flex items-center gap-6">
            <div className="flex items-center gap-2.5">
              <div className="h-8 w-8 rounded-lg bg-slate-900 border border-sky-400/70 flex items-center justify-center font-bold text-sm text-sky-400">
                F
              </div>
              <span className="font-bold tracking-tight text-slate-100">
                F/HUB
              </span>
            </div>

            <nav className="flex items-center gap-1">
              {navItems.map((item) => {
                const active = tab === item.key;
                return (
                  <button
                    key={item.key}
                    type="button"
                    onClick={() => setTab(item.key)}
                    className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition cursor-pointer ${
                      active
                        ? "bg-sky-500/15 text-sky-300 border border-sky-500/30"
                        : "text-slate-400 hover:text-slate-100 hover:bg-slate-800/60 border border-transparent"
                    }`}
                  >
                    {item.icon}
                    {item.label}
                  </button>
                );
              })}
            </nav>
          </div>

          <span
            className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] font-medium border ${
              connected
                ? "bg-emerald-500/15 text-emerald-300 border-emerald-500/30"
                : "bg-rose-500/15 text-rose-300 border-rose-500/30"
            }`}
          >
            {connected ? (
              <Wifi className="w-3.5 h-3.5" />
            ) : (
              <WifiOff className="w-3.5 h-3.5" />
            )}
            {connected ? "接続中" : "再接続中..."}
          </span>
        </div>
      </header>

      {/* メインコンテンツ */}
      <main className="flex-1 max-w-6xl w-full mx-auto px-5 py-6 space-y-4">
        {error && (
          <div className="rounded-xl border border-rose-500/40 bg-rose-950/40 px-4 py-3 text-xs text-rose-200 flex items-center gap-2">
            <AlertCircle className="w-4 h-4 shrink-0 text-rose-400" />
            <span>{error}</span>
          </div>
        )}

        {!snapshot ? (
          <div className="py-16 text-center text-sm text-slate-400">
            バックエンドから状態を取得しています...
          </div>
        ) : (
          <>
            {tab === "dashboard" && (
              <DashboardTab snapshot={snapshot} onToast={showToast} />
            )}
            {tab === "rules" && (
              <RulesTab devices={snapshot.devices} onToast={showToast} />
            )}
            {tab === "settings" && (
              <SettingsTab
                configPath={snapshot.config_path}
                onToast={showToast}
              />
            )}
            {tab === "remap" && (
              <RemapPlaceholderTab devices={snapshot.devices} />
            )}
          </>
        )}
      </main>

      {/* 右下トースト通知 */}
      {toast && (
        <div
          className={`fixed bottom-5 right-5 z-30 max-w-sm rounded-xl border px-4 py-3 text-xs shadow-xl flex items-center gap-2 ${
            toast.kind === "success"
              ? "bg-slate-900 border-emerald-500/40 text-emerald-200"
              : "bg-slate-900 border-rose-500/40 text-rose-200"
          }`}
        >
          {toast.kind === "success" ? (
            <CheckCircle2 className="w-4 h-4 shrink-0 text-emerald-400" />
          ) : (
            <AlertCircle className="w-4 h-4 shrink-0 text-rose-400" />
          )}
          <span>{toast.text}</span>
        </div>
      )}
    </div>
  );
}
