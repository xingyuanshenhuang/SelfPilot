import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import UnoCSS from "unocss/vite";
import AutoImport from "unplugin-auto-import/vite";
import Components from "unplugin-vue-components/vite";
import { NaiveUiResolver } from "unplugin-vue-components/resolvers";
import path from "node:path";
import { generateMdiSubset } from "./scripts/mdi-subset.mjs";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [
    // 启动/构建时扫描源码里的 mdi:*，自动重建本地图标子集（新增图标无需手动跑脚本）
    {
      name: "mdi-subset",
      buildStart() {
        generateMdiSubset();
      },
    },
    vue(),
    UnoCSS(),
    AutoImport({
      imports: [
        "vue",
        "pinia",
        {
          "naive-ui": [
            "useDialog",
            "useMessage",
            "useNotification",
            "useLoadingBar",
          ],
        },
      ],
    }),
    Components({
      resolvers: [NaiveUiResolver()],
    }),
  ],
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: ["es2021", "chrome100", "safari13"],
    minify: !process.env.TAURI_DEBUG ? "oxc" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
    // StatsView 内联了 echarts（树摇后约 600KB），该页面按需懒加载且桌面端从本地磁盘加载，
    // 属预期体积，提高 Vite 的警告阈值以消除误报
    chunkSizeWarningLimit: 700,
    // 关闭 Rolldown 的 pluginTimings 性能诊断（仅调试性能时需要；避免 PowerShell 把 stderr 当错误染成红色）
    rolldownOptions: {
      checks: {
        pluginTimings: false,
      },
    },
  },
}));
