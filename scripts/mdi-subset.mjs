// 扫描 src 中实际使用的 mdi 图标，从整套 mdi.json 提取出小型子集，
// 写入 src/generated/mdi-subset.ts，供本地图标模式使用。
// 可被 vite 插件（buildStart）调用自动重建，也可用 node scripts/mdi-subset.mjs 手动运行。
import { readdirSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const srcDir = join(root, "src");
const genDir = join(srcDir, "generated");
const outFile = join(genDir, "mdi-subset.ts");
const fullJsonFile = join(root, "node_modules/@iconify/json/json/mdi.json");

/** 扫描源码中所有 mdi:* 引用，生成仅含实际使用图标的子集，返回图标数量 */
export function generateMdiSubset() {
  // 1. 扫描 src 下所有源码文件里的 mdi:icon-name 字面量
  const names = new Set();
  function walk(dir) {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const p = join(dir, entry.name);
      if (entry.isDirectory()) {
        if (entry.name === "generated") continue; // 避免把生成的子集本身当资源扫描
        walk(p);
      } else if (/\.(vue|ts|tsx|js|jsx)$/.test(entry.name)) {
        const text = readFileSync(p, "utf8");
        for (const m of text.matchAll(/mdi:([A-Za-z0-9-]+)/g)) names.add(m[1]);
      }
    }
  }
  walk(srcDir);

  // 2. 读取整套 mdi 图标集，仅保留用到的图标及其依赖的 aliases
  const full = JSON.parse(readFileSync(fullJsonFile, "utf8"));
  const icons = {};
  for (const [k, v] of Object.entries(full.icons)) if (names.has(k)) icons[k] = v;
  const aliases = {};
  if (full.aliases) {
    for (const [k, v] of Object.entries(full.aliases)) {
      if (names.has(k) || (v.parent && (icons[v.parent] || aliases[v.parent]))) {
        aliases[k] = v;
      }
    }
  }
  const subset = {
    prefix: "mdi",
    width: full.width,
    height: full.height,
    icons,
    ...(Object.keys(aliases).length ? { aliases } : {}),
  };

  // 3. 写出产物（无声明文件的导出可供 vue-tsc / vite 直接使用）
  mkdirSync(genDir, { recursive: true });
  writeFileSync(
    outFile,
    [
      "// 此文件由 scripts/mdi-subset.mjs 自动生成，请勿手动修改",
      "import type { IconifyJSON } from \"@iconify/vue\";",
      "",
      `const mdiSubset: IconifyJSON = ${JSON.stringify(subset)};`,
      "",
      "export default mdiSubset;",
      "",
    ].join("\n"),
  );
  return names.size;
}

// 命令行直接执行时生成（node scripts/mdi-subset.mjs）
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href.replace(/\\/g, "/").toLowerCase() ===
    import.meta.url.replace(/\\/g, "/").toLowerCase()
) {
  const count = generateMdiSubset();
  console.log(`[mdi-subset] 已重新生成 src/generated/mdi-subset.ts，含 ${count} 个 mdi 图标`);
}