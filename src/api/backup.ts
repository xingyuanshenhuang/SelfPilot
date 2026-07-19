import { invoke } from "@tauri-apps/api/core";
import type { ImportInput, ImportResult } from "@/types";

/** 导出全部数据为 JSON 字符串 */
export async function exportData(): Promise<string> {
  return invoke("export_data");
}

/** 导出全部数据到指定路径的 JSON 文件（直接写入，不走 IPC 字符串传输） */
export async function exportDataToFile(targetPath: string): Promise<void> {
  return invoke("export_data_to_file", { targetPath });
}

/** 导入数据 */
export async function importData(input: ImportInput): Promise<ImportResult> {
  return invoke("import_data", { input });
}

/** SQLite 原生备份（生成 .db 完整副本，P2-4） */
export async function backupDatabase(targetPath: string): Promise<void> {
  return invoke("backup_database", { targetPath });
}

/** SQLite 原生恢复（覆盖当前数据库，需重启应用，P2-4） */
export async function restoreDatabase(sourcePath: string): Promise<void> {
  return invoke("restore_database", { sourcePath });
}
