/**
 * 统一异步操作封装（R-06）
 *
 * 收敛组件内重复的 try/catch + message.error 模式：
 * - 自动包裹 loading 状态（可选）
 * - 失败时统一弹出错误提示
 * - 成功时返回结果，失败时返回 undefined（调用方据此短路后续逻辑）
 *
 * 注意：内部使用 useMessage()，需在 NMessageProvider 包裹的组件 setup 中调用。
 */

import { ref } from "vue";
import { useMessage } from "naive-ui";
import { toErrorMessage } from "@/api/client";

export interface UseAsyncActionOptions {
  /** 失败时的统一提示文案（缺省使用错误原始信息） */
  errorMessage?: string;
  /** 是否用 message 弹出错误提示（默认 true） */
  showError?: boolean;
}

export function useAsyncAction() {
  const loading = ref(false);
  const message = useMessage();

  async function run<T>(
    fn: () => Promise<T>,
    options?: UseAsyncActionOptions,
  ): Promise<T | undefined> {
    loading.value = true;
    try {
      return await fn();
    } catch (e) {
      console.error(e);
      if (options?.showError !== false) {
        message.error(options?.errorMessage ?? toErrorMessage(e));
      }
      return undefined;
    } finally {
      loading.value = false;
    }
  }

  return { run, loading };
}
