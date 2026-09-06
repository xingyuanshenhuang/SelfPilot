import { addCollection } from "@iconify/vue";
import mdiSubset from "@/generated/mdi-subset";

/** 图标加载模式：local = 本地内置（默认，离线可用）；online = 联网按需拉取（备用） */
export type IconMode = "local" | "online";

/**
 * 加载本地 mdi 图标集（仅打包了源码中实际用到的图标子集，约 34KB，由
 * scripts/mdi-subset.mjs 自动生成）。本地与联网两种模式渲染的 SVG 数据
 * 完全一致，尺寸由组件 width 属性决定，切换模式不影响图标大小与外观。
 */
export function enableLocalIcons(): void {
  addCollection(mdiSubset);
}
