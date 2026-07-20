<script setup lang="ts">
import { onMounted, ref, computed } from "vue";
import {
  NCard,
  NButton,
  NSpace,
  NRadioGroup,
  NRadioButton,
  NModal,
  NSelect,
  NDescriptions,
  NDescriptionsItem,
  NTag,
  NSwitch,
  useMessage,
  useDialog,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import { save, open } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { useSettingStore } from "@/stores/settingStore";
import { useEncouragementStore } from "@/stores/encouragementStore";
import * as backupApi from "@/api/backup";
import type { ImportConflictMode, ImportResult } from "@/types";

const settingStore = useSettingStore();
const encStore = useEncouragementStore();
const message = useMessage();
const dialog = useDialog();

const exporting = ref(false);
const importing = ref(false);
const showImportModal = ref(false);
const importPreview = ref("");
const conflictMode = ref<ImportConflictMode>("skip");
const fileInputRef = ref<HTMLInputElement | null>(null);

// SQLite 原生备份/恢复状态
const nativeBackingUp = ref(false);
const nativeRestoring = ref(false);

// 导入文件摘要信息
interface ExportSummary {
  version?: string;
  exported_at?: string;
  goals_count?: number;
  tasks_count?: number;
  encouragements_count?: number;
  settings_count?: number;
}
const importSummary = ref<ExportSummary | null>(null);

const conflictOptions = [
  { label: "跳过冲突项（保留本地）", value: "skip" },
  { label: "覆盖冲突项（使用导入数据）", value: "overwrite" },
  { label: "重命名导入项（保留双方）", value: "rename" },
];

// P1-4：鼓励语偏好设置选项
const frequencyOptions = [
  { label: "每次完成", value: "aggressive" },
  { label: "首任务+里程碑", value: "normal" },
  { label: "仅里程碑", value: "sparse" },
];

const styleOptions = [
  { label: "温暖鼓励", value: "warm" },
  { label: "专业理性", value: "professional" },
  { label: "极简克制", value: "minimal" },
];

onMounted(async () => {
  if (!settingStore.loaded) {
    await settingStore.loadTheme();
  }
  // P1-4：加载鼓励语偏好设置
  await encStore.fetchSettings();
});

async function handleThemeChange(value: "light" | "dark") {
  await settingStore.setTheme(value);
  message.success(value === "dark" ? "已切换到深色主题" : "已切换到浅色主题");
}

/** 解析 JSON 备份文件摘要 */
function parseExportSummary(jsonStr: string): ExportSummary | null {
  try {
    const data = JSON.parse(jsonStr);
    return {
      version: data.version,
      exported_at: data.exported_at,
      goals_count: Array.isArray(data.goals) ? data.goals.length : 0,
      tasks_count: Array.isArray(data.tasks) ? data.tasks.length : 0,
      encouragements_count: Array.isArray(data.encouragements)
        ? data.encouragements.length
        : 0,
      settings_count: Array.isArray(data.settings) ? data.settings.length : 0,
    };
  } catch {
    return null;
  }
}

/** 格式化导出时间 */
const formattedExportTime = computed(() => {
  if (!importSummary.value?.exported_at) return "未知";
  try {
    return importSummary.value.exported_at.replace("T", " ");
  } catch {
    return importSummary.value.exported_at;
  }
});

/** 导出数据为 JSON 文件（使用系统保存对话框选择位置） */
async function handleExport() {
  try {
    const ts = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
    const targetPath = await save({
      title: "选择导出保存位置",
      defaultPath: `selfpilot-backup-${ts}.json`,
      filters: [{ name: "JSON 文件", extensions: ["json"] }],
    });
    if (!targetPath) return; // 用户取消

    exporting.value = true;
    await backupApi.exportDataToFile(targetPath);
    message.success("导出成功！");
  } catch (e) {
    message.error(`导出失败: ${String(e)}`);
  } finally {
    exporting.value = false;
  }
}

/** 选择导入文件 */
function triggerFileSelect() {
  fileInputRef.value?.click();
}

function handleFileChange(e: Event) {
  const target = e.target as HTMLInputElement;
  const file = target.files?.[0];
  if (!file) return;

  const reader = new FileReader();
  reader.onload = () => {
    const text = String(reader.result || "");
    importPreview.value = text;
    importSummary.value = parseExportSummary(text);
    showImportModal.value = true;
    // 重置 input，便于重复选择同一文件
    target.value = "";
  };
  reader.onerror = () => {
    message.error("文件读取失败");
  };
  reader.readAsText(file);
}

/** 确认导入 */
async function confirmImport() {
  if (!importPreview.value) {
    message.warning("没有可导入的数据");
    return;
  }
  importing.value = true;
  try {
    const result: ImportResult = await backupApi.importData({
      data: importPreview.value,
      conflict_mode: conflictMode.value,
    });
    showImportModal.value = false;
    importPreview.value = "";
    importSummary.value = null;

    const total =
      result.goals_imported +
      result.stages_imported +
      result.tasks_imported +
      result.encouragements_imported +
      result.settings_imported;
    const skipped =
      result.goals_skipped + result.stages_skipped + result.tasks_skipped;

    message.success(
      `导入完成：共 ${total} 项${skipped > 0 ? `，跳过 ${skipped} 项` : ""}`,
    );

    dialog.info({
      title: "导入结果",
      content: () => `
        目标：导入 ${result.goals_imported}，跳过 ${result.goals_skipped}
        阶段：导入 ${result.stages_imported}，跳过 ${result.stages_skipped}
        任务：导入 ${result.tasks_imported}，跳过 ${result.tasks_skipped}
        鼓励语：导入 ${result.encouragements_imported}
        设置：导入 ${result.settings_imported}
      `,
      positiveText: "知道了",
    });
  } catch (e) {
    message.error(`导入失败: ${String(e)}`);
  } finally {
    importing.value = false;
  }
}

function cancelImport() {
  showImportModal.value = false;
  importPreview.value = "";
  importSummary.value = null;
}

/** 原生备份 — 弹出保存对话框 → 调用 backup_database */
async function handleNativeBackup() {
  try {
    const ts = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
    const targetPath = await save({
      title: "选择备份保存位置",
      defaultPath: `selfpilot-backup-${ts}.db`,
      filters: [{ name: "数据库文件", extensions: ["db", "sqlite"] }],
    });
    if (!targetPath) return; // 用户取消

    nativeBackingUp.value = true;
    await backupApi.backupDatabase(targetPath);
    message.success("备份成功！");
  } catch (e) {
    message.error(`备份失败: ${String(e)}`);
  } finally {
    nativeBackingUp.value = false;
  }
}

/** 原生恢复 — 弹出打开对话框 → 确认对话框 → 调用 restore_database → 提示重启 */
async function handleNativeRestore() {
  try {
    const sourcePath = await open({
      title: "选择备份文件",
      multiple: false,
      filters: [{ name: "数据库文件", extensions: ["db", "sqlite"] }],
    });
    if (!sourcePath || Array.isArray(sourcePath)) return; // 用户取消

    dialog.warning({
      title: "确认恢复",
      content:
        "恢复将覆盖当前所有数据，且应用需要重启。恢复前会自动备份当前数据，不用担心丢失。确定继续？",
      positiveText: "确认恢复",
      negativeText: "取消",
      onPositiveClick: async () => {
        nativeRestoring.value = true;
        try {
          await backupApi.restoreDatabase(sourcePath as string);
          dialog.success({
            title: "恢复成功",
            content: "数据已恢复，需要重启应用才能生效。",
            positiveText: "重启",
            negativeText: "稍后重启",
            onPositiveClick: async () => {
              await relaunch();
            },
          });
        } catch (e) {
          message.error(`恢复失败: ${String(e)}`);
        } finally {
          nativeRestoring.value = false;
        }
      },
    });
  } catch (e) {
    message.error(String(e));
  }
}
</script>

<template>
  <div class="space-y-4">
    <!-- 主题设置 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon icon="mdi:palette-outline" width="20" class="text-brand-500" />
          <span>主题设置</span>
        </div>
      </template>
      <NSpace vertical :size="12">
        <div class="text-sm text-gray-600">选择应用的主题外观：</div>
        <NRadioGroup
          :value="settingStore.theme"
          @update:value="handleThemeChange"
        >
          <NRadioButton value="light">
            <template #icon><Icon icon="mdi:weather-sunny" /></template>
            浅色
          </NRadioButton>
          <NRadioButton value="dark">
            <template #icon><Icon icon="mdi:weather-night" /></template>
            深色
          </NRadioButton>
        </NRadioGroup>
      </NSpace>
    </NCard>

    <!-- P1-4：鼓励语偏好 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon
            icon="mdi:message-text-outline"
            width="20"
            class="text-orange-500"
          />
          <span>鼓励语偏好</span>
        </div>
      </template>
      <NSpace vertical :size="16">
        <!-- 总开关 -->
        <div class="flex items-center justify-between">
          <div>
            <div class="text-sm font-medium">显示鼓励语</div>
            <div class="text-xs text-gray-500">完成任务时显示鼓励文案</div>
          </div>
          <NSwitch
            v-model:value="encStore.settings.enabled"
            @update:value="
              encStore
                .updateSettings({ enabled: $event })
                .then(() => message.success('设置已保存'))
            "
          />
        </div>

        <!-- 展示频率 -->
        <div>
          <div class="text-sm font-medium mb-2">展示频率</div>
          <NRadioGroup
            v-model:value="encStore.settings.frequency"
            @update:value="
              encStore
                .updateSettings({ frequency: $event })
                .then(() => message.success('设置已保存'))
            "
          >
            <NRadioButton
              v-for="opt in frequencyOptions"
              :key="opt.value"
              :value="opt.value"
            >
              {{ opt.label }}
            </NRadioButton>
          </NRadioGroup>
        </div>

        <!-- 文案风格 -->
        <div>
          <div class="text-sm font-medium mb-2">文案风格</div>
          <NSelect
            v-model:value="encStore.settings.style"
            :options="styleOptions"
            style="width: 200px"
            @update:value="
              encStore
                .updateSettings({ style: $event })
                .then(() => message.success('设置已保存'))
            "
          />
        </div>

        <!-- 庆祝动画 -->
        <div class="flex items-center justify-between">
          <div>
            <div class="text-sm font-medium">庆祝动画</div>
            <div class="text-xs text-gray-500">全部目标完成时显示庆祝效果</div>
          </div>
          <NSwitch
            v-model:value="encStore.settings.celebration_animation"
            @update:value="
              encStore
                .updateSettings({ celebration_animation: $event })
                .then(() => message.success('设置已保存'))
            "
          />
        </div>

        <!-- emoji 显示 -->
        <div class="flex items-center justify-between">
          <div>
            <div class="text-sm font-medium">显示 emoji</div>
            <div class="text-xs text-gray-500">鼓励语文案中显示表情符号</div>
          </div>
          <NSwitch
            v-model:value="encStore.settings.emoji_enabled"
            @update:value="
              encStore
                .updateSettings({ emoji_enabled: $event })
                .then(() => message.success('设置已保存'))
            "
          />
        </div>
      </NSpace>
    </NCard>

    <!-- 一键备份（推荐） -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon
            icon="mdi:database-sync-outline"
            width="20"
            class="text-purple-500"
          />
          <span>一键备份</span>
          <NTag type="success" size="small" :bordered="false">推荐</NTag>
        </div>
      </template>
      <NSpace vertical :size="12">
        <div class="text-sm text-gray-600">
          一键备份所有数据，恢复时完整还原，速度快。适合日常备份使用。
        </div>
        <NSpace>
          <NButton
            type="primary"
            ghost
            :loading="nativeBackingUp"
            @click="handleNativeBackup"
          >
            <template #icon>
              <Icon icon="mdi:content-save-outline" />
            </template>
            立即备份
          </NButton>
          <NButton
            type="warning"
            ghost
            :loading="nativeRestoring"
            @click="handleNativeRestore"
          >
            <template #icon>
              <Icon icon="mdi:folder-open-outline" />
            </template>
            恢复备份
          </NButton>
        </NSpace>
        <div class="text-xs text-gray-400 flex items-center gap-1">
          <Icon icon="mdi:information-outline" width="14" />
          恢复时会自动备份当前数据，不用担心丢失
        </div>
      </NSpace>
    </NCard>

    <!-- 数据导入导出 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon
            icon="mdi:file-document-outline"
            width="20"
            class="text-brand-500"
          />
          <span>数据导入导出</span>
        </div>
      </template>
      <NSpace vertical :size="12">
        <div class="text-sm text-gray-600">
          以文本格式导出或导入数据。适合跨版本迁移或选择性合并数据。
        </div>
        <NSpace>
          <NButton
            type="primary"
            ghost
            :loading="exporting"
            @click="handleExport"
          >
            <template #icon><Icon icon="mdi:upload-outline" /></template>
            导出数据
          </NButton>
          <NButton
            type="primary"
            ghost
            :loading="importing"
            @click="triggerFileSelect"
          >
            <template #icon><Icon icon="mdi:download-outline" /></template>
            导入数据
          </NButton>
        </NSpace>
        <input
          ref="fileInputRef"
          type="file"
          accept=".json,application/json"
          style="display: none"
          @change="handleFileChange"
        />
        <div class="text-xs text-gray-400 space-y-0.5">
          <div class="flex items-center gap-1 font-medium text-gray-500">
            导入时的冲突处理方式：
          </div>
          <div class="flex items-center gap-1">
            <Icon icon="mdi:skip-next" width="12" />
            跳过 — 保留本地数据，不导入冲突项
          </div>
          <div class="flex items-center gap-1">
            <Icon icon="mdi:overwrite" width="12" />
            覆盖 — 用导入数据替换本地冲突项
          </div>
          <div class="flex items-center gap-1">
            <Icon icon="mdi:content-duplicate" width="12" />
            保留双方 — 为导入项生成新标识，双方数据都保留
          </div>
        </div>
      </NSpace>
    </NCard>

    <!-- 备份方式对比 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon
            icon="mdi:help-circle-outline"
            width="20"
            class="text-brand-500"
          />
          <span>两种备份方式有什么区别？</span>
        </div>
      </template>
      <table class="w-full text-sm border-collapse">
        <thead>
          <tr class="border-b border-gray-200">
            <th class="py-2 px-3 text-left text-gray-500 font-medium w-24" />
            <th class="py-2 px-3 text-center font-medium text-purple-600">
              <Icon
                icon="mdi:database-sync-outline"
                width="16"
                class="inline mr-1"
              />
              一键备份
            </th>
            <th class="py-2 px-3 text-center font-medium text-brand-600">
              <Icon
                icon="mdi:file-document-outline"
                width="16"
                class="inline mr-1"
              />
              导入导出
            </th>
          </tr>
        </thead>
        <tbody>
          <tr class="border-b border-gray-100">
            <td class="py-2 px-3 text-gray-500">备份范围</td>
            <td class="py-2 px-3 text-center">全部数据</td>
            <td class="py-2 px-3 text-center">全部数据</td>
          </tr>
          <tr class="border-b border-gray-100">
            <td class="py-2 px-3 text-gray-500">数据格式</td>
            <td class="py-2 px-3 text-center">数据库文件</td>
            <td class="py-2 px-3 text-center">文本文件</td>
          </tr>
          <tr class="border-b border-gray-100">
            <td class="py-2 px-3 text-gray-500">恢复方式</td>
            <td class="py-2 px-3 text-center">替换全部数据</td>
            <td class="py-2 px-3 text-center">可选择合并</td>
          </tr>
          <tr class="border-b border-gray-100">
            <td class="py-2 px-3 text-gray-500">恢复速度</td>
            <td class="py-2 px-3 text-center">
              <NTag size="small" type="success" :bordered="false">快</NTag>
            </td>
            <td class="py-2 px-3 text-center">
              <NTag size="small" type="warning" :bordered="false">较慢</NTag>
            </td>
          </tr>
          <tr class="border-b border-gray-100">
            <td class="py-2 px-3 text-gray-500">适用场景</td>
            <td class="py-2 px-3 text-center">日常备份恢复</td>
            <td class="py-2 px-3 text-center">跨版本迁移</td>
          </tr>
          <tr>
            <td class="py-2 px-3 text-gray-500">推荐度</td>
            <td class="py-2 px-3 text-center text-purple-500">⭐⭐⭐</td>
            <td class="py-2 px-3 text-center text-gray-400">⭐⭐</td>
          </tr>
        </tbody>
      </table>
    </NCard>

    <!-- 关于 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon
            icon="mdi:information-outline"
            width="20"
            class="text-brand-500"
          />
          <span>关于</span>
        </div>
      </template>
      <NDescriptions :column="1" label-placement="left" bordered size="small">
        <NDescriptionsItem label="应用名称"
          >SelfPilot 自学计划任务规划</NDescriptionsItem
        >
        <NDescriptionsItem label="版本">0.1.0</NDescriptionsItem>
        <NDescriptionsItem label="技术栈"
          >Tauri 2 + Vue 3 + SQLite</NDescriptionsItem
        >
        <NDescriptionsItem label="数据存储"
          >本地 SQLite（selfpilot.db）</NDescriptionsItem
        >
      </NDescriptions>
    </NCard>

    <!-- 导入确认弹窗 -->
    <NModal
      v-model:show="showImportModal"
      preset="card"
      title="确认导入数据"
      style="width: 520px"
    >
      <NSpace vertical :size="12">
        <!-- 数据摘要 -->
        <div v-if="importSummary" class="bg-gray-50 rounded p-3 space-y-1">
          <div class="text-sm font-medium text-gray-700 mb-2">备份文件信息</div>
          <div class="text-xs text-gray-600 flex items-center gap-2">
            <Icon icon="mdi:tag-outline" width="14" />
            版本：{{ importSummary.version || "未知" }}
          </div>
          <div class="text-xs text-gray-600 flex items-center gap-2">
            <Icon icon="mdi:clock-outline" width="14" />
            导出时间：{{ formattedExportTime }}
          </div>
          <div class="text-xs text-gray-600 flex items-center gap-2">
            <Icon icon="mdi:bullseye" width="14" />
            {{ importSummary.goals_count ?? 0 }} 个目标，
            {{ importSummary.tasks_count ?? 0 }} 个任务，
            {{ importSummary.encouragements_count ?? 0 }} 条鼓励语，
            {{ importSummary.settings_count ?? 0 }} 项设置
          </div>
        </div>
        <div v-else class="text-xs text-orange-500 flex items-center gap-1">
          <Icon icon="mdi:alert-outline" width="14" />
          无法解析备份文件摘要，请确认文件格式正确
        </div>

        <div class="text-sm">选择 ID 冲突时的处理方式：</div>
        <NSelect v-model:value="conflictMode" :options="conflictOptions" />
      </NSpace>
      <template #footer>
        <NSpace justify="end">
          <NButton @click="cancelImport">取消</NButton>
          <NButton type="primary" :loading="importing" @click="confirmImport">
            确认导入
          </NButton>
        </NSpace>
      </template>
    </NModal>
  </div>
</template>
