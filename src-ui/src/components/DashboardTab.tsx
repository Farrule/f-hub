import { useState } from "react";
import {
  Activity,
  AlertTriangle,
  Battery,
  BatteryCharging,
  Cpu,
  Disc,
  Keyboard,
  Lock,
  Mouse,
  RefreshCw,
  Unlock,
  Usb,
  Wifi,
  WifiOff,
} from "lucide-react";
import {
  ApiError,
  refreshBatteryNow,
  setAutoSwitchMode,
  switchDeviceProfile,
} from "../api";
import type { AppSnapshot, DeviceStateSnapshot, ToastMessage } from "../types";

interface DashboardTabProps {
  snapshot: AppSnapshot;
  onToast: (toast: ToastMessage) => void;
}

type NormalizedMode = "wired" | "wireless" | "disconnected";

function normalizeMode(raw: string): NormalizedMode {
  const key = raw.toLowerCase().replace(/[^a-z0-9]/g, "");
  if (key.includes("wired") || key === "usb") return "wired";
  if (key.includes("wireless") || key.includes("24g") || key.includes("bt"))
    return "wireless";
  return "disconnected";
}

function modeBadge(raw: string) {
  const mode = normalizeMode(raw);
  if (mode === "wired") {
    return {
      label: "有線 USB",
      className: "bg-sky-500/15 text-sky-300 border-sky-500/30",
      icon: <Usb className="w-3 h-3" />,
    };
  }
  if (mode === "wireless") {
    return {
      label: "2.4G 無線",
      className: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
      icon: <Wifi className="w-3 h-3" />,
    };
  }
  return {
    label: "未接続",
    className: "bg-slate-800 text-slate-400 border-slate-700",
    icon: <WifiOff className="w-3 h-3" />,
  };
}

function deviceIcon(kind: string) {
  const k = kind.toLowerCase();
  if (k.includes("mouse")) return <Mouse className="w-4 h-4 text-sky-400" />;
  if (k.includes("keyboard"))
    return <Keyboard className="w-4 h-4 text-sky-400" />;
  if (k.includes("trackball")) return <Disc className="w-4 h-4 text-sky-400" />;
  return <Cpu className="w-4 h-4 text-sky-400" />;
}

function batteryBarColor(percent: number, isCharging: boolean): string {
  if (isCharging) return "bg-sky-400";
  if (percent <= 20) return "bg-rose-500";
  if (percent <= 40) return "bg-amber-400";
  return "bg-emerald-400";
}

function slotRange(slots: [number, number] | null): number[] {
  if (!slots) return [];
  const [min, max] = slots;
  const list: number[] = [];
  for (let i = min; i <= max; i += 1) {
    list.push(i);
  }
  return list;
}

