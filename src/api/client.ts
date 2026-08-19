/**
 * API 客户端统一入口（R-06）
 *
 * 所有 api/* 模块通过本模块调用 Tauri 命令，避免各自重复 import invoke，
 * 并将错误信息格式化逻辑集中到一处，供 useAsyncAction 等统一消费。
 */

import { invoke } from "@tauri-apps/api/core";

/** 将任意错误转换为可读文案（Tauri 错误通常是字符串/Error） */
export function toErrorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string" && e.length > 0) return e;
  return String(e);
}

/** 类型安全的 Tauri 命令调用（返回值类型由调用方返回类型注解推断） */
export async function invokeCommand<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(cmd, args);
}
