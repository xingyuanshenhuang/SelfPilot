<script setup lang="ts">
import { useEditor, EditorContent } from "@tiptap/vue-3";
import { StarterKit } from "@tiptap/starter-kit";
import { TextStyleKit } from "@tiptap/extension-text-style";
import { Placeholder } from "@tiptap/extensions";
import { NButton, NSelect, NTooltip, NDivider } from "naive-ui";
import { Icon } from "@iconify/vue";
import { ref, watch } from "vue";

/**
 * 富文本编辑器（Tiptap 3 + Naive UI 自建工具栏）
 *
 * - modelValue 为 HTML 字符串（未净化也可，保存后由后端统一白名单净化）
 * - 支持：加粗 / 斜体 / 下划线 / 删除线 / 字号 / 字体颜色 / 列表 / 清除格式
 * - 暗色模式随 html.dark 切换
 */

const props = withDefaults(
  defineProps<{
    modelValue: string;
    placeholder?: string;
    /** 编辑区最小高度 */
    minHeight?: string;
  }>(),
  { modelValue: "", placeholder: "", minHeight: "96px" },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
}>();

const fontSizes = [
  { label: "默认", value: "" },
  { label: "12px", value: "12px" },
  { label: "14px", value: "14px" },
  { label: "16px", value: "16px" },
  { label: "18px", value: "18px" },
  { label: "24px", value: "24px" },
  { label: "32px", value: "32px" },
];

const colorPalette = [
  "#000000",
  "#888888",
  "#ffffff",
  "#d0021b",
  "#f5a623",
  "#f8e71c",
  "#7ed321",
  "#2080f0",
  "#9013fe",
];

const editor = useEditor({
  content: props.modelValue,
  extensions: [
    StarterKit,
    TextStyleKit,
    Placeholder.configure({ placeholder: props.placeholder }),
  ],
  editorProps: {
    attributes: {
      class: "richtext-content",
    },
  },
  onUpdate: ({ editor }) => {
    emit("update:modelValue", editor.getHTML());
  },
});

// 外部值回填：仅在内容实际变化时 setContent 且不触发 emitUpdate，避免光标跳动
watch(
  () => props.modelValue,
  (val) => {
    if (!editor.value) return;
    const next = val || "";
    if (next !== editor.value.getHTML()) {
      editor.value.commands.setContent(next, { emitUpdate: false });
    }
  },
);

// 预定义主题色按钮
function applyColor(color: string) {
  editor.value?.chain().focus().setColor(color).run();
}

function resetColor() {
  editor.value?.chain().focus().unsetColor().run();
}

const currentFontSize = ref<string>("");
function onFontSizeChange(size: string) {
  if (size) {
    editor.value?.chain().focus().setFontSize(size).run();
  } else {
    editor.value?.chain().focus().unsetFontSize().run();
  }
  currentFontSize.value = size;
}
function syncDisabled() {
  if (!editor.value) return;
  const attrs = editor.value.getAttributes("textStyle");
  currentFontSize.value = (attrs.fontSize as string) || "";
}
// 选区变化时同步按钮高亮状态
function updateToolbar() {
  syncDisabled();
}
</script>

