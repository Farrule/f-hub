import type { AppSnapshot, Config } from "./types";

export class ApiError extends Error {
  readonly errors: string[];

  constructor(errors: string[]) {
    super(errors.join("\n"));
    this.name = "ApiError";
    this.errors = errors;
  }
}

async function parseJsonOrThrow<T>(res: Response): Promise<T> {
  if (res.ok) {
    return (await res.json()) as T;
  }
  try {
    const body = (await res.json()) as { errors?: string[]; message?: string };
    if (Array.isArray(body.errors) && body.errors.length > 0) {
      throw new ApiError(body.errors);
    }
    if (body.message) {
      throw new ApiError([body.message]);
    }
  } catch (err) {
    if (err instanceof ApiError) throw err;
  }
  throw new ApiError([`HTTP ${res.status} ${res.statusText}`]);
}

export async function fetchState(): Promise<AppSnapshot> {
  const res = await fetch("/api/state");
  return parseJsonOrThrow<AppSnapshot>(res);
}

export async function switchDeviceProfile(
  deviceId: string,
  slot: number,
  lockAuto: boolean,
): Promise<{ message: string }> {
  const res = await fetch(
    `/api/devices/${encodeURIComponent(deviceId)}/profile`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ slot, lock_auto: lockAuto }),
    },
  );
  return parseJsonOrThrow<{ message: string }>(res);
}

export async function setAutoSwitchMode(
  autoSwitch: boolean,
): Promise<{ auto_switch: boolean }> {
  const res = await fetch("/api/mode", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ auto_switch: autoSwitch }),
  });
  return parseJsonOrThrow<{ auto_switch: boolean }>(res);
}

export async function refreshBatteryNow(): Promise<{ message: string }> {
  const res = await fetch("/api/battery/refresh", { method: "POST" });
  return parseJsonOrThrow<{ message: string }>(res);
}

export async function fetchConfig(): Promise<Config> {
  const res = await fetch("/api/config");
  return parseJsonOrThrow<Config>(res);
}

export async function saveConfig(config: Config): Promise<{ message: string }> {
  const res = await fetch("/api/config", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(config),
  });
  return parseJsonOrThrow<{ message: string }>(res);
}

export async function fetchProcesses(): Promise<string[]> {
  const res = await fetch("/api/processes");
  const data = await parseJsonOrThrow<{ processes: string[] }>(res);
  return data.processes;
}
