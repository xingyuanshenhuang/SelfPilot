# P2-5 跨目标负载平衡 — 实现计划

## Context

当前多目标独立拆解任务，互不感知。某天可能因多个目标同时拆解导致任务爆炸。用户只能在日历中事后看到任务堆积，无法提前感知。P2-5 引入跨目标负载感知：

1. **每日负载查询** — 后端新增 `get_daily_load` 命令，按日期聚合所有目标的任务量
2. **日历色阶** — 月视图日期格子根据负载显示背景色阶（绿/黄/红）
3. **拆解时过载警告** — 自动拆解前检查目标日期范围内的负载，超阈值时弹窗警告

## 现状分析

| 组件 | 现状 |
|------|------|
| `src-tauri/src/commands/stats.rs` | **已存在**，含 `get_heatmap`、`get_completion_trend` 等。**缺少** `get_daily_load` |
| `src/api/stats.ts` | **已存在**，封装了 4 个 stats 命令。**缺少** `getDailyLoad` |
| `src/views/CalendarView.vue` | 月视图日期格子已有完成数/总数标签 + 任务点，但**无负载色阶** |
| `src/views/GoalTreeView.vue` | `handleSmartSplit` (L568-619) 直接拆解，**无负载检查** |
| 热力图 `HeatmapCell` | 已有 `plan_qty`/`task_count` 字段，但不分组到 goal，无法拆解跨目标分布 |

## 设计方案

### Step 1: 后端新增 `get_daily_load` 命令

**`src-tauri/src/db/models.rs`** — 新增两个结构体：

```rust
/// 每日负载（按目标分组）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyLoad {
    pub date: String,
    pub total_tasks: i64,
    pub total_qty: f64,
    pub by_goal: Vec<GoalLoad>,
}

/// 单目标在某日的负载
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalLoad {
    pub goal_id: String,
    pub goal_name: String,
    pub task_count: i64,
    pub total_qty: f64,
}
```

**`src-tauri/src/commands/stats.rs`** — 新增命令：

```rust
#[tauri::command]
pub async fn get_daily_load(
    start_date: String,
    end_date: String,
    state: State<'_, DbPool>,
) -> AppResult<Vec<DailyLoad>> {
    // SQL：JOIN goals 拿名称，按 plan_date + goal_id 分组
    let rows: Vec<(String, String, String, i64, f64)> = sqlx::query_as(
        "SELECT t.plan_date, t.goal_id, g.name,
                COUNT(*) as task_count,
                CAST(COALESCE(SUM(t.plan_qty), 0) AS REAL) as total_qty
         FROM tasks t
         INNER JOIN goals g ON t.goal_id = g.id
         WHERE t.plan_date IS NOT NULL
           AND t.plan_date >= ? AND t.plan_date <= ?
           AND t.status != 'skipped'
         GROUP BY t.plan_date, t.goal_id, g.name
         ORDER BY t.plan_date",
    )
    .bind(&start_date)
    .bind(&end_date)
    .fetch_all(&state.0)
    .await?;

    // 应用层聚合：按日期分组
    use std::collections::BTreeMap;
    let mut daily_map: BTreeMap<String, (i64, f64, Vec<GoalLoad>)> = BTreeMap::new();
    for (date, goal_id, goal_name, task_count, total_qty) in rows {
        let entry = daily_map.entry(date).or_insert((0, 0.0, Vec::new()));
        entry.0 += task_count;
        entry.1 += total_qty;
        entry.2.push(GoalLoad { goal_id, goal_name, task_count, total_qty });
    }

    // 填充日期范围内每一天（含无任务日）
    let start = chrono::NaiveDate::parse_from_str(&start_date, "%Y-%m-%d")
        .map_err(|e| crate::error::AppError::Param(format!("起始日期格式错误: {}", e)))?;
    let end = chrono::NaiveDate::parse_from_str(&end_date, "%Y-%m-%d")
        .map_err(|e| crate::error::AppError::Param(format!("结束日期格式错误: {}", e)))?;

    let mut result = Vec::new();
    let mut cursor = start;
    while cursor <= end {
        let date_str = cursor.format("%Y-%m-%d").to_string();
        let (total_tasks, total_qty, by_goal) =
            daily_map.remove(&date_str).unwrap_or((0, 0.0, Vec::new()));
        result.push(DailyLoad { date: date_str, total_tasks, total_qty, by_goal });
        cursor += chrono::Duration::days(1);
    }
    Ok(result)
}
```

