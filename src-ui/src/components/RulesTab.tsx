import { useEffect, useState } from "react";
import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Trash2,
  X,
} from "lucide-react";
import { ApiError, fetchConfig, fetchProcesses, saveConfig } from "../api";
import type {
  Config,
  DeviceStateSnapshot,
  RuleConfig,
  ToastMessage,
} from "../types";

interface RulesTabProps {
  devices: DeviceStateSnapshot[];
  onToast: (toast: ToastMessage) => void;
}

function slotOptions(slots: [number, number] | null): number[] {
  if (!slots) return [];
  const [min, max] = slots;
  const out: number[] = [];
  for (let i = min; i <= max; i += 1) out.push(i);
  return out;
}

export function RulesTab({ devices, onToast }: RulesTabProps) {
  const profileDevices = devices.filter((d) => d.capabilities.profile);

  const [config, setConfig] = useState<Config | null>(null);
  const [processes, setProcesses] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [validationErrors, setValidationErrors] = useState<string[]>([]);
  const [exeDrafts, setExeDrafts] = useState<Record<number, string>>({});

  useEffect(() => {
    let active = true;
    Promise.all([fetchConfig(), fetchProcesses()])
      .then(([cfg, procs]) => {
        if (!active) return;
        setConfig(cfg);
        setProcesses(procs);
        setDirty(false);
        setValidationErrors([]);
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
      const [cfg, procs] = await Promise.all([fetchConfig(), fetchProcesses()]);
      setConfig(cfg);
      setProcesses(procs);
      setDirty(false);
      setValidationErrors([]);
    } catch (err) {
      const msg =
        err instanceof ApiError ? err.errors.join(" / ") : String(err);
      onToast({ kind: "error", text: `設定読込エラー: ${msg}` });
    } finally {
      setLoading(false);
    }
  };

  const reloadProcesses = async () => {
    try {
      const procs = await fetchProcesses();
      setProcesses(procs);
    } catch (err) {
      onToast({ kind: "error", text: `プロセス取得失敗: ${String(err)}` });
    }
  };

  const updateConfig = (updater: (prev: Config) => Config) => {
    setConfig((prev) => (prev ? updater(prev) : prev));
    setDirty(true);
  };

  const setDefaultSlot = (deviceId: string, rawVal: string) => {
    updateConfig((prev) => {
      const nextDefault = { ...prev.default };
      if (rawVal === "") {
        delete nextDefault[deviceId];
      } else {
        nextDefault[deviceId] = Number(rawVal);
      }
      return { ...prev, default: nextDefault };
    });
  };

  const addRule = () => {
    const firstDev = profileDevices[0];
    const initialProfiles: Record<string, number> = {};
    if (firstDev && firstDev.capabilities.profile_slots) {
      initialProfiles[firstDev.id] = firstDev.capabilities.profile_slots[0];
    }
    const newRule: RuleConfig = {
      name: `ルール ${(config?.rules.length ?? 0) + 1}`,
      match_exe: [],
      profiles: initialProfiles,
    };
    updateConfig((prev) => ({ ...prev, rules: [...prev.rules, newRule] }));
  };

  const updateRule = (
    idx: number,
    updater: (rule: RuleConfig) => RuleConfig,
  ) => {
    updateConfig((prev) => {
      const rules = prev.rules.map((r, i) => (i === idx ? updater(r) : r));
      return { ...prev, rules };
    });
  };

  const removeRule = (idx: number) => {
    updateConfig((prev) => ({
      ...prev,
      rules: prev.rules.filter((_, i) => i !== idx),
    }));
  };

  const moveRule = (idx: number, delta: -1 | 1) => {
    updateConfig((prev) => {
      const target = idx + delta;
      if (target < 0 || target >= prev.rules.length) return prev;
      const rules = [...prev.rules];
      const [item] = rules.splice(idx, 1);
      rules.splice(target, 0, item);
      return { ...prev, rules };
    });
  };

  const addExeToRule = (idx: number, rawExe: string) => {
    const exe = rawExe.trim();
    if (!exe) return;
    updateRule(idx, (rule) => {
      if (rule.match_exe.some((e) => e.toLowerCase() === exe.toLowerCase())) {
        return rule;
      }
      return { ...rule, match_exe: [...rule.match_exe, exe] };
    });
    setExeDrafts((prev) => ({ ...prev, [idx]: "" }));
  };

  const removeExeFromRule = (idx: number, exeIndex: number) => {
    updateRule(idx, (rule) => ({
      ...rule,
      match_exe: rule.match_exe.filter((_, i) => i !== exeIndex),
    }));
  };

  const setRuleProfile = (idx: number, deviceId: string, rawVal: string) => {
    updateRule(idx, (rule) => {
      const profiles = { ...rule.profiles };
      if (rawVal === "") {
        delete profiles[deviceId];
      } else {
        profiles[deviceId] = Number(rawVal);
      }
      return { ...rule, profiles };
    });
  };

  const handleSave = async () => {
    if (!config) return;
    setSaving(true);
    setValidationErrors([]);
    try {
      const res = await saveConfig(config);
      setDirty(false);
      onToast({ kind: "success", text: res.message });
    } catch (err) {
      if (err instanceof ApiError) {
        setValidationErrors(err.errors);
        onToast({
          kind: "error",
          text: "設定検証エラーがあります。赤枠の内容を確認してください。",
        });
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
    <div className="space-y-6">
      {/* 操作バー */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-slate-100">
            プロファイル自動切替ルール
          </h2>
          <p className="text-xs text-slate-400">
            上から順に評価され、最初に一致したルールを適用します（未一致時はデフォルトへ復帰）。
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

      {/* バリデーションエラー表示 */}
      {validationErrors.length > 0 && (
        <div className="rounded-xl border border-rose-500/40 bg-rose-950/40 p-4 space-y-1.5 text-xs text-rose-200">
          <div className="flex items-center gap-1.5 font-semibold text-rose-300">
            <AlertCircle className="w-4 h-4" />
            設定内容の検証エラー
          </div>
          <ul className="list-disc pl-5 space-y-1">
            {validationErrors.map((msg, i) => (
              <li key={i}>{msg}</li>
            ))}
          </ul>
        </div>
      )}

      {/* デフォルトプロファイル */}
      <section className="rounded-xl border border-slate-800 bg-slate-900/60 p-4 space-y-3">
        <div>
          <h3 className="text-sm font-semibold text-slate-100">
            デフォルトプロファイル ([default])
          </h3>
          <p className="text-xs text-slate-400">
            どのルールにも一致しないアプリが最前面にあるときに維持するスロット番号です。
          </p>
        </div>
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          {profileDevices.map((dev) => (
            <label
              key={dev.id}
              className="flex flex-col gap-1.5 rounded-lg border border-slate-800 bg-slate-950/60 p-3 text-xs"
            >
              <span className="font-medium text-slate-200">{dev.name}</span>
              <select
                value={config.default[dev.id] ?? ""}
                onChange={(e) => setDefaultSlot(dev.id, e.target.value)}
                className="rounded-md border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-slate-100 focus:border-sky-500 focus:outline-none"
              >
                <option value="">指定なし</option>
                {slotOptions(dev.capabilities.profile_slots).map((slot) => (
                  <option key={slot} value={slot}>
                    Slot {slot}
                  </option>
                ))}
              </select>
            </label>
          ))}
        </div>
      </section>

      {/* ルール一覧 */}
      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-slate-100">
            アプリ別ルール（{config.rules.length} 件）
          </h3>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={reloadProcesses}
              className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs border border-slate-700 bg-slate-900 hover:bg-slate-800 text-slate-300 cursor-pointer"
            >
              <RefreshCw className="w-3.5 h-3.5" />
              起動中プロセス更新 ({processes.length})
            </button>
            <button
              type="button"
              onClick={addRule}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-slate-800 hover:bg-slate-700 border border-slate-700 text-sky-300 cursor-pointer"
            >
              <Plus className="w-3.5 h-3.5" />
              ルールを追加
            </button>
          </div>
        </div>

        {config.rules.length === 0 ? (
          <div className="rounded-xl border border-dashed border-slate-800 p-8 text-center text-xs text-slate-400">
            ルールがありません。「ルールを追加」からアプリごとの切替設定を作成できます。
          </div>
        ) : (
          <div className="space-y-3">
            {config.rules.map((rule, idx) => (
              <div
                key={idx}
                className="rounded-xl border border-slate-800 bg-slate-900/60 p-4 space-y-4"
              >
                {/* ルール見出し行 */}
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2 flex-1 min-w-52">
                    <span className="text-xs font-mono text-slate-500">
                      #{idx + 1}
                    </span>
                    <input
                      type="text"
                      value={rule.name}
                      onChange={(e) =>
                        updateRule(idx, (r) => ({ ...r, name: e.target.value }))
                      }
                      placeholder="ルール名（例: 開発作業）"
                      className="flex-1 rounded-md border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-sm font-medium text-slate-100 focus:border-sky-500 focus:outline-none"
                    />
                  </div>
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      disabled={idx === 0}
                      onClick={() => moveRule(idx, -1)}
                      title="優先度を上げる"
                      className="p-1.5 rounded-md border border-slate-700 bg-slate-800 hover:bg-slate-700 text-slate-300 disabled:opacity-30 cursor-pointer"
                    >
                      <ArrowUp className="w-3.5 h-3.5" />
                    </button>
                    <button
                      type="button"
                      disabled={idx === config.rules.length - 1}
                      onClick={() => moveRule(idx, 1)}
                      title="優先度を下げる"
                      className="p-1.5 rounded-md border border-slate-700 bg-slate-800 hover:bg-slate-700 text-slate-300 disabled:opacity-30 cursor-pointer"
                    >
                      <ArrowDown className="w-3.5 h-3.5" />
                    </button>
                    <button
                      type="button"
                      onClick={() => removeRule(idx)}
                      title="ルールを削除"
                      className="p-1.5 rounded-md border border-rose-500/30 bg-rose-950/40 hover:bg-rose-900/50 text-rose-300 cursor-pointer ml-1"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>

                {/* 対象プロセス (match_exe) */}
                <div className="space-y-2">
                  <span className="text-xs text-slate-400">
                    対象プロセス (.exe)
                  </span>
                  <div className="flex flex-wrap gap-1.5">
                    {rule.match_exe.map((exe, exeIdx) => (
                      <span
                        key={`${exe}-${exeIdx}`}
                        className="inline-flex items-center gap-1 rounded-md bg-sky-500/15 border border-sky-500/30 px-2 py-0.5 text-xs font-mono text-sky-200"
                      >
                        {exe}
                        <button
                          type="button"
                          onClick={() => removeExeFromRule(idx, exeIdx)}
                          className="text-sky-300/70 hover:text-rose-300 cursor-pointer"
                        >
                          <X className="w-3 h-3" />
                        </button>
                      </span>
                    ))}
                    {rule.match_exe.length === 0 && (
                      <span className="text-xs text-amber-400/90">
                        ※1件以上の実行ファイル名を追加してください
                      </span>
                    )}
                  </div>

                  <div className="flex flex-wrap gap-2">
                    <div className="flex items-center gap-1.5 flex-1 min-w-56">
                      <input
                        type="text"
                        value={exeDrafts[idx] ?? ""}
                        onChange={(e) =>
                          setExeDrafts((prev) => ({
                            ...prev,
                            [idx]: e.target.value,
                          }))
                        }
                        onKeyDown={(e) => {
                          if (e.key === "Enter") {
                            e.preventDefault();
                            addExeToRule(idx, exeDrafts[idx] ?? "");
                          }
                        }}
                        placeholder="例: Code.exe を入力して Enter"
                        className="flex-1 rounded-md border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs font-mono text-slate-100 focus:border-sky-500 focus:outline-none"
                      />
                      <button
                        type="button"
                        onClick={() => addExeToRule(idx, exeDrafts[idx] ?? "")}
                        className="px-2.5 py-1.5 rounded-md border border-slate-700 bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 cursor-pointer"
                      >
                        追加
                      </button>
                    </div>

                    <select
                      value=""
                      onChange={(e) => {
                        if (e.target.value) addExeToRule(idx, e.target.value);
                      }}
                      className="rounded-md border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-300 focus:border-sky-500 focus:outline-none"
                    >
                      <option value="">＋ 起動中ウィンドウから選択...</option>
                      {processes.map((proc) => (
                        <option key={proc} value={proc}>
                          {proc}
                        </option>
                      ))}
                    </select>
                  </div>
                </div>

                {/* 適用プロファイル */}
                <div className="space-y-2 pt-2 border-t border-slate-800/80">
                  <span className="text-xs text-slate-400">
                    適用するプロファイル
                  </span>
                  <div className="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
                    {profileDevices.map((dev) => (
                      <label
                        key={dev.id}
                        className="flex items-center justify-between gap-2 rounded-lg border border-slate-800 bg-slate-950/60 px-3 py-2 text-xs"
                      >
                        <span className="text-slate-300 truncate">
                          {dev.name}
                        </span>
                        <select
                          value={rule.profiles[dev.id] ?? ""}
                          onChange={(e) =>
                            setRuleProfile(idx, dev.id, e.target.value)
                          }
                          className="rounded border border-slate-700 bg-slate-900 px-2 py-1 text-slate-100 focus:border-sky-500 focus:outline-none"
                        >
                          <option value="">デフォルト維持</option>
                          {slotOptions(dev.capabilities.profile_slots).map(
                            (slot) => (
                              <option key={slot} value={slot}>
                                Slot {slot}
                              </option>
                            ),
                          )}
                        </select>
                      </label>
                    ))}
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
