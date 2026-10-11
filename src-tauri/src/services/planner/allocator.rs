//! 目标树智能规划引擎（v2）——A3 自适应负载分配器 + A3.1 迟滞守恒补偿
//!
//! 实现重设计方案 §3.4（A3 加权公平配额：最大余数法 / 硬上限裁剪再分配 /
//! 退化兜底）与整合方案 §4.7.5（A3.1 `apply_hysteresis`：迟滞抑制量的
//! 守恒回流，修复 INV-1 守恒缺口）。
//!
//! 与伪代码的对应关系：
//! - 行 1~4：过滤可用日、Σweight ≤ 0 时权重置 1 的退化兜底（W-ALL-ZERO）；
//! - 行 5~10：量化为整数份数后按最大余数法分配（余数按小数部分降序、
//!   平局按日期升序——本实现对日期升序的索引做稳定排序天然满足）；
//! - 行 11~17：`max_daily_qty` 折算为单日份数上限，超限日裁剪并把溢出
//!   份数按权重比例再分配到未饱和日，循环至多 [`MAX_ALLOC_ITER`] 次；
//! - 行 18：组装 [`DayQuota`]，量化舍入误差归入最后一个有效日，保证
//!   `Σ qty == total_qty`（整数域份数守恒 + 浮点域显式校验闭环）。
//!
//! 本模块为纯函数：不访问数据库、不读时钟。

use std::cmp::Ordering;

use chrono::NaiveDate;
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::services::planner::snapshot::DaySlot;
use crate::services::planner::types::SmoothingPolicy;

/// 单日配额（A3 输出）
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DayQuota {
    /// 日期
    pub date: NaiveDate,
    /// 当日可排计划量（全部配额之和与 total_qty 严格守恒）
    pub qty: f64,
}

/// 硬上限裁剪再分配的迭代上限（伪代码 MAX_ITER，对应 W-ITERATION-CAPPED）
pub const MAX_ALLOC_ITER: usize = 64;

/// 守恒校验容差（INV-1：误差 < 1e-6）
const CONSERVATION_EPS: f64 = 1e-6;

/// 迟滞相对变化的零保护分母
const HYST_EPS: f64 = 1e-9;

/// 按总量选择量化粒度（份数步长）
///
/// 整数量纲（页/词/题）以 1 为粒度；小数量纲（小时/公里）以 0.1 为粒度。
/// 舍入误差统一归入最后一个有效日（见 [`allocate_weights`] 尾部修正）。
fn quantization_step(total: f64) -> f64 {
    if (total - total.round()).abs() < 1e-9 {
        1.0
    } else {
        0.1
    }
}

/// 最大余数法（Hare quota）：把 `total` 份按权重分配，返回各日整数份数
///
/// 余数份数给小数部分最大的日，平局按索引升序（即日期升序，因为调用方
/// 保证传入序列按日期升序）——与伪代码行 9 `sort avail by (f desc, date asc)`
/// 一致。数学性质：`0 ≤ R < n`，故总能选出 R 个不同的日各 +1，精确守恒。
fn largest_remainder(weights: &[f64], total_shares: i64) -> Vec<i64> {
    let n = weights.len();
    if n == 0 {
        return Vec::new();
    }
    if total_shares <= 0 {
        return vec![0; n];
    }
    let wsum: f64 = weights.iter().sum();
    if wsum <= 0.0 {
        // 全零权重：均分兜底（调用方已在退化兜底中置 1，此为防御分支）
        let base = total_shares / n as i64;
        let mut a = vec![base; n];
        let mut r = total_shares - base * n as i64;
        for v in a.iter_mut() {
            if r <= 0 {
                break;
            }
            *v += 1;
            r -= 1;
        }
        return a;
    }
    let mut shares = vec![0i64; n];
    let mut fracs = vec![0.0f64; n];
    for i in 0..n {
        let q = total_shares as f64 * weights[i] / wsum;
        // floor 前对负零/极小值做保护，避免 -0.0 转换越界
        let floor = if q > 0.0 { q.floor() } else { 0.0 };
        shares[i] = floor as i64;
        fracs[i] = q - floor;
    }
    let mut remainder = total_shares - shares.iter().sum::<i64>();
    // 索引序即日期升序：stable sort 保证小数部分相同时更早的日优先
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&x, &y| {
        fracs[y]
            .partial_cmp(&fracs[x])
            .unwrap_or(Ordering::Equal)
    });
    let mut k = 0;
    while remainder > 0 {
        // 正常情况下 remainder < n，一轮即可；浮点边界下防御性地循环取模
        let idx = order[k % n];
        shares[idx] += 1;
        remainder -= 1;
        k += 1;
        if k > total_shares as usize {
            break; // 防御：避免理论不可达的死循环
        }
    }
    shares
}

