<script setup lang="ts">
import { ref, computed, onMounted, watch, nextTick } from "vue";
import {
  NCard,
  NEmpty,
  NRadioGroup,
  NRadioButton,
  NStatistic,
  useMessage,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import * as echarts from "echarts/core";
import { BarChart, LineChart, HeatmapChart } from "echarts/charts";
import {
  TitleComponent,
  TooltipComponent,
  GridComponent,
  LegendComponent,
  DataZoomComponent,
  CalendarComponent,
  VisualMapComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import type {
  GoalCompletionStat,
  DailyTrend,
  HeatmapCell,
  CompletionPrediction,
  PredictionStatus,
  TagType,
} from "@/types";
import * as statsApi from "@/api/stats";
import { format, subDays } from "date-fns";

echarts.use([
  BarChart,
  LineChart,
  HeatmapChart,
  TitleComponent,
  TooltipComponent,
  GridComponent,
  LegendComponent,
  DataZoomComponent,
  CalendarComponent,
  VisualMapComponent,
  CanvasRenderer,
]);

const message = useMessage();
const trendDays = ref<7 | 30>(7);
const goalStats = ref<GoalCompletionStat[]>([]);
const trend = ref<DailyTrend[]>([]);
const heatmapData = ref<HeatmapCell[]>([]);
const heatmapRange = ref<90 | 180 | 365>(90);
const predictions = ref<CompletionPrediction[]>([]);
const loading = ref(false);

const barChartRef = ref<HTMLDivElement | null>(null);
const lineChartRef = ref<HTMLDivElement | null>(null);
const heatmapChartRef = ref<HTMLDivElement | null>(null);
let barChart: echarts.ECharts | null = null;
let lineChart: echarts.ECharts | null = null;
let heatmapChart: echarts.ECharts | null = null;

onMounted(async () => {
  await Promise.all([
    loadGoalStats(),
    loadTrend(),
    loadHeatmap(),
    loadPredictions(),
  ]);
  await nextTick();
  renderCharts();
});

watch(trendDays, async () => {
  await loadTrend();
  await nextTick();
  renderLineChart();
});

watch(heatmapRange, async () => {
  await loadHeatmap();
  await nextTick();
  renderHeatmap();
});

async function loadGoalStats() {
  try {
    goalStats.value = await statsApi.getGoalCompletionStats();
  } catch (e) {
    message.error(String(e));
  }
}

async function loadTrend() {
  loading.value = true;
  try {
    trend.value = await statsApi.getCompletionTrend(trendDays.value);
  } catch (e) {
    message.error(String(e));
  } finally {
    loading.value = false;
  }
}

async function loadHeatmap() {
  try {
    const today = new Date();
    const start = subDays(today, heatmapRange.value - 1);
    heatmapData.value = await statsApi.getHeatmap(
      format(start, "yyyy-MM-dd"),
      format(today, "yyyy-MM-dd"),
    );
  } catch (e) {
    message.error(String(e));
  }
}

async function loadPredictions() {
  try {
    predictions.value = await statsApi.getCompletionPredictions();
  } catch (e) {
    message.error(String(e));
  }
}

/** 预测状态元信息 */
const PREDICTION_META: Record<
  PredictionStatus,
  { label: string; color: TagType; icon: string; iconColor: string }
> = {
  on_track: {
    label: "按期完成",
    color: "success",
    icon: "mdi:check-circle",
    iconColor: "text-green-500",
  },
  ahead: {
    label: "可提前",
    color: "info",
    icon: "mdi:rocket-launch",
    iconColor: "text-blue-500",
  },
  need_speed: {
    label: "需提速",
    color: "error",
    icon: "mdi:alert-circle",
    iconColor: "text-red-500",
  },
  no_deadline: {
    label: "无截止",
    color: "default",
    icon: "mdi:calendar-remove",
    iconColor: "text-gray-400",
  },
  no_data: {
    label: "无数据",
    color: "default",
    icon: "mdi:help-circle",
    iconColor: "text-gray-400",
  },
  completed: {
    label: "已完成",
    color: "success",
    icon: "mdi:trophy",
    iconColor: "text-yellow-500",
  },
};

function renderCharts() {
  renderBarChart();
  renderLineChart();
  renderHeatmap();
}

function renderBarChart() {
  if (!barChartRef.value) return;
  if (!barChart) barChart = echarts.init(barChartRef.value);

  const names = goalStats.value.map((g) => g.name);
  const percentages = goalStats.value.map((g) =>
    Math.round(g.percentage * 100),
  );
  const doneCounts = goalStats.value.map((g) => g.done_count);
  const taskCounts = goalStats.value.map((g) => g.task_count);

  barChart.setOption({
    tooltip: {
      trigger: "axis",
      axisPointer: { type: "shadow" },
      formatter: (params: any) => {
        const idx = params[0].dataIndex;
        const g = goalStats.value[idx];
        if (!g) return "";
        return `${g.name}<br/>
          完成度：${Math.round(g.percentage * 100)}%<br/>
          已完成：${g.done_count}/${g.task_count} 任务<br/>
          实际/计划：${g.total_actual}/${g.total_plan}`;
      },
    },
    grid: { left: 50, right: 30, top: 30, bottom: 50 },
    xAxis: {
      type: "category",
      data: names,
      axisLabel: { interval: 0, rotate: names.length > 4 ? 30 : 0 },
    },
    yAxis: [
      {
        type: "value",
        name: "完成度(%)",
        max: 100,
        axisLabel: { formatter: "{value}%" },
      },
    ],
    series: [
      {
        name: "完成度",
        type: "bar",
        data: percentages,
        itemStyle: {
          color: (params: any) => {
            const v = params.value as number;
            if (v >= 80) return "#67c23a";
            if (v >= 50) return "#e6a23c";
            if (v >= 20) return "#f56c6c";
            return "#909399";
          },
        },
        label: { show: true, position: "top", formatter: "{c}%" },
        barWidth: "50%",
      },
      {
        name: "任务进度",
        type: "bar",
        data: taskCounts.map((total, i) => ({
          value: (doneCounts[i] / (total || 1)) * 100,
          total,
          done: doneCounts[i],
        })),
        itemStyle: { color: "rgba(64, 158, 255, 0.3)" },
        barGap: "-100%",
        barWidth: "50%",
        tooltip: {
          formatter: (p: any) => `已完成 ${p.data.done}/${p.data.total} 任务`,
        },
      },
    ],
  });
}

function renderLineChart() {
  if (!lineChartRef.value) return;
  if (!lineChart) lineChart = echarts.init(lineChartRef.value);

  const dates = trend.value.map((t) => t.date.slice(5)); // MM-dd
  const qtySeries = trend.value.map((t) => t.completed_qty);
  const countSeries = trend.value.map((t) => t.completed_count);

  lineChart.setOption({
    tooltip: {
      trigger: "axis",
      formatter: (params: any) => {
        const idx = params[0].dataIndex;
        const t = trend.value[idx];
        if (!t) return "";
        return `${t.date}<br/>完成数量：${t.completed_qty}<br/>完成任务：${t.completed_count} 个`;
      },
    },
    legend: { data: ["完成数量", "完成任务数"], top: 0 },
    grid: {
      left: 50,
      right: 30,
      top: 40,
      bottom: trendDays.value === 30 ? 60 : 30,
    },
    xAxis: {
      type: "category",
      data: dates,
      axisLabel: {
        interval: trendDays.value === 30 ? 4 : 0,
        rotate: trendDays.value === 30 ? 30 : 0,
      },
    },
    yAxis: [
      { type: "value", name: "完成数量", minInterval: 1 },
      { type: "value", name: "任务数", minInterval: 1 },
    ],
    series: [
      {
        name: "完成数量",
        type: "line",
        data: qtySeries,
        smooth: true,
        areaStyle: { opacity: 0.2 },
        itemStyle: { color: "#67c23a" },
        lineStyle: { width: 2 },
      },
      {
        name: "完成任务数",
        type: "line",
        yAxisIndex: 1,
        data: countSeries,
        smooth: true,
        itemStyle: { color: "#409eff" },
        lineStyle: { width: 2, type: "dashed" },
      },
    ],
  });
}

/** 渲染日历热力图 */
function renderHeatmap() {
  if (!heatmapChartRef.value) return;
  if (!heatmapChart) heatmapChart = echarts.init(heatmapChartRef.value);

  // 转换为 [date, value] 格式，value 为完成率（0-1）
  const data: [string, number][] = heatmapData.value.map((cell) => [
    cell.date,
    Math.round(cell.completion_rate * 100) / 100,
  ]);

  const today = format(new Date(), "yyyy-MM-dd");
  const start = format(
    subDays(new Date(), heatmapRange.value - 1),
    "yyyy-MM-dd",
  );

  heatmapChart.setOption({
    tooltip: {
      formatter: (params: any) => {
        const cell = heatmapData.value.find((c) => c.date === params.value[0]);
        if (!cell) return "";
        const rate = Math.round(cell.completion_rate * 100);
        return `${cell.date}<br/>
          完成率：${rate}%<br/>
          任务：${cell.done_count}/${cell.task_count}<br/>
          数量：${cell.completed_qty}/${cell.plan_qty}`;
      },
    },
    visualMap: {
      min: 0,
      max: 1,
      show: true,
      orient: "horizontal",
      left: "center",
      bottom: 0,
      inRange: {
        color: ["#ebedf0", "#c6e48b", "#7bc96f", "#239a3b", "#196127"],
      },
      text: ["100%", "0%"],
      textStyle: { fontSize: 11 },
    },
    calendar: {
      top: 60,
      left: 50,
      right: 30,
      cellSize: ["auto", 16],
      range: [start, today],
      itemStyle: {
        borderWidth: 2,
        borderColor: "#fff",
        color: "#ebedf0",
      },
      yearLabel: { show: false },
      dayLabel: {
        firstDay: 1,
        nameMap: "cn",
        color: "#909399",
        fontSize: 11,
      },
      monthLabel: {
        nameMap: "cn",
        color: "#606266",
        fontSize: 11,
      },
      splitLine: { show: false },
    },
    series: [
      {
        type: "heatmap",
        coordinateSystem: "calendar",
        data,
        itemStyle: {
          borderRadius: 3,
        },
      },
    ],
  });
}

// 汇总统计
const totalCompletedCount = computed(() =>
  trend.value.reduce((sum, t) => sum + t.completed_count, 0),
);
const totalCompletedQty = computed(() =>
  trend.value.reduce((sum, t) => sum + t.completed_qty, 0),
);
const avgDailyQty = computed(() =>
  trend.value.length > 0
    ? (totalCompletedQty.value / trend.value.length).toFixed(1)
    : "0",
);
const activeDays = computed(
  () => trend.value.filter((t) => t.completed_count > 0).length,
);

// 自适应窗口大小
function handleResize() {
  barChart?.resize();
  lineChart?.resize();
  heatmapChart?.resize();
}
onMounted(() => {
  window.addEventListener("resize", handleResize);
});
</script>

<template>
  <div class="space-y-4">
    <!-- 顶部汇总卡片 -->
    <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
      <NCard :bordered="false" size="small">
        <NStatistic
          :label="`近 ${trendDays} 天完成任务数`"
          :value="totalCompletedCount"
        >
          <template #suffix>个</template>
        </NStatistic>
      </NCard>
      <NCard :bordered="false" size="small">
        <NStatistic
          :label="`近 ${trendDays} 天完成数量`"
          :value="totalCompletedQty"
        />
      </NCard>
      <NCard :bordered="false" size="small">
        <NStatistic label="日均完成数量" :value="avgDailyQty" />
      </NCard>
      <NCard :bordered="false" size="small">
        <NStatistic label="活跃天数" :value="activeDays">
          <template #suffix>/ {{ trendDays }}</template>
        </NStatistic>
      </NCard>
    </div>

    <!-- 目标完成百分比柱状图 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon icon="mdi:chart-bar" width="20" class="text-brand-500" />
          <span>各目标完成百分比</span>
        </div>
      </template>
      <div
        v-if="goalStats.length > 0"
        ref="barChartRef"
        style="height: 320px"
      />
      <NEmpty v-else description="还没有目标数据" />
    </NCard>

    <!-- 日历热力图 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon
            icon="mdi:calendar-blank-outline"
            width="20"
            class="text-brand-500"
          />
          <span>日历热力图</span>
        </div>
      </template>
      <template #header-extra>
        <NRadioGroup v-model:value="heatmapRange" size="small">
          <NRadioButton :value="90">近 90 天</NRadioButton>
          <NRadioButton :value="180">近 180 天</NRadioButton>
          <NRadioButton :value="365">近 1 年</NRadioButton>
        </NRadioGroup>
      </template>
      <div ref="heatmapChartRef" style="height: 240px" />
      <div class="mt-2 text-xs text-gray-500 text-center">
        颜色深浅表示当日"完成任务量 / 计划任务总量"的完成比例
      </div>
    </NCard>

    <!-- 完成趋势折线图 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon
            icon="mdi:chart-line-variant"
            width="20"
            class="text-brand-500"
          />
          <span>完成趋势</span>
        </div>
      </template>
      <template #header-extra>
        <NRadioGroup v-model:value="trendDays" size="small">
          <NRadioButton :value="7">近 7 天</NRadioButton>
          <NRadioButton :value="30">近 30 天</NRadioButton>
        </NRadioGroup>
      </template>
      <div ref="lineChartRef" style="height: 340px" />
    </NCard>

    <!-- 目标明细列表 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon
            icon="mdi:format-list-bulleted"
            width="20"
            class="text-brand-500"
          />
          <span>目标明细</span>
        </div>
      </template>
      <div v-if="goalStats.length > 0" class="space-y-2">
        <div
          v-for="g in goalStats"
          :key="g.id"
          class="flex items-center gap-3 p-3 rounded border border-gray-100"
        >
          <div class="flex-1">
            <div class="font-medium">{{ g.name }}</div>
            <div class="text-xs text-gray-500 mt-1">
              已完成 {{ g.done_count }}/{{ g.task_count }} 任务 · 累计
              {{ g.total_actual }}/{{ g.total_plan }}
            </div>
          </div>
          <div class="text-right">
            <div
              class="text-2xl font-bold"
              :class="{
                'text-green-500': g.percentage >= 0.8,
                'text-orange-500': g.percentage >= 0.5 && g.percentage < 0.8,
                'text-red-500': g.percentage < 0.5,
              }"
            >
              {{ Math.round(g.percentage * 100) }}%
            </div>
          </div>
        </div>
      </div>
      <NEmpty v-else description="还没有目标数据" />
    </NCard>

    <!-- 完成预测 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon icon="mdi:crystal-ball" width="20" class="text-brand-500" />
          <span>完成预测</span>
        </div>
      </template>
      <template #header-extra>
        <span class="text-xs text-gray-400">基于过去 7 天平均速度</span>
      </template>
      <div v-if="predictions.length > 0" class="space-y-2">
        <div
          v-for="p in predictions"
          :key="p.goal_id"
          class="p-3 rounded border flex items-start gap-3"
          :class="{
            'border-green-100 bg-green-50/30':
              p.status === 'on_track' || p.status === 'completed',
            'border-blue-100 bg-blue-50/30': p.status === 'ahead',
            'border-red-100 bg-red-50/30': p.status === 'need_speed',
            'border-gray-100':
              p.status === 'no_deadline' || p.status === 'no_data',
          }"
        >
          <Icon
            :icon="PREDICTION_META[p.status].icon"
            width="22"
            :class="PREDICTION_META[p.status].iconColor"
            class="mt-0.5"
          />
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <span class="font-medium">{{ p.goal_name }}</span>
              <NTag size="tiny" :type="PREDICTION_META[p.status].color" round>
                {{ PREDICTION_META[p.status].label }}
              </NTag>
            </div>
            <div
              class="text-xs text-gray-500 mt-1 flex flex-wrap gap-x-3 gap-y-0.5"
            >
              <span>剩余 {{ p.remaining_qty }}/{{ p.total_qty }}</span>
              <span>日均速度 {{ p.avg_daily_speed.toFixed(1) }}</span>
              <span v-if="p.predicted_days !== null">
                预计需 {{ p.predicted_days }} 天
              </span>
              <span v-if="p.predicted_date">
                预计 {{ p.predicted_date }} 完成
              </span>
              <span v-if="p.days_to_deadline !== null">
                <template v-if="p.days_to_deadline < 0">
                  已逾期 {{ -p.days_to_deadline }} 天
                </template>
                <template v-else> 距截止 {{ p.days_to_deadline }} 天 </template>
              </span>
            </div>
            <div class="text-sm mt-1.5 text-gray-700">
              {{ p.suggestion }}
            </div>
          </div>
        </div>
      </div>
      <NEmpty v-else description="还没有目标数据" />
    </NCard>
  </div>
</template>
