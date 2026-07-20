<script setup lang="ts">
import { ref, computed, watch, onMounted, defineAsyncComponent } from "vue";
import {
  NLayout,
  NLayoutSider,
  NLayoutContent,
  NMenu,
  NMessageProvider,
  NDialogProvider,
  NConfigProvider,
  NScrollbar,
  NModal,
  NButton,
  NSpace,
  darkTheme,
} from "naive-ui";
import type { MenuOption } from "naive-ui";
import { Icon } from "@iconify/vue";
import { h } from "vue";
import { useSettingStore } from "@/stores/settingStore";
import { useTaskStore } from "@/stores/taskStore";
import { useEncouragementStore } from "@/stores/encouragementStore";
import CelebrationModal from "@/components/CelebrationModal.vue";
import { getCelebrationAchievement } from "@/api/stats";
import type { CelebrationAchievement } from "@/types";

const activeView = ref<string>("dashboard");

const settingStore = useSettingStore();
const taskStore = useTaskStore();
const encStore = useEncouragementStore();

const DashboardView = defineAsyncComponent(
  () => import("./views/DashboardView.vue"),
);
const GoalTreeView = defineAsyncComponent(
  () => import("./views/GoalTreeView.vue"),
);
const CalendarView = defineAsyncComponent(
  () => import("./views/CalendarView.vue"),
);
const StatsView = defineAsyncComponent(() => import("./views/StatsView.vue"));
const EncouragementView = defineAsyncComponent(
  () => import("./views/EncouragementView.vue"),
);
const SettingsView = defineAsyncComponent(
  () => import("./views/SettingsView.vue"),
);

// 启动时从后端加载主题设置
onMounted(async () => {
  await settingStore.loadTheme();
});

const menuOptions: MenuOption[] = [
  {
    label: "目标总览",
    key: "dashboard",
    icon: () => h(Icon, { icon: "mdi:view-dashboard-outline" }),
  },
  {
    label: "目标树",
    key: "goalTree",
    icon: () => h(Icon, { icon: "mdi:file-tree-outline" }),
  },
  {
    label: "日历视图",
    key: "calendar",
    icon: () => h(Icon, { icon: "mdi:calendar-month-outline" }),
  },
  {
    label: "数据统计",
    key: "stats",
    icon: () => h(Icon, { icon: "mdi:chart-line" }),
  },
  {
    label: "鼓励语库",
    key: "encouragement",
    icon: () => h(Icon, { icon: "mdi:emoticon-happy-outline" }),
  },
  {
    label: "设置",
    key: "settings",
    icon: () => h(Icon, { icon: "mdi:cog-outline" }),
  },
];

const currentView = computed(() => {
  switch (activeView.value) {
    case "dashboard":
      return DashboardView;
    case "goalTree":
      return GoalTreeView;
    case "calendar":
      return CalendarView;
    case "stats":
      return StatsView;
    case "encouragement":
      return EncouragementView;
    case "settings":
      return SettingsView;
    default:
      return null;
  }
});

// PRD §4.2 模块七 & Sprint 5：完成当日首个任务后弹出鼓励语弹窗
// Sprint 5：支持等级触发（1/3/7天 + 全部完成庆祝）
// P1-3：celebration 使用全屏庆祝弹窗
const showEncouragementModal = ref(false);
const showCelebrationModal = ref(false);
const celebrationAchievement = ref<CelebrationAchievement | null>(null);

watch(
  () => taskStore.pendingEncouragement,
  async (enc) => {
    if (!enc) return;

    if (taskStore.isCelebration) {
      // P1-3：全部目标完成，显示庆祝弹窗
      if (encStore.settings.celebration_animation) {
        try {
          celebrationAchievement.value = await getCelebrationAchievement();
          showCelebrationModal.value = true;
        } catch (e) {
          console.warn("获取庆祝数据失败:", e);
          // 降级：使用普通弹窗
          showEncouragementModal.value = true;
        }
      } else {
        // 用户关闭了庆祝动画，使用普通弹窗
        showEncouragementModal.value = true;
      }
    } else {
      // 普通鼓励语弹窗
      showEncouragementModal.value = true;
    }
  },
);