/// A3 加权公平分配（伪代码 §3.4.1 + 裁剪再分配循环）
///
/// - `days`：编译后的日槽序列（内部过滤 `available && !is_rest`）；
/// - `total_qty`：待分配总量 W（≥ 0 且有限）；
/// - `max_daily_qty`：单日计划量上限（None = 不限），折算为份数上限参与裁剪。
///
/// 平滑与迟滞由 [`apply_hysteresis`] 独立执行（伪代码 §3.4.2 的分层），
/// 本函数不消费 SmoothingPolicy。
///
/// 守恒保证：返回值 `Σ qty == total_qty`（误差 < 1e-6），校验失败返回
/// `Err` 而非 panic。全饱和无法守恒时保守恒（尾部挂账），由调用方经
/// A2/A4 告警暴露。
pub fn allocate_weights(
    days: &[DaySlot],
    total_qty: f64,
    max_daily_qty: Option<f64>,
) -> AppResult<Vec<DayQuota>> {
    if !total_qty.is_finite() || total_qty < 0.0 {
        return Err(AppError::Param("待分配总量必须为非负有限数".into()));
    }

    // 行 1：可用日（available 且非休息日）
    let avail: Vec<&DaySlot> = days
        .iter()
        .filter(|d| d.available && !d.is_rest)
        .collect();
    if avail.is_empty() {
        return Err(AppError::Business("视界内没有可用于排期的日期".into()));
    }
    let n = avail.len();

    // 行 3~4：权重与退化兜底（Wsum <= 0 时全部置 1，W-ALL-ZERO）
    let mut weights: Vec<f64> = avail.iter().map(|d| d.weight.max(0.0)).collect();
    let wsum: f64 = weights.iter().sum();
    if wsum <= 0.0 {
        for w in weights.iter_mut() {
            *w = 1.0;
        }
    }

    // 量化：总量 → 整数份数（整数域守恒的单位）
    let step = quantization_step(total_qty);
    let total_shares = (total_qty / step).round() as i64;
    let cap_per_day: Option<i64> = max_daily_qty.map(|m| {
        if m.is_finite() && m > 0.0 {
            ((m / step).floor() as i64).max(0)
        } else {
            0
        }
    });

    // 行 5~10：最大余数法基础分配
    let mut shares = largest_remainder(&weights, total_shares);

    // 行 11~17：硬上限裁剪与再分配（迭代直到收敛或 MAX_ITER）
    if let Some(cap) = cap_per_day {
        let mut iter = 0;
        loop {
            if iter >= MAX_ALLOC_ITER {
                // W-ITERATION-CAPPED：达到迭代上限，差额挂最后一天保守恒
                let diff = total_shares - shares.iter().sum::<i64>();
                if diff != 0 {
                    let last = n - 1;
                    shares[last] += diff;
                }
                break;
            }
            iter += 1;
            // 步骤 13~16：裁剪溢出日，收集盈余
            let mut surplus: i64 = 0;
            let mut has_over = false;
            for s in shares.iter_mut() {
                if *s > cap {
                    surplus += *s - cap;
                    *s = cap;
                    has_over = true;
                }
            }
            if !has_over || surplus <= 0 {
                break; // 收敛
            }
            // 步骤 17：盈余再分配到未饱和日（按权重比例，重复最大余数法）
            let unsat: Vec<usize> = (0..n).filter(|&i| shares[i] < cap).collect();
            if unsat.is_empty() {
                // 全部饱和：尾部溢出挂账（保守恒，A2 应已拦截该场景）
                let last = n - 1;
                shares[last] += surplus;
                break;
            }
            let unsat_weights: Vec<f64> = unsat.iter().map(|&i| weights[i]).collect();
            let add = largest_remainder(&unsat_weights, surplus);
            let mut placed: i64 = 0;
            for (k, &i) in unsat.iter().enumerate() {
                let room = cap - shares[i];
                let give = add[k].min(room);
                shares[i] += give;
                placed += give;
            }
            if placed >= surplus {
                break; // 完全安置
            }
            // 未安置部分下一轮继续（deficit 由裁剪检查重算，防死循环：
            // unsat 非空时每轮至少安置 1 份，或提前 break）
        }
        // 迭代后兜底：极小概率仍不守恒时挂最后一天
        let diff = total_shares - shares.iter().sum::<i64>();
        if diff != 0 {
            let last = n - 1;
            shares[last] += diff;
        }
    }

    // 行 18：组装 DayQuota（份数 → 数量）
    let mut quotas: Vec<DayQuota> = avail
        .iter()
        .enumerate()
        .map(|(i, d)| DayQuota {
            date: d.date,
            qty: shares[i] as f64 * step,
        })
        .collect();

    // 量化舍入误差归入最后一个有效日（任务约定：误差归入最后一天或余数法）
    let current_sum: f64 = quotas.iter().map(|q| q.qty).sum();
    let residual = total_qty - current_sum;
    if residual.abs() > 1e-12 {
        let target = quotas
            .iter()
            .rposition(|q| q.qty > 0.0)
            .unwrap_or(quotas.len() - 1);
        quotas[target].qty += residual;
    }

    // 守恒显式校验（debug_assert + Err，禁止 panic）
    let final_sum: f64 = quotas.iter().map(|q| q.qty).sum();
    debug_assert!(
        (final_sum - total_qty).abs() < CONSERVATION_EPS,
        "A3 分配守恒被破坏：Σqty={}，W={}",
        final_sum,
        total_qty
    );
    if (final_sum - total_qty).abs() >= CONSERVATION_EPS {
        return Err(AppError::Internal(format!(
            "每日配额守恒校验失败：Σqty({}) != W({})",
            final_sum, total_qty
        )));
    }

    Ok(quotas)
}