**`src-tauri/src/lib.rs`** — 注册命令：
```rust
commands::stats::get_daily_load,
```

### Step 2: 前端 API + 类型

**`src/types/index.ts`** — 新增类型：
```ts
export interface GoalLoad {
  goal_id: string;
  goal_name: string;
  task_count: number;
  total_qty: number;
}

export interface DailyLoad {
  date: string;
  total_tasks: number;
  total_qty: number;
  by_goal: GoalLoad[];
}
```

**`src/api/stats.ts`** — 新增函数：
```ts
import type { ..., DailyLoad } from "@/types";

/** 获取日期范围内每日负载（按目标分组） */
export async function getDailyLoad(startDate: string, endDate: string): Promise<DailyLoad[]> {
  return invoke("get_daily_load", { startDate, endDate });
}
```

### Step 3: 日历色阶

**`src/views/CalendarView.vue`** — 改动：

1. **导入与状态**：
```ts
import * as statsApi from "@/api/stats";
import type { DailyLoad } from "@/types";

const dailyLoadMap = ref<Record<string, DailyLoad>>({});
const LOAD_THRESHOLD_MEDIUM = 4; // ≥4 中等
const LOAD_THRESHOLD_HIGH = 7;   // ≥7 过载
```

2. **`loadData()` 内追加负载查询**（在现有 `taskApi.listTasksByDateRange` 之后）：
```ts
const { start, end } = getDateRange();
const startStr = format(start, "yyyy-MM-dd");
const endStr = format(end, "yyyy-MM-dd");
const [taskList, loadList] = await Promise.all([
  taskApi.listTasksByDateRange(startStr, endStr),
  statsApi.getDailyLoad(startStr, endStr),
]);
tasks.value = taskList;
dailyLoadMap.value = Object.fromEntries(loadList.map(l => [l.date, l]));
```

3. **新增 `getLoadLevel(day)` 函数**：
```ts
function getLoadLevel(day: Date): "none" | "low" | "medium" | "high" {
  const key = format(day, "yyyy-MM-dd");
  const load = dailyLoadMap.value[key];
  if (!load || load.total_tasks === 0) return "none";
  if (load.total_tasks >= LOAD_THRESHOLD_HIGH) return "high";
  if (load.total_tasks >= LOAD_THRESHOLD_MEDIUM) return "medium";
  return "low";
}
```

4. **月视图日期格子加色阶 class**（在 L831 的 `:class` 对象中追加）：
```ts
// 当日非本月、非今天、非聚焦时，根据负载等级加背景色
'bg-green-50': !isSameMonth(day, currentDate) && !isToday(day) && getLoadLevel(day) === 'low',
'bg-yellow-50': !isSameMonth(day, currentDate) && !isToday(day) && getLoadLevel(day) === 'medium',
'bg-red-50': !isSameMonth(day, currentDate) && !isToday(day) && getLoadLevel(day) === 'high',
```

> 注意：色阶仅在「非今天、非聚焦」时生效，避免与 today/聚焦 样式冲突。今天/聚焦格子的负载通过底部的小色点显示。

5. **日期格子内新增负载指示条**（在任务点之前，L843 `<div v-if="getDayStatsCached(day).total > 0">` 之前）：
```vue
<div
  v-if="getLoadLevel(day) !== 'none'"
  class="w-full h-1 rounded-full mb-0.5"
  :class="{
    'bg-green-300': getLoadLevel(day) === 'low',
    'bg-yellow-300': getLoadLevel(day) === 'medium',
    'bg-red-300': getLoadLevel(day) === 'high',
  }"
  :title="`当日负载: ${dailyLoadMap[format(day, 'yyyy-MM-dd')]?.total_tasks || 0} 个任务`"
/>
```

6. **NPopover 悬浮内容追加负载信息**（在任务列表前）：
```vue
<div class="text-xs text-gray-500 mb-1 flex items-center gap-2">
  <Icon icon="mdi:speedometer" width="12" />
  负载：{{ getLoadOfDay(day) }}
</div>
```

新增辅助函数 `getLoadOfDay(day)` 返回字符串如 `"5 个任务（目标A×2, 目标B×3）"`。

### Step 4: 拆解时过载警告

**`src/views/GoalTreeView.vue`** — 改动 `handleSmartSplit` (L568-619)：

