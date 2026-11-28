import { CheckCircle2, Keyboard, Wrench } from "lucide-react";
import type { DeviceStateSnapshot } from "../types";

interface RemapPlaceholderTabProps {
  devices: DeviceStateSnapshot[];
}

export function RemapPlaceholderTab({ devices }: RemapPlaceholderTabProps) {
  return (
    <div className="space-y-5 max-w-3xl">
      <div className="rounded-xl border border-sky-500/30 bg-sky-950/20 p-5 space-y-2">
        <div className="flex items-center gap-2 text-sky-300 font-semibold text-sm">
          <Wrench className="w-4 h-4" />
          リマップ設定（Phase 3-B 実装予定）
        </div>
        <p className="text-xs text-slate-300 leading-relaxed">
          Phase 3-B
          で、以下のプロトコル解析済みデバイスに対するキー／ボタン割当エディタをこのタブに組み込みます。
        </p>
      </div>

      <section className="rounded-xl border border-slate-800 bg-slate-900/60 divide-y divide-slate-800/80">
        {devices.map((dev) => (
          <div
            key={dev.id}
            className="p-4 flex items-center justify-between gap-3 text-xs"
          >
            <div className="flex items-center gap-2.5">
              <Keyboard className="w-4 h-4 text-slate-400" />
              <div>
                <p className="font-medium text-slate-100">{dev.name}</p>
                <p className="font-mono text-[11px] text-slate-500">{dev.id}</p>
              </div>
            </div>
            <span
              className={`inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full border ${
                dev.capabilities.remap
                  ? "bg-emerald-500/15 text-emerald-300 border-emerald-500/30"
                  : "bg-slate-800 text-slate-400 border-slate-700"
              }`}
            >
              <CheckCircle2 className="w-3 h-3" />
              {dev.capabilities.remap ? "対応予定 (解析済)" : "非対応"}
            </span>
          </div>
        ))}
      </section>
    </div>
  );
}