/// A3.1 迟滞守恒补偿（整合方案 §4.7.5，修复守恒缺口）
///
/// 对每个配额日：若旧方案同日数量存在且相对变化小于
/// `policy.qty_change_threshold`（默认 0.15），则保留旧值（迟滞命中），
/// 被抑制的带符号差额累计为 `suppressed`；随后把 `suppressed` 按当日
/// 数量比例回流到未被抑制的日——正负差额统一带符号处理，误差归入最后
/// 一个候选日，保证 `Σ qty` 与输入严格一致（守恒优先于平滑）。
///
/// 全部日期被迟滞抑制时放弃平滑、恢复原配额（伪代码
/// `revert_all_hysteresis`，对应 W-HYSTERESIS-ABORTED）。
///
/// 拆解场景 `old_placements` 传空切片即天然直通。
pub fn apply_hysteresis(
    quotas: Vec<DayQuota>,
    old_placements: &[DayQuota],
    policy: &SmoothingPolicy,
) -> Vec<DayQuota> {
    if old_placements.is_empty() || quotas.is_empty() {
        return quotas;
    }
    let before_sum: f64 = quotas.iter().map(|q| q.qty).sum();

    let mut result = quotas.clone();
    let mut old_map: std::collections::HashMap<NaiveDate, f64> =
        std::collections::HashMap::with_capacity(old_placements.len());
    for q in old_placements {
        old_map.insert(q.date, q.qty);
    }

    // 步骤 2~10：逐日迟滞判定，累计被抑制差额
    let mut suppressed: f64 = 0.0;
    let mut is_suppressed = vec![false; result.len()];
    for (i, q) in result.iter_mut().enumerate() {
        if let Some(&old_qty) = old_map.get(&q.date) {
            let denom = if old_qty.abs() > HYST_EPS {
                old_qty
            } else {
                HYST_EPS
            };
            if ((q.qty - old_qty) / denom).abs() < policy.qty_change_threshold {
                suppressed += q.qty - old_qty;
                q.qty = old_qty;
                is_suppressed[i] = true;
            }
        }
    }

    // 步骤 12~22：守恒补偿
    if suppressed.abs() > 1e-9 {
        let candidates: Vec<usize> = (0..result.len())
            .filter(|&i| !is_suppressed[i])
            .collect();
        if candidates.is_empty() {
            // 全部被抑制：放弃迟滞，恢复原配额（守恒优先于平滑）
            return quotas;
        }
        redistribute_signed(&mut result, &candidates, suppressed);
    }

    // 步骤 23：INV-1 断言（Σ 保持不变）
    let after_sum: f64 = result.iter().map(|q| q.qty).sum();
    debug_assert!(
        (after_sum - before_sum).abs() < CONSERVATION_EPS,
        "A3.1 迟滞补偿破坏守恒：前={}，后={}",
        before_sum,
        after_sum
    );
    result
}

