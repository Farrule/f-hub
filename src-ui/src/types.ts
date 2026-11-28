export type ConnectionMode = "wired" | "wireless_24g" | "disconnected";

export interface Capabilities {
  battery: boolean;
  profile: boolean;
  profile_slots: [number, number] | null;
  remap: boolean;
  wired_only_control: boolean;
}

export interface DeviceStateSnapshot {
  id: string;
  name: string;
  kind: string;
  capabilities: Capabilities;
  mode: ConnectionMode;
  battery_percent: number | null;
  charging_label: string | null;
  is_charging: boolean;
  battery_stale: boolean;
  active_slot: number | null;
  last_error: string | null;
}

export interface AppSnapshot {
  dry_run: boolean;
  auto_switch: boolean;
  active_exe: string | null;
  active_rule: string;
  config_path: string;
  updated_at_ms: number;
  devices: DeviceStateSnapshot[];
}

export interface GeneralConfig {
  debounce_ms: number;
  battery_poll_sec: number;
  low_battery_threshold: number;
}

export interface RuleConfig {
  name: string;
  match_exe: string[];
  profiles: Record<string, number>;
}

export interface Config {
  general: GeneralConfig;
  default: Record<string, number>;
  rules: RuleConfig[];
}
