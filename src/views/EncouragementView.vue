<script setup lang="ts">
import { onMounted, ref, computed } from "vue";
import {
  NCard,
  NButton,
  NSpace,
  NInput,
  NTag,
  NPopconfirm,
  NEmpty,
  NStatistic,
  NSelect,
  NModal,
  NForm,
  NFormItem,
  NCheckbox,
  useMessage,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import { useEncouragementStore } from "@/stores/encouragementStore";
import type { Encouragement, EncouragementLevel } from "@/types";
import * as encApi from "@/api/encouragement";

type TagType = "default" | "success" | "error" | "warning" | "info" | "primary";

const store = useEncouragementStore();
const message = useMessage();

const newText = ref("");
const newLevel = ref<EncouragementLevel>("normal");

// P0-5：编辑鼓励语状态
const showEditModal = ref(false);
const editingItem = ref<Encouragement | null>(null);
const editText = ref("");
const editLevel = ref<EncouragementLevel>("normal");
const saving = ref(false);

// P0-6：删除按钮防连点状态
const deletingId = ref<string | null>(null);

// P1-6：搜索筛选状态
const searchKeyword = ref("");
const filterLevel = ref<EncouragementLevel | "">("");
const filterSource = ref<"preset" | "custom" | "">("");

// P1-5：批量操作状态
const batchMode = ref(false);
const selectedIds = ref<Set<string>>(new Set());
const batchLevel = ref<EncouragementLevel>("normal");
const batchDeleting = ref(false);

// P3-6：导入导出状态
const showImportModal = ref(false);
const importJson = ref("");
const importing = ref(false);

const levelOptions = [
  { label: "普通（1天连续）", value: "normal" },
  { label: "进阶（3天连续）", value: "advanced" },
  { label: "高亮（7天连续）", value: "highlight" },
  { label: "庆祝（全部完成）", value: "celebration" },
  { label: "挫折安抚", value: "setback" },
];

/** 等级元信息 */
const LEVEL_META: Record<
  EncouragementLevel,
  {
    label: string;
    color: TagType;
    icon: string;
    desc: string;
    iconColor: string;
  }
> = {
  normal: {
    label: "普通",
    color: "info",
    icon: "mdi:emoticon-happy-outline",
    desc: "连续 1 天完成时抽取",
    iconColor: "text-blue-500",
  },
  advanced: {
    label: "进阶",
    color: "success",
    icon: "mdi:emoticon-cool-outline",
    desc: "连续 3 天完成时抽取",
    iconColor: "text-green-500",
  },
  highlight: {
    label: "高亮",
    color: "warning",
    icon: "mdi:emoticon-star-outline",
    desc: "连续 7 天完成时抽取",
    iconColor: "text-orange-500",
  },
  celebration: {
    label: "庆祝",
    color: "error",
    icon: "mdi:trophy-outline",
    desc: "全部目标完成时抽取",
    iconColor: "text-red-500",
  },
  setback: {
    label: "挫折安抚",
    color: "default",
    icon: "mdi:heart-outline",
    desc: "连续中断或进度滞后时显示",
    iconColor: "text-gray-500",
  },
};

onMounted(async () => {
  await Promise.all([
    store.fetchAll(),
    store.fetchStreak(),
    store.fetchFavorites(),
  ]);
});

async function handleAdd() {
  const text = newText.value.trim();
  if (!text) {
    message.warning("请输入鼓励语内容");
    return;
  }
  try {
    await store.add(text, newLevel.value);
    message.success("已添加");
    newText.value = "";
  } catch (e) {
    message.error(String(e));
  }
}

/** P0-5：打开编辑弹窗，填充原数据 */
function openEditModal(item: Encouragement) {
  editingItem.value = item;
  editText.value = item.text;
  editLevel.value = item.level;
  showEditModal.value = true;
}

/** P0-5：保存编辑 */
async function handleEditSave() {
  if (!editingItem.value) return;
  const text = editText.value.trim();
  if (text.length < 2) {
    message.warning("鼓励语至少 2 个字");
    return;
  }
  if (text.length > 100) {
    message.warning("鼓励语不超过 100 字");
    return;
  }
  saving.value = true;
  try {
    await store.update(editingItem.value.id, text, editLevel.value);
    message.success("已更新");
    showEditModal.value = false;
  } catch (e) {
    message.error(String(e));
  } finally {
    saving.value = false;
  }
}

/** P0-6：删除反馈增强 — 回显被删文案前 12 字 + 防连点 */
async function handleDelete(item: Encouragement) {
  deletingId.value = item.id;
  try {
    await store.remove(item.id);
    const preview = item.text.slice(0, 12);
    const ellipsis = item.text.length > 12 ? "..." : "";
    message.success(`已删除："${preview}${ellipsis}"`);
  } catch (e) {
    message.error(String(e));
  } finally {
    deletingId.value = null;
  }
}

async function handleRefreshStreak() {
  await store.fetchStreak();
  message.success("已刷新连续天数");
}

/** 按等级分组的自定义鼓励语 */
const customByLevel = computed(() => {
  const groups: Record<EncouragementLevel, typeof store.customList> = {
    normal: [],
    advanced: [],
    highlight: [],
    celebration: [],
    setback: [],
  };
  for (const e of store.customList) {
    if (groups[e.level]) {
      groups[e.level].push(e);
    }
  }
  return groups;
});

/** 按等级分组的预设鼓励语 */
const presetByLevel = computed(() => store.byLevel);

// ============================================================
// P1-6：搜索筛选
// ============================================================

/** 根据关键词筛选文案列表 */
function filterByText(list: Encouragement[]): Encouragement[] {
  const keyword = searchKeyword.value.trim().toLowerCase();
  if (!keyword) return list;
  return list.filter((e) => e.text.toLowerCase().includes(keyword));
}

/** 根据等级筛选 */
function filterByLevel(list: Encouragement[]): Encouragement[] {
  if (!filterLevel.value) return list;
  return list.filter((e) => e.level === filterLevel.value);
}

/** 根据来源筛选 */
function filterBySource(list: Encouragement[]): Encouragement[] {
  if (!filterSource.value) return list;
  return list.filter((e) => e.category === filterSource.value);
}

/** 综合筛选 */
function applyFilters(list: Encouragement[]): Encouragement[] {
  return filterBySource(filterByLevel(filterByText(list)));
}

/** 筛选后的自定义文案 */
const filteredCustomList = computed(() => applyFilters(store.customList));

/** 筛选后的预设文案 */
const filteredPresetList = computed(() => applyFilters(store.presetList));

/** 是否有筛选条件 */
const hasFilter = computed(
  () =>
    searchKeyword.value.trim() !== "" ||
    filterLevel.value !== "" ||
    filterSource.value !== "",
);

/** 重置筛选 */
function resetFilters() {
  searchKeyword.value = "";
  filterLevel.value = "";
  filterSource.value = "";
}

// ============================================================
// P1-5：批量操作
// ============================================================

/** 切换批量模式 */
function toggleBatchMode() {
  batchMode.value = !batchMode.value;
  if (!batchMode.value) {
    selectedIds.value.clear();
  }
}

/** 切换单项选择 */
function toggleSelect(id: string) {
  if (selectedIds.value.has(id)) {
    selectedIds.value.delete(id);
  } else {
    selectedIds.value.add(id);
  }
}

/** 全选/取消全选 */
function toggleSelectAll(list: Encouragement[]) {
  const allIds = list.map((e) => e.id);
  const allSelected = allIds.every((id) => selectedIds.value.has(id));

  if (allSelected) {
    allIds.forEach((id) => selectedIds.value.delete(id));
  } else {
    allIds.forEach((id) => selectedIds.value.add(id));
  }
}

/** 批量删除 */
async function handleBatchDelete() {
  if (selectedIds.value.size === 0) {
    message.warning("请先选择要删除的文案");
    return;
  }
  batchDeleting.value = true;
  try {
    const ids = Array.from(selectedIds.value);
    const deleted = await encApi.batchDeleteEncouragements(ids);
    message.success(`已删除 ${deleted} 条文案`);
    selectedIds.value.clear();
    batchMode.value = false;
    await store.fetchAll();
  } catch (e) {
    message.error(String(e));
  } finally {
    batchDeleting.value = false;
  }
}

/** 批量修改等级 */
async function handleBatchUpdateLevel() {
  if (selectedIds.value.size === 0) {
    message.warning("请先选择要修改的文案");
    return;
  }
  batchDeleting.value = true;
  try {
    const ids = Array.from(selectedIds.value);
    const updated = await encApi.batchUpdateEncouragementLevel(
      ids,
      batchLevel.value,
    );
    message.success(`已修改 ${updated} 条文案等级`);
    selectedIds.value.clear();
    batchMode.value = false;
    await store.fetchAll();
  } catch (e) {
    message.error(String(e));
  } finally {
    batchDeleting.value = false;
  }
}

// ============================================================
// P3-2：收藏与反馈
// ============================================================

/** 切换收藏状态 */
async function toggleFavorite(item: Encouragement) {
  try {
    if (store.isFavorite(item.id)) {
      await store.removeFavorite(item.id);
      message.success("已取消收藏");
    } else {
      await store.addFavorite(item.id);
      message.success("已收藏");
    }
  } catch (e) {
    message.error(String(e));
  }
}

/** 记录反馈 */
async function handleFeedback(item: Encouragement, type: "like" | "dislike") {
  try {
    await store.recordFeedback(item.id, type);
    message.success(
      type === "like"
        ? "感谢反馈，会多推荐这类文案"
        : "感谢反馈，会减少这类文案",
    );
  } catch (e) {
    message.error(String(e));
  }
}

// ============================================================
// P3-5：拖拽排序
// ============================================================

/** 上移鼓励语 */
async function moveUp(item: Encouragement, level: EncouragementLevel) {
  const levelList = store.byLevel[level];
  const idx = levelList.findIndex((e) => e.id === item.id);
  if (idx <= 0) return;

  const prevItem = levelList[idx - 1];
  const currentOrder = item.sort_order || 0;
  const prevOrder = prevItem.sort_order || 0;

  try {
    // 交换排序值
    await encApi.batchUpdateEncouragementOrder([
      [item.id, prevOrder],
      [prevItem.id, currentOrder],
    ]);
    await store.fetchAll();
    message.success("已上移");
  } catch (e) {
    message.error(String(e));
  }
}

/** 下移鼓励语 */
async function moveDown(item: Encouragement, level: EncouragementLevel) {
  const levelList = store.byLevel[level];
  const idx = levelList.findIndex((e) => e.id === item.id);
  if (idx >= levelList.length - 1) return;

  const nextItem = levelList[idx + 1];
  const currentOrder = item.sort_order || 0;
  const nextOrder = nextItem.sort_order || 0;

  try {
    // 交换排序值
    await encApi.batchUpdateEncouragementOrder([
      [item.id, nextOrder],
      [nextItem.id, currentOrder],
    ]);
    await store.fetchAll();
    message.success("已下移");
  } catch (e) {
    message.error(String(e));
  }
}

// ============================================================
// P3-6：导入导出
// ============================================================

/** 导出鼓励语 */
async function handleExport() {
  try {
    const json = await encApi.exportEncouragements();
    // 创建下载
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `encouragements_${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
    message.success("已导出");
  } catch (e) {
    message.error(String(e));
  }
}

/** 导入鼓励语 */
async function handleImport() {
  if (!importJson.value.trim()) {
    message.warning("请输入 JSON 内容");
    return;
  }
  importing.value = true;
  try {
    const result = await encApi.importEncouragements(importJson.value);
    message.success(
      `已导入 ${result.imported} 条，跳过 ${result.skipped} 条重复`,
    );
    showImportModal.value = false;
    importJson.value = "";
    await store.fetchAll();
  } catch (e) {
    message.error(String(e));
  } finally {
    importing.value = false;
  }
}
</script>

<template>
  <div class="space-y-4">
    <!-- 连续天数统计 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon icon="mdi:fire" width="22" class="text-orange-500" />
          <span>连续完成打卡</span>
        </div>
      </template>
      <template #header-extra>
        <NButton size="small" quaternary @click="handleRefreshStreak">
          <template #icon><Icon icon="mdi:refresh" /></template>
          刷新
        </NButton>
      </template>
      <div class="grid grid-cols-3 gap-4">
        <div class="text-center">
          <NStatistic label="当前连续" :value="store.streak.current_streak">
            <template #suffix>天</template>
          </NStatistic>
        </div>
        <div class="text-center">
          <NStatistic label="最长连续" :value="store.streak.longest_streak">
            <template #suffix>天</template>
          </NStatistic>
        </div>
        <div class="text-center">
          <NStatistic
            label="今日状态"
            :value="store.streak.completed_today ? '已完成' : '未完成'"
          />
        </div>
      </div>
      <div class="mt-3 text-xs text-gray-500">
        规则：每天至少完成一个任务计为打卡；当天无任务不中断也不计入；当天有任务但未完成则中断。
      </div>
    </NCard>

    <!-- 添加自定义鼓励语 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon
            icon="mdi:plus-circle-outline"
            width="20"
            class="text-brand-500"
          />
          <span>添加自定义鼓励语</span>
        </div>
      </template>
      <NSpace align="center" :wrap="false">
        <NInput
          v-model:value="newText"
          placeholder="输入鼓励语，回车快速添加"
          style="width: 360px"
          maxlength="100"
          show-count
          @keyup.enter="handleAdd"
        />
        <NSelect
          v-model:value="newLevel"
          :options="levelOptions"
          style="width: 180px"
        />
        <NButton type="primary" @click="handleAdd">
          <template #icon><Icon icon="mdi:plus" /></template>
          添加
        </NButton>
      </NSpace>
    </NCard>

    <!-- P1-6：搜索筛选栏 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon icon="mdi:filter-outline" width="20" class="text-brand-500" />
          <span>搜索筛选</span>
        </div>
      </template>
      <NSpace align="center" :wrap="true">
        <NInput
          v-model:value="searchKeyword"
          placeholder="关键词搜索"
          style="width: 200px"
          clearable
        />
        <NSelect
          v-model:value="filterLevel"
          placeholder="等级筛选"
          :options="[{ label: '全部等级', value: '' }, ...levelOptions]"
          style="width: 160px"
          clearable
        />
        <NSelect
          v-model:value="filterSource"
          placeholder="来源筛选"
          :options="[
            { label: '全部来源', value: '' },
            { label: '预设文案', value: 'preset' },
            { label: '自定义文案', value: 'custom' },
          ]"
          style="width: 140px"
          clearable
        />
        <NButton
          v-if="hasFilter"
          quaternary
          type="warning"
          @click="resetFilters"
        >
          重置筛选
        </NButton>
        <NButton quaternary @click="toggleBatchMode">
          <template #icon>
            <Icon
              :icon="
                batchMode
                  ? 'mdi:check-box-outline'
                  : 'mdi:checkbox-multiple-outline'
              "
            />
          </template>
          {{ batchMode ? "取消批量" : "批量操作" }}
        </NButton>
        <!-- P3-6：导入导出按钮 -->
        <NButton quaternary type="info" @click="handleExport">
          <template #icon><Icon icon="mdi:download" /></template>
          导出
        </NButton>
        <NButton quaternary type="info" @click="showImportModal = true">
          <template #icon><Icon icon="mdi:upload" /></template>
          导入
        </NButton>
      </NSpace>
      <div class="mt-2 text-xs text-gray-500">
        共 {{ filteredCustomList.length + filteredPresetList.length }} 条文案
      </div>
    </NCard>

    <!-- P1-5：批量操作工具栏 -->
    <NCard v-if="batchMode" :bordered="false" class="!bg-blue-50">
      <NSpace align="center" justify="space-between">
        <NSpace>
          <span class="text-sm">已选择 {{ selectedIds.size }} 条</span>
          <NSelect
            v-model:value="batchLevel"
            :options="levelOptions"
            style="width: 150px"
            placeholder="选择等级"
          />
          <NButton
            :disabled="selectedIds.size === 0"
            :loading="batchDeleting"
            @click="handleBatchUpdateLevel"
          >
            批量改等级
          </NButton>
          <NPopconfirm @positive-click="handleBatchDelete">
            <template #trigger>
              <NButton
                :disabled="selectedIds.size === 0"
                :loading="batchDeleting"
                type="error"
              >
                批量删除
              </NButton>
            </template>
            确定删除选中的 {{ selectedIds.size }} 条文案吗？预设文案不会被删除。
          </NPopconfirm>
        </NSpace>
        <NButton quaternary @click="toggleBatchMode">取消</NButton>
      </NSpace>
    </NCard>

    <!-- 按等级展示鼓励语 -->
    <template v-if="!hasFilter">
      <NCard
        v-for="level in [
          'normal',
          'advanced',
          'highlight',
          'celebration',
          'setback',
        ] as EncouragementLevel[]"
        :key="level"
        :bordered="false"
      >
        <template #header>
          <div class="flex items-center gap-2">
            <Icon
              :icon="LEVEL_META[level].icon"
              width="20"
              :class="LEVEL_META[level].iconColor"
            />
            <span>{{ LEVEL_META[level].label }}鼓励语</span>
            <NTag size="tiny" :type="LEVEL_META[level].color" round>
              {{ LEVEL_META[level].desc }}
            </NTag>
          </div>
        </template>
        <div
          v-if="
            presetByLevel[level].length > 0 || customByLevel[level].length > 0
          "
          class="space-y-2"
        >
          <div
            v-for="item in [...presetByLevel[level], ...customByLevel[level]]"
            :key="item.id"
            class="p-3 rounded border text-sm flex items-start gap-2"
            :class="{
              'border-blue-100 bg-blue-50/50': item.category === 'preset',
              'border-green-100 bg-green-50/50': item.category === 'custom',
            }"
          >
            <!-- P1-5：批量选择 checkbox -->
            <NCheckbox
              v-if="batchMode && item.category === 'custom'"
              :checked="selectedIds.has(item.id)"
              @update:checked="toggleSelect(item.id)"
            />
            <Icon
              icon="mdi:format-quote-open"
              width="16"
              class="text-gray-400 mt-0.5"
            />
            <span class="flex-1">{{ item.text }}</span>
            <NTag
              size="tiny"
              :bordered="false"
              :type="item.category === 'preset' ? 'info' : 'success'"
            >
              {{ item.category === "preset" ? "预设" : "自定义" }}
            </NTag>
            <!-- P3-2：收藏按钮 -->
            <NButton
              size="tiny"
              quaternary
              :type="store.isFavorite(item.id) ? 'warning' : 'default'"
              @click="toggleFavorite(item)"
            >
              <Icon
                :icon="
                  store.isFavorite(item.id) ? 'mdi:star' : 'mdi:star-outline'
                "
                width="14"
              />
            </NButton>
            <!-- P3-3：反馈按钮 -->
            <NButton
              size="tiny"
              quaternary
              type="success"
              @click="handleFeedback(item, 'like')"
            >
              <Icon icon="mdi:thumb-up-outline" width="14" />
            </NButton>
            <NButton
              size="tiny"
              quaternary
              type="error"
              @click="handleFeedback(item, 'dislike')"
            >
              <Icon icon="mdi:thumb-down-outline" width="14" />
            </NButton>
            <!-- P3-5：排序按钮（仅自定义文案） -->
            <NButton
              v-if="item.category === 'custom'"
              size="tiny"
              quaternary
              :disabled="batchMode"
              @click="moveUp(item, level)"
            >
              <Icon icon="mdi:chevron-up" width="14" />
            </NButton>
            <NButton
              v-if="item.category === 'custom'"
              size="tiny"
              quaternary
              :disabled="batchMode"
              @click="moveDown(item, level)"
            >
              <Icon icon="mdi:chevron-down" width="14" />
            </NButton>
            <!-- P0-5：编辑按钮（仅自定义文案） -->
            <NButton
              v-if="item.category === 'custom'"
              size="tiny"
              quaternary
              type="info"
              :disabled="deletingId !== null || batchMode"
              @click="openEditModal(item)"
            >
              <Icon icon="mdi:pencil-outline" width="14" />
            </NButton>
            <!-- P0-6：删除按钮加 loading + disabled 防连点 -->
            <NPopconfirm
              v-if="item.category === 'custom' && !batchMode"
              positive-text="确定"
              negative-text="取消"
              @positive-click="handleDelete(item)"
            >
              <template #trigger>
                <NButton
                  size="tiny"
                  quaternary
                  type="error"
                  :loading="deletingId === item.id"
                  :disabled="deletingId !== null && deletingId !== item.id"
                >
                  <Icon icon="mdi:delete" width="14" />
                </NButton>
              </template>
              确定删除这条鼓励语？
            </NPopconfirm>
          </div>
        </div>
        <NEmpty v-else :description="`暂无${LEVEL_META[level].label}鼓励语`" />
      </NCard>
    </template>

    <!-- 筛选结果（有筛选条件时显示） -->
    <NCard v-else :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon icon="mdi:text-search" width="20" class="text-brand-500" />
          <span>筛选结果</span>
          <NTag size="small" :bordered="false">
            共 {{ filteredCustomList.length + filteredPresetList.length }} 条
          </NTag>
        </div>
      </template>
      <div
        v-if="filteredCustomList.length > 0 || filteredPresetList.length > 0"
        class="space-y-2"
      >
        <div
          v-for="item in [...filteredPresetList, ...filteredCustomList]"
          :key="item.id"
          class="p-3 rounded border text-sm flex items-start gap-2"
          :class="{
            'border-blue-100 bg-blue-50/50': item.category === 'preset',
            'border-green-100 bg-green-50/50': item.category === 'custom',
          }"
        >
          <!-- P1-5：批量选择 checkbox -->
          <NCheckbox
            v-if="batchMode && item.category === 'custom'"
            :checked="selectedIds.has(item.id)"
            @update:checked="toggleSelect(item.id)"
          />
          <Icon
            icon="mdi:format-quote-open"
            width="16"
            class="text-gray-400 mt-0.5"
          />
          <span class="flex-1">{{ item.text }}</span>
          <NTag
            size="tiny"
            :bordered="false"
            :type="item.category === 'preset' ? 'info' : 'success'"
          >
            {{ item.category === "preset" ? "预设" : "自定义" }}
          </NTag>
          <NTag
            size="tiny"
            :bordered="false"
            :type="LEVEL_META[item.level].color"
          >
            {{ LEVEL_META[item.level].label }}
          </NTag>
          <!-- P3-2：收藏按钮 -->
          <NButton
            size="tiny"
            quaternary
            :type="store.isFavorite(item.id) ? 'warning' : 'default'"
            @click="toggleFavorite(item)"
          >
            <Icon
              :icon="
                store.isFavorite(item.id) ? 'mdi:star' : 'mdi:star-outline'
              "
              width="14"
            />
          </NButton>
          <!-- P3-3：反馈按钮 -->
          <NButton
            size="tiny"
            quaternary
            type="success"
            @click="handleFeedback(item, 'like')"
          >
            <Icon icon="mdi:thumb-up-outline" width="14" />
          </NButton>
          <NButton
            size="tiny"
            quaternary
            type="error"
            @click="handleFeedback(item, 'dislike')"
          >
            <Icon icon="mdi:thumb-down-outline" width="14" />
          </NButton>
          <!-- P0-5：编辑按钮 -->
          <NButton
            v-if="item.category === 'custom'"
            size="tiny"
            quaternary
            type="info"
            :disabled="deletingId !== null || batchMode"
            @click="openEditModal(item)"
          >
            <Icon icon="mdi:pencil-outline" width="14" />
          </NButton>
          <!-- P0-6：删除按钮 -->
          <NPopconfirm
            v-if="item.category === 'custom' && !batchMode"
            positive-text="确定"
            negative-text="取消"
            @positive-click="handleDelete(item)"
          >
            <template #trigger>
              <NButton
                size="tiny"
                quaternary
                type="error"
                :loading="deletingId === item.id"
                :disabled="deletingId !== null && deletingId !== item.id"
              >
                <Icon icon="mdi:delete" width="14" />
              </NButton>
            </template>
            确定删除这条鼓励语？
          </NPopconfirm>
        </div>
      </div>
      <NEmpty v-else description="没有找到匹配的文案" />
    </NCard>

    <!-- P0-5：编辑鼓励语弹窗 -->
    <NModal
      v-model:show="showEditModal"
      preset="card"
      title="编辑鼓励语"
      style="width: 420px"
    >
      <NForm label-placement="top">
        <NFormItem label="鼓励语内容">
          <NInput
            v-model:value="editText"
            type="textarea"
            :autosize="{ minRows: 2, maxRows: 4 }"
            maxlength="100"
            show-count
            placeholder="输入鼓励语内容"
          />
        </NFormItem>
        <NFormItem label="等级">
          <NSelect
            v-model:value="editLevel"
            :options="levelOptions"
            style="width: 100%"
          />
        </NFormItem>
      </NForm>
      <template #footer>
        <NSpace justify="end">
          <NButton :disabled="saving" @click="showEditModal = false">
            取消
          </NButton>
          <NButton type="primary" :loading="saving" @click="handleEditSave">
            保存
          </NButton>
        </NSpace>
      </template>
    </NModal>

    <!-- P3-6：导入鼓励语弹窗 -->
    <NModal
      v-model:show="showImportModal"
      preset="card"
      title="导入鼓励语"
      style="width: 500px"
    >
      <NForm label-placement="top">
        <NFormItem label="JSON 内容">
          <NInput
            v-model:value="importJson"
            type="textarea"
            :autosize="{ minRows: 6, maxRows: 12 }"
            placeholder="粘贴从导出功能生成的 JSON 内容"
          />
        </NFormItem>
      </NForm>
      <div class="text-xs text-gray-500 mb-3">
        支持导入鼓励语 JSON 文件，重复文案会自动跳过
      </div>
      <template #footer>
        <NSpace justify="end">
          <NButton :disabled="importing" @click="showImportModal = false">
            取消
          </NButton>
          <NButton type="primary" :loading="importing" @click="handleImport">
            导入
          </NButton>
        </NSpace>
      </template>
    </NModal>
  </div>
</template>
