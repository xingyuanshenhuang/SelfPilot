import { addCollection, addAPIProvider } from "@iconify/vue";
import mdiSubset from "@/generated/mdi-subset";

/** 图标加载模式：local = 本地内置（默认，离线可用）；online = 联网按需拉取（备用） */
export type IconMode = "local" | "online";

/**
 * 固定 Iconify 运行时 API 端点：默认提供者只使用 api.iconify.design。
 * Iconify 默认会在 api.iconify.design / api.simplesvg.com / api.unisvg.com
 * 三个镜像间随机挑选（fallBackAPISources），而严格 CSP 只放行了 api.iconify.design，
 * 随机请求打到另外两个镜像会被 CSP 拦截，导致控制台报错、图标加载失败。
 * 此调用将"联网"兜底模式限定在 CSP 白名单内，必须与 enableLocalIcons 一样在启动时执行。
 */
export function initIconRuntime(): void {
  addAPIProvider("", { resources: ["https://api.iconify.design"] });
}

/**
 * 加载本地 mdi 图标集（仅打包了源码中实际用到的图标子集，约 34KB，由
 * scripts/mdi-subset.mjs 自动生成）。本地与联网两种模式渲染的 SVG 数据
 * 完全一致，尺寸由组件 width 属性决定，切换模式不影响图标大小与外观。
 */
export function enableLocalIcons(): void {
  addCollection(mdiSubset);
}
