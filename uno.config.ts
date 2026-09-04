import {
  defineConfig,
  presetUno,
  presetAttributify,
  transformerDirectives,
} from "unocss";

export default defineConfig({
  presets: [presetUno({ dark: "class" }), presetAttributify()],
  transformers: [transformerDirectives()],
  theme: {
    colors: {
      brand: {
        50: "#eef6ff",
        100: "#d9eaff",
        200: "#bcd9ff",
        300: "#8ec1ff",
        400: "#599dff",
        500: "#3478f6",
        600: "#1f5ae0",
        700: "#1947b8",
        800: "#1a3e96",
        900: "#1b3877",
      },
      // 深色模式语义色，与 Naive UI darkTheme 基础色保持一致
      surface: {
        body: "#101014",
        card: "#18181c",
        muted: "#26262c",
        strong: "#313137",
        hover: "rgba(255,255,255,0.06)",
        hoverStrong: "rgba(255,255,255,0.10)",
        border: "rgba(255,255,255,0.12)",
        borderMuted: "rgba(255,255,255,0.07)",
      },
    },
  },
});
