import { invokeCommand } from "./client";
import type { SetSettingInput, Setting } from "@/types";

/** 获取所有设置项 */
export async function getAllSettings(): Promise<Setting[]> {
  return invokeCommand("get_all_settings");
}

/** 获取单个设置项 */
export async function getSetting(key: string): Promise<string | null> {
  return invokeCommand("get_setting", { key });
}

/** 设置某个值（upsert） */
export async function setSetting(input: SetSettingInput): Promise<void> {
  return invokeCommand("set_setting", { input });
}