export function DashboardTab({ snapshot, onToast }: DashboardTabProps) {
  const [lockOnManual, setLockOnManual] = useState(true);
  const [busyMode, setBusyMode] = useState(false);
  const [busyRefresh, setBusyRefresh] = useState(false);
  const [switchingDevice, setSwitchingDevice] = useState<string | null>(null);

  const handleToggleAuto = async () => {
    setBusyMode(true);
    try {
      const next = !snapshot.auto_switch;
      await setAutoSwitchMode(next);
      onToast({
        kind: "success",
        text: next
          ? "自動切替を再開しました"
          : "手動固定モード（自動切替一時停止）にしました",
      });
    } catch (err) {
      const msg =
        err instanceof ApiError ? err.errors.join(" / ") : String(err);
      onToast({ kind: "error", text: `モード変更失敗: ${msg}` });
    } finally {
      setBusyMode(false);
    }
  };

  const handleRefreshBattery = async () => {
    setBusyRefresh(true);
    try {
      const res = await refreshBatteryNow();
      onToast({ kind: "success", text: res.message });
    } catch (err) {
      const msg =
        err instanceof ApiError ? err.errors.join(" / ") : String(err);
      onToast({ kind: "error", text: `バッテリー更新失敗: ${msg}` });
    } finally {
      setTimeout(() => setBusyRefresh(false), 500);
    }
  };

  const handleSwitchProfile = async (
    dev: DeviceStateSnapshot,
    slot: number,
  ) => {
    setSwitchingDevice(dev.id);
    try {
      const res = await switchDeviceProfile(dev.id, slot, lockOnManual);
      onToast({ kind: "success", text: `${dev.name}: ${res.message}` });
    } catch (err) {
      const msg =
        err instanceof ApiError ? err.errors.join(" / ") : String(err);
      onToast({ kind: "error", text: `${dev.name} 切替失敗: ${msg}` });
    } finally {
      setSwitchingDevice(null);
    }
  };

  const updatedLabel = new Date(snapshot.updated_at_ms).toLocaleTimeString(
    "ja-JP",
  );

  return (
    <div className="space-y-5">
      {/* 監視ステータスバー */}
      <section className="rounded-xl border border-slate-800 bg-slate-900/70 p-4 flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
        <div className="flex flex-wrap items-center gap-x-6 gap-y-2 text-sm">
          <div className="flex items-center gap-2">
            <Activity className="w-4 h-4 text-sky-400" />
            <span className="text-slate-400">前面アプリ</span>
            <span className="font-mono font-medium text-slate-100 bg-slate-800/80 px-2 py-0.5 rounded">
              {snapshot.active_exe ?? "-"}
            </span>
          </div>
          <div className="flex items-center gap-2">
            <Cpu className="w-4 h-4 text-sky-400" />
            <span className="text-slate-400">適用ルール</span>
            <span className="font-semibold text-sky-300">
              {snapshot.active_rule}
            </span>
          </div>
          {snapshot.dry_run && (
            <span className="text-xs px-2 py-0.5 rounded bg-amber-500/20 text-amber-300 border border-amber-500/30 font-medium">
              DRY-RUN
            </span>
          )}
          <span className="text-xs text-slate-500">
            最終更新: {updatedLabel}
          </span>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            disabled={busyMode}
            onClick={handleToggleAuto}
            className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium border transition cursor-pointer disabled:opacity-50 ${
              snapshot.auto_switch
                ? "bg-emerald-500/15 text-emerald-300 border-emerald-500/30 hover:bg-emerald-500/25"
                : "bg-amber-500/15 text-amber-300 border-amber-500/30 hover:bg-amber-500/25"
            }`}
          >
            {snapshot.auto_switch ? (
              <>
                <Unlock className="w-3.5 h-3.5" />
                自動切替: ON
              </>
            ) : (
              <>
                <Lock className="w-3.5 h-3.5" />
                手動固定中（クリックで再開）
              </>
            )}
          </button>

          <button
            type="button"
            disabled={busyRefresh || snapshot.dry_run}
            onClick={handleRefreshBattery}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium border border-slate-700 bg-slate-800 hover:bg-slate-700 text-slate-200 transition cursor-pointer disabled:opacity-50"
          >
            <RefreshCw
              className={`w-3.5 h-3.5 ${busyRefresh ? "animate-spin" : ""}`}
            />
            バッテリー更新
          </button>
        </div>
      </section>

      {/* 手動切替オプション */}
      <div className="flex items-center justify-between text-xs text-slate-400 px-1">
        <span>登録デバイス（全 {snapshot.devices.length} 台）</span>
        <label className="inline-flex items-center gap-2 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={lockOnManual}
            onChange={(e) => setLockOnManual(e.target.checked)}
            className="rounded border-slate-700 bg-slate-900 accent-sky-500"
          />
          <span>手動切替時に自動切替を一時停止する</span>
        </label>
      </div>

      {/* デバイスカード一覧 */}
      <section className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
        {snapshot.devices.map((dev) => {
          const badge = modeBadge(dev.mode);
          const modeNorm = normalizeMode(dev.mode);
          const slots = slotRange(dev.capabilities.profile_slots);
          const isBusy = switchingDevice === dev.id;
          const isDisconnected =
            modeNorm === "disconnected" && !snapshot.dry_run;

          return (
            <article
              key={dev.id}
              className="rounded-xl border border-slate-800 bg-slate-900/60 p-4 flex flex-col justify-between gap-4"
            >
              {/* ヘッダー */}
              <div className="flex items-start justify-between gap-2">
                <div className="flex items-center gap-2.5 min-w-0">
                  <div className="p-2 rounded-lg bg-slate-800/80 border border-slate-700/60 shrink-0">
                    {deviceIcon(dev.kind)}
                  </div>
                  <div className="min-w-0">
                    <h2 className="font-semibold text-sm text-slate-100 truncate">
                      {dev.name}
                    </h2>
                    <p className="text-[11px] font-mono text-slate-500">
                      {dev.id}
                    </p>
                  </div>
                </div>
                <span
                  className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium border shrink-0 ${badge.className}`}
                >
                  {badge.icon}
                  {badge.label}
                </span>
              </div>

              {/* バッテリーセクション */}
              <div className="space-y-1.5">
                <div className="flex items-center justify-between text-xs">
                  <span className="inline-flex items-center gap-1.5 text-slate-400">
                    {dev.is_charging ? (
                      <BatteryCharging className="w-3.5 h-3.5 text-sky-400" />
                    ) : (
                      <Battery className="w-3.5 h-3.5 text-slate-400" />
                    )}
                    バッテリー
                  </span>

                  {dev.capabilities.battery ? (
                    <div className="flex items-center gap-1.5">
                      {dev.battery_stale && (
                        <span className="px-1.5 py-0.5 rounded text-[10px] bg-amber-500/15 text-amber-300 border border-amber-500/30">
                          前回値
                        </span>
                      )}
                      {dev.charging_label && (
                        <span className="text-[11px] text-slate-400">
                          {dev.charging_label}
                        </span>
                      )}
                      <span className="font-mono font-semibold text-slate-100">
                        {dev.battery_percent !== null
                          ? `${dev.battery_percent}%`
                          : "--"}
                      </span>
                    </div>
                  ) : (
                    <span className="text-[11px] text-slate-500">
                      残量取得非対応
                    </span>
                  )}
                </div>

                {dev.capabilities.battery && (
                  <div className="h-2 w-full rounded-full bg-slate-800 overflow-hidden">
                    {dev.battery_percent !== null && (
                      <div
                        className={`h-full rounded-full transition-all duration-300 ${batteryBarColor(
                          dev.battery_percent,
                          dev.is_charging,
                        )} ${dev.battery_stale ? "opacity-50" : ""}`}
                        style={{
                          width: `${Math.max(0, Math.min(100, dev.battery_percent))}%`,
                        }}
                      />
                    )}
                  </div>
                )}
              </div>

              {/* プロファイル切替セクション */}
              <div className="space-y-2 pt-2 border-t border-slate-800/80">
                <div className="flex items-center justify-between text-xs">
                  <span className="text-slate-400">プロファイル</span>
                  {dev.capabilities.profile ? (
                    <span className="font-mono text-slate-300">
                      適用中:{" "}
                      {dev.active_slot !== null
                        ? `Slot ${dev.active_slot}`
                        : "-"}
                    </span>
                  ) : (
                    <span className="text-[11px] text-slate-500">
                      切替非対応
                    </span>
                  )}
                </div>

                {dev.capabilities.profile && slots.length > 0 && (
                  <div className="flex flex-wrap gap-1.5">
                    {slots.map((slot) => {
                      const isActive = dev.active_slot === slot;
                      return (
                        <button
                          key={slot}
                          type="button"
                          disabled={isBusy || isDisconnected}
                          onClick={() => handleSwitchProfile(dev, slot)}
                          className={`min-w-8 px-2.5 py-1 rounded-md text-xs font-mono font-medium border transition cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed ${
                            isActive
                              ? "bg-sky-500 text-slate-950 border-sky-400 shadow-sm shadow-sky-500/30"
                              : "bg-slate-800/80 text-slate-200 border-slate-700 hover:border-sky-500/60 hover:text-sky-300"
                          }`}
                        >
                          {slot}
                        </button>
                      );
                    })}
                  </div>
                )}

                {dev.capabilities.wired_only_control &&
                  modeNorm !== "wired" && (
                    <p className="text-[11px] text-slate-500">
                      ※制御コマンドは有線接続時のみ対応
                    </p>
                  )}
              </div>

              {/* 直近エラー表示 */}
              {dev.last_error && (
                <div className="flex items-start gap-1.5 rounded-lg bg-rose-950/40 border border-rose-500/30 px-2.5 py-1.5 text-[11px] text-rose-200">
                  <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5 text-rose-400" />
                  <span className="break-all">{dev.last_error}</span>
                </div>
              )}
            </article>
          );
        })}
      </section>
    </div>
  );
}