1. **导入**：
```ts
import * as statsApi from "@/api/stats";
```

2. **在构造 `input` 后、调用 `goalStore.smartSplit` 前，计算拆解日期范围并查询负载**：
```ts
async function handleSmartSplit() {
  // ... 现有的入参校验和 input 构造（L569-608）...

  // P2-5：拆解前检查目标日期范围内的负载
  const todayStr = format(new Date(), "yyyy-MM-dd");
  let rangeStart = todayStr;
  let rangeEnd = todayStr;
  if (splitForm.strategy === "by_deadline") {
    rangeEnd = input.deadline!;
  } else if (splitForm.strategy === "by_capacity") {
    rangeEnd = input.deadline!;
  } else if (splitForm.strategy === "by_date_range") {
    rangeStart = input.start_date!;
    rangeEnd = input.end_date!;
  }

  try {
    const loads = await statsApi.getDailyLoad(rangeStart, rangeEnd);
    const LOAD_THRESHOLD = 5;
    const overloaded = loads.filter(l => l.total_tasks >= LOAD_THRESHOLD);

    if (overloaded.length > 0) {
      const detail = overloaded
        .slice(0, 5)
        .map(l => `${l.date}：${l.total_tasks} 个任务`)
        .join("\n");
      const more = overloaded.length > 5 ? `\n... 等共 ${overloaded.length} 天` : "";

      const confirmed = await new Promise<boolean>((resolve) => {
        dialog.warning({
          title: "负载警告",
          content: `以下日期任务较多：\n${detail}${more}\n\n继续拆解可能导致这些日期任务过载。是否继续？`,
          positiveText: "继续拆解",
          negativeText: "取消",
          onPositiveClick: () => resolve(true),
          onNegativeClick: () => resolve(false),
          onClose: () => resolve(false),
        });
      });
      if (!confirmed) return;
    }

    // 执行原有拆解逻辑
    const tasks = await goalStore.smartSplit(input);
    message.success(`已拆解为 ${tasks.length} 个每日任务`);
    showSplitModal.value = false;
    await goalStore.fetchGoalTree();
    expandedNodes.value.add(splitForm.goal_id);
  } catch (e) {
    message.error(String(e));
  }
}
```

## 涉及文件

| 层 | 文件 | 改动 |
|---|------|------|
| 后端 | `src-tauri/src/db/models.rs` | 新增 `DailyLoad`、`GoalLoad` 结构体 |
| 后端 | `src-tauri/src/commands/stats.rs` | 新增 `get_daily_load` 命令 |
| 后端 | `src-tauri/src/lib.rs` | 注册 `get_daily_load` 命令 |
| 前端 | `src/types/index.ts` | 新增 `DailyLoad`、`GoalLoad` 类型 |
| 前端 | `src/api/stats.ts` | 新增 `getDailyLoad` 函数 |
| 前端 | `src/views/CalendarView.vue` | 色阶显示 + 负载查询 + 悬浮提示 |
| 前端 | `src/views/GoalTreeView.vue` | `handleSmartSplit` 增加过载检查 |

## 验证步骤

1. **构建验证**：
   - `cargo check` — Rust 编译通过
   - `npx vue-tsc --noEmit` — 前端类型检查通过

2. **日历色阶验证**（启动应用后）：
   - 进入「日历」→ 月视图
   - 当天有 1-3 个任务 → 绿色背景/指示条
   - 当天有 4-6 个任务 → 黄色背景/指示条
   - 当天有 ≥7 个任务 → 红色背景/指示条
   - 悬浮日期格子 → 弹层显示「负载：X 个任务（目标A×N, 目标B×M）」

3. **拆解时过载警告验证**：
   - 准备一个已有 5+ 任务的目标，对另一个目标在该日期范围内拆解
   - 点击「确认拆解」→ 弹出警告「负载警告：以下日期任务较多...」
   - 点击「继续拆解」→ 执行拆解
   - 点击「取消」→ 不拆解

4. **无过载场景**：
   - 在任务较少的日期范围内拆解 → 不弹出警告，直接拆解

## 阈值设计

- `LOAD_THRESHOLD_MEDIUM = 4`（中等负载）
- `LOAD_THRESHOLD_HIGH = 7`（过载）
- 拆解警告阈值 `LOAD_THRESHOLD = 5`

> 这三个阈值是合理的初始默认值。后续可考虑放到设置中让用户调整，但本任务不