function closeEncouragementModal() {
  showEncouragementModal.value = false;
  taskStore.clearPendingEncouragement();
}

function closeCelebrationModal() {
  showCelebrationModal.value = false;
  taskStore.clearPendingEncouragement();
}

const themeProvider = computed(() => (settingStore.isDark ? darkTheme : null));

/** 鼓励语弹窗标题（庆祝 vs 普通） */
const encouragementTitle = computed(() =>
  taskStore.isCelebration ? "全部目标完成！" : "今日首完成！",
);
const encouragementIcon = computed(() =>
  taskStore.isCelebration ? "mdi:trophy-award" : "mdi:star-four-points",
);
const encouragementHeaderColor = computed(() =>
  taskStore.isCelebration ? "text-orange-500" : "text-brand-600",
);
const encouragementBodyIcon = computed(() =>
  taskStore.isCelebration ? "mdi:trophy" : "mdi:emoticon-excited-outline",
);
const encouragementBodyColor = computed(() =>
  taskStore.isCelebration ? "text-yellow-500" : "text-orange-400",
);
</script>

<template>
  <n-config-provider :theme="themeProvider">
    <n-message-provider>
      <n-dialog-provider>
        <n-layout has-sider class="h-screen">
          <n-layout-sider
            bordered
            :width="208"
            :collapsed-width="64"
            show-trigger="bar"
            collapse-mode="width"
          >
            <div
              class="logo flex items-center gap-2 px-4 py-4 text-lg font-bold text-brand-600"
            >
              <Icon icon="mdi:rocket-launch" width="24" />
              <span v-if="true">SelfPilot</span>
            </div>
            <n-menu
              v-model:value="activeView"
              :options="menuOptions"
              :indent="18"
              :collapsed-width="64"
              :collapsed-icon-size="20"
            />
          </n-layout-sider>
          <n-layout>
            <n-layout-content class="h-full">
              <n-scrollbar>
                <div class="p-4">
                  <component :is="currentView" v-if="currentView" />
                  <div
                    v-else
                    class="flex h-full items-center justify-center text-gray-400"
                  >
                    <div class="text-center">
                      <Icon icon="mdi:hammer-wrench" width="48" />
                      <p class="mt-2">该模块开发中</p>
                    </div>
                  </div>
                </div>
              </n-scrollbar>
            </n-layout-content>
          </n-layout>
        </n-layout>

        <!-- 完成首任务鼓励语弹窗 -->
        <NModal
          :show="showEncouragementModal"
          preset="card"
          :bordered="false"
          style="width: 420px"
          :mask-closable="true"
          @update:show="(v: boolean) => !v && closeEncouragementModal()"
        >
          <template #header>
            <div
              class="flex items-center gap-2"
              :class="encouragementHeaderColor"
            >
              <Icon :icon="encouragementIcon" width="22" />
              <span>{{ encouragementTitle }}</span>
            </div>
          </template>
          <div class="py-4 text-center">
            <Icon
              :icon="encouragementBodyIcon"
              width="48"
              :class="[encouragementBodyColor, 'mx-auto mb-3']"
            />
            <div class="text-base text-gray-700 px-4 leading-relaxed">
              {{ taskStore.pendingEncouragement?.text }}
            </div>
          </div>
          <template #footer>
            <NSpace justify="center">
              <NButton type="primary" @click="closeEncouragementModal">
                {{ taskStore.isCelebration ? "感谢陪伴" : "继续加油" }}
              </NButton>
            </NSpace>
          </template>
        </NModal>

        <!-- P1-3：庆祝弹窗（全部目标完成） -->
        <CelebrationModal
          v-model:show="showCelebrationModal"
          :achievement="celebrationAchievement"
          :encouragement="taskStore.pendingEncouragement"
          :animation-enabled="encStore.settings.celebration_animation"
          @close="closeCelebrationModal"
        />
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>

<style>
html,
body,
#app {
  margin: 0;
  padding: 0;
  height: 100%;
  font-family:
    -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue",
    Arial, "PingFang SC", "Microsoft YaHei", sans-serif;
}
</style>
