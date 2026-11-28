import { useEffect, useState } from "react";
import { AlertCircle, FileCode, RotateCcw, Save } from "lucide-react";
import { ApiError, fetchConfig, saveConfig } from "../api";
import type { Config, ToastMessage } from "../types";

interface SettingsTabProps {
  configPath: string;
  onToast: (toast: ToastMessage) => void;
}

export function SettingsTab({ configPath, onToast }: SettingsTabProps) {
  const [config, setConfig] = useState<Config | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [errors, setErrors] = useState<string[]>([]);

  useEffect(() => {
    let active = true;
    fetchConfig()
      .then((cfg) => {
        if (!active) return;
        setConfig(cfg);
        setDirty(false);
        setErrors([]);
      })
      .catch((err) => {
        if (!active) return;
        const msg =
          err instanceof ApiError ? err.errors.join(" / ") : String(err);
        onToast({ kind: "error", text: `設定読込エラー: ${msg}` });
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, [onToast]);

  const handleReload = async () => {
    setLoading(true);
    try {
      const cfg = await fetchConfig();
      setConfig(cfg);
      setDirty(false);
      setErrors([]);
    } catch (err) {
      const msg =
        err instanceof ApiError ? err.errors.join(" / ") : String(err);
      onToast({ kind: "error", text: `設定読込エラー: ${msg}` });
    } finally {
      setLoading(false);
    }
  };

  const handleSave = async () => {
    if (!config) return;
    setSaving(true);
    setErrors([]);
    try {
      const res = await saveConfig(config);
      setDirty(false);
      onToast({ kind: "success", text: res.message });
    } catch (err) {
      if (err instanceof ApiError) {
        setErrors(err.errors);
        onToast({ kind: "error", text: "入力値にエラーがあります" });
      } else {
        onToast({ kind: "error", text: `保存失敗: ${String(err)}` });
      }
    } finally {
      setSaving(false);
    }
  };

  if (loading || !config) {
    return (
      <div className="text-sm text-slate-400 py-8 text-center">
        設定を読み込んでいます...
      </div>
    );
  }

  return (
    <div className="space-y-6 max-w-2xl">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-slate-100">
            一般設定 ([general])
          </h2>
          <p className="text-xs text-slate-400">
            保存するとデーモンを再起動せず即時反映されます。
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={handleReload}
            disabled={saving}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium border border-slate-700 bg-slate-800 hover:bg-slate-700 text-slate-200 cursor-pointer disabled:opacity-50"
          >
            <RotateCcw className="w-3.5 h-3.5" />
            再読込
          </button>
          <button
            type="button"
            onClick={handleSave}
            disabled={saving || !dirty}
            className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-semibold bg-sky-500 hover:bg-sky-400 text-slate-950 transition cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
          >
            <Save className="w-3.5 h-3.5" />
            {saving ? "保存中..." : "保存して即時反映"}
          </button>
        </div>
      </div>

      {errors.length > 0 && (
        <div className="rounded-xl border border-rose-500/40 bg-rose-950/40 p-4 space-y-1.5 text-xs text-rose-200">
          <div className="flex items-center gap-1.5 font-semibold text-rose-300">
            <AlertCircle className="w-4 h-4" />
            検証エラー
          </div>
          <ul className="list-disc pl-5 space-y-1">
            {errors.map((e, i) => (
              <li key={i}>{e}</li>
            ))}
          </ul>
        </div>
      )}

      <section className="rounded-xl border border-slate-800 bg-slate-900/60 p-5 space-y-4">
        <label className="block space-y-1.5">
          <div className="flex items-center justify-between text-xs">
            <span className="font-medium text-slate-200">
              ウィンドウ切替デバウンス (ms)
            </span>
            <span className="text-slate-500">範囲: 50 〜 5000 ms</span>
          </div>
          <input
            type="number"
            min={50}
            max={5000}
            value={config.general.debounce_ms}
            onChange={(e) => {
              setConfig({
                ...config,
                general: {
                  ...config.general,
                  debounce_ms: Number(e.target.value),
                },
              });
              setDirty(true);
            }}
            className="w-40 rounded-md border border-slate-700 bg-slate-950 px-3 py-1.5 text-sm font-mono text-slate-100 focus:border-sky-500 focus:outline-none"
          />
          <p className="text-[11px] text-slate-400">
            Alt+Tab
            などの素早い切替を無視し、同じアプリが留まったあとにプロファイルを切り替える待機時間です。
          </p>
        </label>

        <label className="block space-y-1.5 pt-3 border-t border-slate-800/80">
          <div className="flex items-center justify-between text-xs">
            <span className="font-medium text-slate-200">
              バッテリー監視間隔 (秒)
            </span>
            <span className="text-slate-500">範囲: 10 〜 3600 秒</span>
          </div>
          <input
            type="number"
            min={10}
            max={3600}
            value={config.general.battery_poll_sec}
            onChange={(e) => {
              setConfig({
                ...config,
                general: {
                  ...config.general,
                  battery_poll_sec: Number(e.target.value),
                },
              });
              setDirty(true);
            }}
            className="w-40 rounded-md border border-slate-700 bg-slate-950 px-3 py-1.5 text-sm font-mono text-slate-100 focus:border-sky-500 focus:outline-none"
          />
          <p className="text-[11px] text-slate-400">
            対応 4 デバイスから定期的にバッテリー残量を取得する間隔です（推奨:
            60〜180 秒）。
          </p>
        </label>

        <label className="block space-y-1.5 pt-3 border-t border-slate-800/80">
          <div className="flex items-center justify-between text-xs">
            <span className="font-medium text-slate-200">
              低バッテリー通知の閾値 (%)
            </span>
            <span className="text-slate-500">範囲: 1 〜 99 %</span>
          </div>
          <input
            type="number"
            min={1}
            max={99}
            value={config.general.low_battery_threshold}
            onChange={(e) => {
              setConfig({
                ...config,
                general: {
                  ...config.general,
                  low_battery_threshold: Number(e.target.value),
                },
              });
              setDirty(true);
            }}
            className="w-40 rounded-md border border-slate-700 bg-slate-950 px-3 py-1.5 text-sm font-mono text-slate-100 focus:border-sky-500 focus:outline-none"
          />
          <p className="text-[11px] text-slate-400">
            放電中にこの残量以下になると OS 通知を 1 回発行し、充電開始または
            +5% 回復まで再通知を抑止します。
          </p>
        </label>
      </section>

      <section className="rounded-xl border border-slate-800 bg-slate-900/40 p-4 flex items-start gap-2.5 text-xs">
        <FileCode className="w-4 h-4 text-slate-400 shrink-0 mt-0.5" />
        <div className="space-y-1 min-w-0">
          <p className="text-slate-400">使用中の設定ファイルパス</p>
          <p className="font-mono text-slate-200 break-all">{configPath}</p>
        </div>
      </section>
    </div>
  );
}