/// 把带符号差额按候选日现有数量比例回流（正负统一处理）
///
/// 最后一日承接全部舍入误差，保证回流总额精确等于 `amount`；
/// 负方向减到 0 为止（缺口顺延由最后一日承接，总量守恒不变）。
fn redistribute_signed(quotas: &mut [DayQuota], candidates: &[usize], amount: f64) {
    if candidates.is_empty() || amount.abs() < 1e-12 {
        return;
    }
    let total_weight: f64 = candidates
        .iter()
        .map(|&i| quotas[i].qty.max(HYST_EPS))
        .sum();
    if total_weight <= 0.0 {
        // 候选日全为零：均分
        let each = amount / candidates.len() as f64;
        for &i in candidates {
            quotas[i].qty += each;
        }
        return;
    }
    let mut assigned = 0.0;
    for (k, &i) in candidates.iter().enumerate() {
        if k == candidates.len() - 1 {
            // 误差归入最后一个候选日
            quotas[i].qty += amount - assigned;
        } else {
            let share = amount * quotas[i].qty.max(HYST_EPS) / total_weight;
            let new_qty = (quotas[i].qty + share).max(0.0);
            assigned += new_qty - quotas[i].qty;
            quotas[i].qty = new_qty;
        }
    }
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn slot(date: NaiveDate, weight: f64) -> DaySlot {
        DaySlot {
            date,
            available: weight > 0.0,
            weight,
            is_rest: weight <= 0.0,
        }
    }

    fn day_slots(start: NaiveDate, weights: &[f64]) -> Vec<DaySlot> {
        weights
            .iter()
            .enumerate()
            .map(|(i, w)| slot(start + Duration::days(i as i64), *w))
            .collect()
    }

    fn policy() -> SmoothingPolicy {
        SmoothingPolicy::default()
    }

    /// 1. 守恒：随机权重下 Σ 分配量 == W（含裁剪循环触发场景）
    #[test]
    fn test_conservation_with_random_weights() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        // 权重经过 weekend_factor / phase 等路径可能为任意非负值
        let cases: Vec<(Vec<f64>, f64, Option<f64>)> = vec![
            (vec![1.0, 1.0, 1.0], 10.0, None),
            (vec![0.3, 2.7, 1.4, 0.9], 25.0, None),
            (vec![1.0, 1.0, 1.0, 1.0, 1.0], 47.0, Some(12.0)), // 触发裁剪循环
            (vec![2.0, 0.5, 3.3, 1.1, 0.7, 2.9], 100.0, Some(20.0)),
            (vec![1.0; 30], 300.5, None), // 小数量纲
            (vec![1.5, 1.5], 0.0, None),  // 零总量
        ];
        for (weights, total, cap) in cases {
            let days = day_slots(start, &weights);
            let quotas =
                allocate_weights(&days, total, cap).unwrap();
            let sum: f64 = quotas.iter().map(|q| q.qty).sum();
            assert!(
                (sum - total).abs() < 1e-6,
                "守恒失败：weights={:?} total={} sum={}",
                weights,
                total,
                sum
            );
            if let Some(c) = cap {
                // 裁剪收敛后除最后一天（挂账位）外均不超上限
                for (i, q) in quotas.iter().enumerate() {
                    if i + 1 < quotas.len() {
                        assert!(q.qty <= c + 1e-9, "日 {} 超上限: {}", q.date, q.qty);
                    }
                }
            }
        }
    }

    /// 2. 最大余数法：3 日权重下余数给小数部分最大且更早的日
    #[test]
    fn test_largest_remainder_tie_break() {
        // 100 份 / 3 日等权：q=33.33..，余数 1 份给最早的日
        let start = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        let days = day_slots(start, &[1.0, 1.0, 1.0]);
        let quotas = allocate_weights(&days, 100.0, None).unwrap();
        assert_eq!(quotas[0].qty, 34.0); // 最早日 +1
        assert_eq!(quotas[1].qty, 33.0);
        assert_eq!(quotas[2].qty, 33.0);

        // 权重 [1, 2, 1]：q = [20, 40, 20]，整除无余数
        let days = day_slots(start, &[1.0, 2.0, 1.0]);
        let quotas = allocate_weights(&days, 80.0, None).unwrap();
        assert_eq!((quotas[0].qty, quotas[1].qty, quotas[2].qty), (20.0, 40.0, 20.0));

        // 权重 [1, 3, 2]，W=100：q=[16.67, 50, 33.33]，floor=[16,50,33]，
        // R=1，小数部分 [0.67, 0, 0.33] → 给第 1 日
        let days = day_slots(start, &[1.0, 3.0, 2.0]);
        let quotas = allocate_weights(&days, 100.0, None).unwrap();
        assert_eq!((quotas[0].qty, quotas[1].qty, quotas[2].qty), (17.0, 50.0, 33.0));
    }

    /// 3. 退化兜底：全部权重 0 时权重置 1 仍守恒
    ///
    /// 场景：weekend_factor=0 等使可用日（available 且非休息日）权重全为 0
    /// ——日仍然 available，仅权重归零，触发伪代码行 4 的退化兜底。
    #[test]
    fn test_degenerate_all_zero_weights() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        let days: Vec<DaySlot> = (0..4)
            .map(|i| DaySlot {
                date: start + chrono::Duration::days(i as i64),
                available: true,
                weight: 0.0,
                is_rest: false,
            })
            .collect();
        let quotas = allocate_weights(&days, 40.0, None).unwrap();
        let sum: f64 = quotas.iter().map(|q| q.qty).sum();
        assert!((sum - 40.0).abs() < 1e-6);
        assert_eq!(quotas.len(), 4);
        // 均分：10/10/10/10
        for q in &quotas {
            assert_eq!(q.qty, 10.0);
        }
    }

    /// 无可用日时返回错误
    #[test]
    fn test_no_available_days() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        let days = day_slots(start, &[0.0]);
        let result = allocate_weights(&days, 10.0, None);
        assert!(result.is_err());
    }

    /// A3.1：迟滞命中保留旧值，守恒补偿后总量不变
    #[test]
    fn test_hysteresis_conservation() {
        let d1 = NaiveDate::from_ymd_opt(2026, 10, 11).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
        let d3 = NaiveDate::from_ymd_opt(2026, 10, 13).unwrap();
        // 新方案 [10, 10, 10]；旧方案 d1=9（变化 11% < 15% → 迟滞命中）
        let quotas = vec![
            DayQuota { date: d1, qty: 10.0 },
            DayQuota { date: d2, qty: 10.0 },
            DayQuota { date: d3, qty: 10.0 },
        ];
        let old = vec![
            DayQuota { date: d1, qty: 9.0 },
            DayQuota { date: d2, qty: 12.0 }, // 变化 16.7% ≥ 15% → 不抑制
            DayQuota { date: d3, qty: 9.0 },  // 变化 11% → 抑制
        ];
        let result = apply_hysteresis(quotas.clone(), &old, &policy());
        let sum: f64 = result.iter().map(|q| q.qty).sum();
        assert!((sum - 30.0).abs() < 1e-6, "迟滞后守恒破坏: {}", sum);
        // d1/d3 保留旧值
        assert_eq!(result[0].qty, 9.0);
        assert_eq!(result[2].qty, 9.0);
        // d2 承接补偿：30 - 9 - 9 = 12
        assert!((result[1].qty - 12.0).abs() < 1e-6);
    }

    /// A3.1：全部被抑制时放弃迟滞，恢复原配额
    #[test]
    fn test_hysteresis_all_suppressed_revert() {
        let d1 = NaiveDate::from_ymd_opt(2026, 10, 11).unwrap();
        let quotas = vec![DayQuota { date: d1, qty: 10.0 }];
        let old = vec![DayQuota { date: d1, qty: 9.0 }];
        let result = apply_hysteresis(quotas.clone(), &old, &policy());
        assert_eq!(result[0].qty, 10.0); // 恢复原配额
    }

    /// A3.1：空 old_placements（拆解场景）直通
    #[test]
    fn test_hysteresis_passthrough() {
        let d1 = NaiveDate::from_ymd_opt(2026, 10, 11).unwrap();
        let quotas = vec![DayQuota { date: d1, qty: 7.5 }];
        let result = apply_hysteresis(quotas.clone(), &[], &policy());
        assert_eq!(result[0].qty, 7.5);
    }
}