<template>
  <div
    class="border border-[#d8d8d9] dark:border-[rgba(255,255,255,0.2)] rounded overflow-hidden"
  >
    <!-- 工具栏 -->
    <div
      class="flex flex-wrap items-center gap-1 px-2 py-1.5 border-b border-gray-200 dark:border-surface-borderMuted bg-gray-50 dark:bg-surface-muted"
    >
      <NTooltip placement="bottom">
        <template #trigger>
          <NButton
            size="tiny"
            quaternary
            :type="editor?.isActive('bold') ? 'primary' : 'default'"
            @click="editor?.chain().focus().toggleBold().run()"
          >
            <template #icon><Icon icon="mdi:format-bold" /></template>
          </NButton>
        </template>
        加粗
      </NTooltip>
      <NTooltip placement="bottom">
        <template #trigger>
          <NButton
            size="tiny"
            quaternary
            :type="editor?.isActive('italic') ? 'primary' : 'default'"
            @click="editor?.chain().focus().toggleItalic().run()"
          >
            <template #icon><Icon icon="mdi:format-italic" /></template>
          </NButton>
        </template>
        斜体
      </NTooltip>
      <NTooltip placement="bottom">
        <template #trigger>
          <NButton
            size="tiny"
            quaternary
            :type="editor?.isActive('underline') ? 'primary' : 'default'"
            @click="editor?.chain().focus().toggleUnderline().run()"
          >
            <template #icon><Icon icon="mdi:format-underline" /></template>
          </NButton>
        </template>
        下划线
      </NTooltip>
      <NTooltip placement="bottom">
        <template #trigger>
          <NButton
            size="tiny"
            quaternary
            :type="editor?.isActive('strike') ? 'primary' : 'default'"
            @click="editor?.chain().focus().toggleStrike().run()"
          >
            <template #icon><Icon icon="mdi:format-strikethrough" /></template>
          </NButton>
        </template>
        删除线
      </NTooltip>
      <NTooltip placement="bottom">
        <template #trigger>
          <NButton
            size="tiny"
            quaternary
            :type="editor?.isActive('bulletList') ? 'primary' : 'default'"
            @click="editor?.chain().focus().toggleBulletList().run()"
          >
            <template #icon><Icon icon="mdi:format-list-bulleted" /></template>
          </NButton>
        </template>
        无序列表
      </NTooltip>
      <NTooltip placement="bottom">
        <template #trigger>
          <NButton
            size="tiny"
            quaternary
            :type="editor?.isActive('orderedList') ? 'primary' : 'default'"
            @click="editor?.chain().focus().toggleOrderedList().run()"
          >
            <template #icon><Icon icon="mdi:format-list-numbered" /></template>
          </NButton>
        </template>
        有序列表
      </NTooltip>

      <NDivider vertical />

      <!-- 字号 -->
      <NSelect
        :value="currentFontSize"
        :options="fontSizes"
        size="tiny"
        style="width: 80px"
        placeholder="字号"
        @update:value="onFontSizeChange"
        @focus="syncDisabled"
      />

      <NDivider vertical />

      <!-- 颜色 -->
      <div class="flex items-center gap-1">
        <NTooltip placement="bottom">
          <template #trigger>
            <NButton size="tiny" quaternary class="w-6 p-0" @click="resetColor">
              <template #icon><Icon icon="mdi:format-color" /></template>
            </NButton>
          </template>
          恢复默认颜色
        </NTooltip>
        <button
          v-for="c in colorPalette"
          :key="c"
          type="button"
          :style="{ backgroundColor: c }"
          class="w-4 h-4 rounded-full border border-gray-300 dark:border-surface-borderMuted hover:scale-110 transition-transform"
          :title="c"
          @click="applyColor(c)"
        />
      </div>

      <div class="flex-1" />

      <!-- 清除格式 -->
      <NTooltip placement="bottom">
        <template #trigger>
          <NButton
            size="tiny"
            quaternary
            type="error"
            @click="editor?.chain().focus().clearNodes().unsetAllMarks().run()"
          >
            <template #icon><Icon icon="mdi:format-clear" /></template>
          </NButton>
        </template>
        清除格式
      </NTooltip>
    </div>

    <!-- 编辑区 -->
    <div
      class="rte-body min-h-[var(--rte-min-height)] max-h-60 overflow-y-auto px-3 py-2 bg-white dark:bg-transparent"
      :style="{ '--rte-min-height': minHeight }"
    >
      <EditorContent :editor="editor" @selectionchange="updateToolbar" />
    </div>
  </div>
</template>

<style scoped>
/* 编辑区内容（Tiptap 运行时生成的 DOM 无 data-v，须用 :deep() 作用于后代） */
.rte-body :deep(.richtext-content) {
  min-height: var(--rte-min-height, 96px);
  outline: none;
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 14px;
  line-height: 1.6;
  color: inherit;
}
.rte-body :deep(.richtext-content p) {
  margin: 0.25rem 0;
}
.rte-body :deep(.richtext-content ul),
.rte-body :deep(.richtext-content ol) {
  padding-left: 1.25rem;
  margin: 0.25rem 0;
}
.rte-body :deep(.richtext-content blockquote) {
  border-left: 3px solid #d0d5dd;
  padding-left: 0.75rem;
  color: #6b7280;
  margin: 0.25rem 0;
}
.rte-body :deep(.richtext-content pre) {
  background: #f5f5f5;
  padding: 0.5rem;
  border-radius: 4px;
  overflow-x: auto;
}
.dark .rte-body :deep(.richtext-content blockquote) {
  border-left-color: #4b5563;
  color: #9ca3af;
}
.dark .rte-body :deep(.richtext-content pre) {
  background: #1f2937;
}

/* 占位提示 */
.rte-body :deep(p.is-editor-empty:first-child::before) {
  content: attr(data-placeholder);
  float: left;
  color: #adb5bd;
  pointer-events: none;
  height: 0;
}
</style>
