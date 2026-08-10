import { invoke } from "@tauri-apps/api/core";
import type { ImportInput, ImportResult } from "@/types";

/**
 * 前端路径基本校验（S-02: SEC-H-01 路径遍历防护）
 *
 * 拒绝空字符串、含空字节、含 `..` 的路径。
 * 后端会做更严格的校验（canonicalize + SQLite 魔术字），
 * 前端校验作为第一道防线，提前拦截明显恶意输入。
 */
function validatePath(path: string, label: string): void {
  if (!path || path.trim().length === 0) {
    throw new Error(`${label}不能为空`);
  }
  if (path.includes("\0")) {
    throw new Error(`${label}包含非法字符（空字节）`);
  }
  if (path.includes("..")) {
    throw new Error(`${label}不能包含父目录引用 (..)`);
  }
}

/** 导出全部数据为 JSON 字符串 */
export async function exportData(): Promise<string> {
  return invoke("export_data");
}

/** 导出全部数据到指定路径的 JSON 文件（直接写入，不走 IPC 字符串传输） */
export async function exportDataToFile(targetPath: string): Promise<void> {
  validatePath(targetPath, "导出路径");
  return invoke("export_data_to_file", { targetPath });
}

/** 导入数据 */
export async function importData(input: ImportInput): Promise<ImportResult> {
  return invoke("import_data", { input });
}

/** SQLite 原生备份（生成 .db 完整副本，P2-4） */
export async function backupDatabase(targetPath: string): Promise<void> {
  validatePath(targetPath, "备份路径");
  return invoke("backup_database", { targetPath });
}

/** SQLite 原生恢复（覆盖当前数据库，需重启应用，P2-4） */
export async function restoreDatabase(sourcePath: string): Promise<void> {
  validatePath(sourcePath, "恢复源路径");
  return invoke("restore_database", { sourcePath });
}
