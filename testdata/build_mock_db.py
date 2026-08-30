# -*- coding: utf-8 -*-
"""构建并验证 SelfPilot 模拟数据库。

产出（testdata/ 下，均为「可直接导入应用」的 drop-in 库，已含合法 _sqlx_migrations）：
  mock_selfpilot.db                首任务弹干净态（默认：完成今日首任务后远离里程碑）
  mock_selfpilot_approaching.db    里程碑「接近历史」态（current=5 vs longest=6）
  mock_selfpilot_milestone.db      里程碑「追平历史」态（current=6 = longest=6）
  mock_selfpilot_celebration.db    全部完成庆祝态（仅剩 c-1 未完成，完成它全部 100%）

「可直接导入」说明：应用启动时 sqlx 会用 _sqlx_migrations 校验已应用迁移；
若模拟库缺该表，sqlx 会重跑 001~017 迁移，因 schema 已是最新形态而报
「duplicate column name: level」。本脚本按 sqlx 0.8 算法（迁移文件原始字节的
完整 SHA-384）写入 1~17 全部成功记录，使模拟库可经应用内「设置-备份/恢复」
直接导入，或直接覆盖 %APPDATA%\\com.selfpilot.desktop\\selfpilot.db 使用。

验证方式：在临时副本上模拟「完成今日第 1 个任务」（把今日 sort_order 最小的
pending 任务置 done），随后复算 streak_service / stats 逻辑：
  - current_streak / longest_streak / check_longest_streak_milestone 判定
  - 顶层目标完成度百分比（stats 口径）
  - 今日剩余任务 / 依赖阻塞

注：生产 streak_service 会做 longest=max(longest,current) 合并，「超越历史」
    分支实际不可达，故不提供 breakthrough 变体（补完长链断点只会得到「追平历史」）。
"""
import glob
import hashlib
import os
import shutil
import sqlite3
from datetime import date, timedelta

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
SQL_PATH = os.path.join(BASE_DIR, "mock_selfpilot.sql")
MIGRATIONS_DIR = os.path.normpath(os.path.join(BASE_DIR, "..", "src-tauri", "migrations"))

# ---------- 变体：接近历史（a2-5 -> done 补断点，b2-1 -> pending 留断点，current=5 vs 6） ----------
APPROACHING = """
UPDATE tasks SET status='done', actual_qty=plan_qty WHERE id='mock-task-a2-5';
UPDATE tasks SET status='pending', actual_qty=0 WHERE id='mock-task-b2-1';
"""

# ---------- 变体：追平历史（a2-5 -> done 补断点，current=6 = longest=6） ----------
TIE = """
UPDATE tasks SET status='done', actual_qty=plan_qty WHERE id='mock-task-a2-5';
"""

# ---------- 变体：全部完成庆祝（仅剩 c-1 今日待办，完成它触发庆祝） ----------
CELEBRATION = """
UPDATE tasks SET status='done', actual_qty=plan_qty WHERE status IN ('pending','partial');
UPDATE tasks SET status='done', actual_qty=plan_qty
    WHERE id IN ('mock-task-a-direct-1','mock-task-b-direct-1');
UPDATE tasks SET status='pending', actual_qty=0 WHERE id='mock-task-c-1';
"""


def generate_migrations_checksums():
    """按 sqlx 0.8 算法计算 001~017 迁移的 SHA-384 与 description。

    sqlx 读取迁移文件用的是 Rust fs::read_to_string（UTF-8 解码并剥离 BOM），
    再对解码后字符串重新编码为 UTF-8 的字节做 Sha384 摘要（sqlx-core/src/migrate/migration.rs）。
    当前迁移文件均无 BOM，故直接对原始字节计算即等价；用 utf-8-sig 解码更稳妥。
    description 来自文件名：去掉 `<version>_` 前缀与 `.sql` 后缀，`_` 替换为空格。
    """
    checksums = {}
    descriptions = {}
    for f in sorted(glob.glob(os.path.join(MIGRATIONS_DIR, "*.sql"))):
        ver = int(os.path.basename(f).split("_")[0])
        with open(f, "rb") as fh:
            text = fh.read().decode("utf-8-sig")
        checksums[ver] = hashlib.sha384(text.encode("utf-8")).digest()
        desc = os.path.basename(f)[len(str(ver)) + 1:]
        descriptions[ver] = desc[: -len(".sql")].replace("_", " ")
    return checksums, descriptions


def init_sqlx_migrations(con):
    """写入 sqlx 0.8 的 _sqlx_migrations 表（1~17 全部 success=1 记录）。

    若模拟库缺该表，应用启动时 sqlx 会认为从未迁移过而重跑 001~017，
    因 schema 已是最新形态而报「duplicate column name: level」等错误。
    表结构 / checksum 算法与 sqlx-sqlite-0.8.6 的 ensure_migrations_table /
    list_applied_migrations 完全对齐。
    """
    checksums, descriptions = generate_migrations_checksums()
    # 先删除可能残留的旧表，保证幂等重建（该表完全由本脚本管理）
    con.execute("DROP TABLE IF EXISTS _sqlx_migrations")
    con.execute(
        """
        CREATE TABLE _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            success BOOLEAN NOT NULL,
            checksum BLOB NOT NULL,
            execution_time BIGINT NOT NULL
        )
        """
    )
    con.executemany(
        "INSERT INTO _sqlx_migrations "
        "(version, description, success, checksum, execution_time) VALUES (?, ?, ?, ?, ?)",
        [(v, descriptions[v], 1, checksums[v], 0) for v in sorted(checksums)],
    )


def build(path, extra_sql=None):
    con = sqlite3.connect(path)
    con.execute("PRAGMA foreign_keys = ON")
    with open(SQL_PATH, encoding="utf-8") as f:
        con.executescript(f.read())
    init_sqlx_migrations(con)
    if extra_sql:
        con.executescript(extra_sql)
    con.commit()
    con.close()
    print(f"built: {os.path.basename(path)}")


def complete_first_task(con, today):
    """模拟用户完成今日第 1 个任务（取今日 sort_order 最小的 pending 任务置 done）。"""
    row = con.execute(
        "SELECT id, plan_qty FROM tasks "
        "WHERE plan_date=? AND status='pending' ORDER BY sort_order LIMIT 1",
        (today.isoformat(),),
    ).fetchone()
    if row is None:
        return None
    con.execute("UPDATE tasks SET status='done', actual_qty=? WHERE id=?", (row[1], row[0]))
    con.commit()
    return row[0]


# ======================= 复算 streak（复刻 streak_service.rs） =======================
def calc_streak(con, today):
    """复刻 streak_service.rs 的 load_day_map + calc_current_streak + calc_longest_streak"""
    rows = con.execute(
        "SELECT plan_date, COUNT(*), "
        "SUM(CASE WHEN status='done' THEN 1 ELSE 0 END) "
        "FROM tasks WHERE plan_date IS NOT NULL AND status != 'skipped' "
        "GROUP BY plan_date"
    ).fetchall()
    day_map = {}
    for ds, cnt, done in rows:
        try:
            d = date.fromisoformat(ds)
        except ValueError:
            continue
        day_map[d] = (cnt > 0, done > 0)

    # current_streak
    cur = 0
    cursor = today - timedelta(days=1)
    te = day_map.get(today)
    if te and te[0] and not te[1]:
        cur = 0
    elif te and te[0] and te[1]:
        cur = 1
    if not (te and te[0] and not te[1]):
        while True:
            e = day_map.get(cursor)
            if e is None:
                cursor -= timedelta(days=1)
            elif e[0] and e[1]:
                cur += 1
                cursor -= timedelta(days=1)
            elif e[0] and not e[1]:
                break
            else:
                cursor -= timedelta(days=1)
            if (today - cursor).days > 3650:
                break

    # longest_streak
    longest = 0
    temp = 0
    last = None
    for d in sorted(day_map):
        ht, comp = day_map[d]
        if not ht:
            continue
        if comp:
            ok = True
            if last is not None:
                chk = last + timedelta(days=1)
                while chk < d:
                    if chk in day_map and day_map[chk][0] and not day_map[chk][1]:
                        ok = False
                        break
                    chk += timedelta(days=1)
            temp = temp + 1 if ok else 1
            last = d
            longest = max(longest, temp)
        else:
            temp = 0
            last = d
    # 生产合并：longest = max(longest, current)
    if cur > longest:
        longest = cur
    return cur, longest


def milestone_type(cur, longest):
    """复刻 commands/encouragement.rs 的 check_longest_streak_milestone。"""
    if longest > 0 and cur >= longest:
        return "超越历史" if cur > longest else "追平历史"
    if longest > 0 and cur >= longest - 2:
        return "接近历史"
    return "无里程碑"


# ======================= 验证辅助查询 =======================
def goal_pct(con):
    """顶层目标完成度（stats 口径：直属任务 actual/plan 累计）。"""
    return con.execute(
        "SELECT g.name, "
        "CASE WHEN COALESCE(SUM(CASE WHEN t.status != 'skipped' THEN t.plan_qty ELSE 0 END),0)=0 THEN 0.0 "
        "ELSE MIN(1.0, COALESCE(SUM(t.actual_qty),0)/COALESCE(SUM(CASE WHEN t.status != 'skipped' THEN t.plan_qty ELSE 0 END),0)) END "
        "FROM goals g LEFT JOIN tasks t ON t.goal_id=g.id "
        "WHERE g.parent_id IS NULL GROUP BY g.id,g.name ORDER BY g.created_at"
    ).fetchall()


def today_tasks(con, today):
    return con.execute(
        "SELECT t.name, t.status FROM tasks t "
        "WHERE t.plan_date=? AND t.status!='skipped' ORDER BY t.sort_order",
        (today.isoformat(),),
    ).fetchall()


def blocked_deps(con):
    return con.execute(
        "SELECT t.name, d.depends_on_id FROM task_dependencies d "
        "JOIN tasks t ON t.id=d.task_id JOIN tasks p ON p.id=d.depends_on_id "
        "WHERE p.status!='done'"
    ).fetchall()


# ======================= 验证入口 =======================
def verify(path, label, expect):
    """在临时副本上模拟完成今日首任务，再复算并断言 (cur, longest, mtype)。"""
    tmp = path + ".verify.tmp.db"
    shutil.copyfile(path, tmp)
    con = sqlite3.connect(tmp)
    today = date.today()

    done_id = complete_first_task(con, today)
    cur, longest = calc_streak(con, today)
    mtype = milestone_type(cur, longest)
    pct = goal_pct(con)
    today_left = today_tasks(con, today)
    blocked = blocked_deps(con)

    print(f"\n===== {label} =====")
    print(f"完成今日首任务: {done_id}")
    print(f"current_streak={cur}  longest_streak={longest}  里程碑判定 => {mtype}")
    print(f"今日剩余任务({len(today_left)}): {[f'{n}:{s}' for n, s in today_left]}")
    print(f"顶层目标完成度: {[f'{n}={v:.0%}' for n, v in pct]}")
    print(f"依赖阻塞中: {[f'{n}(依赖 {d})' for n, d in blocked]}")
    ok = (cur, longest, mtype) == expect
    print("验证:", "PASS" if ok else f"FAIL (期望 {expect})")
    con.close()
    os.remove(tmp)
    return ok


def verify_celebration(path):
    """庆祝态：完成 c-1 前未全部完成 → 完成后顶层目标全部 100%。"""
    tmp = path + ".verify.tmp.db"
    shutil.copyfile(path, tmp)
    con = sqlite3.connect(tmp)
    today = date.today()

    before = goal_pct(con)
    all_before = all(v >= 0.999 for _, v in before)
    done_id = complete_first_task(con, today)
    after = goal_pct(con)
    all_after = all(v >= 0.999 for _, v in after)
    cur, longest = calc_streak(con, today)

    print(f"\n===== 全部完成庆祝态 =====")
    print(f"完成前 顶层目标完成度: {[f'{n}={v:.0%}' for n, v in before]}  全部完成={all_before}")
    print(f"完成  今日首任务: {done_id}")
    print(f"完成后 顶层目标完成度: {[f'{n}={v:.0%}' for n, v in after]}  全部完成={all_after}")
    print(f"current_streak={cur}  longest_streak={longest}  里程碑判定 => {milestone_type(cur, longest)}")
    ok = (not all_before) and all_after and done_id == 'mock-task-c-1'
    print("验证:", "PASS" if ok else "FAIL (期望: 完成 c-1 前未全部完成 → 完成后全部 100%)")
    con.close()
    os.remove(tmp)
    return ok


if __name__ == "__main__":
    # 清理可能残留的临时副本
    for p in os.listdir(BASE_DIR):
        if p.endswith(".verify.tmp.db"):
            os.remove(os.path.join(BASE_DIR, p))

    base = os.path.join(BASE_DIR, "mock_selfpilot.db")
    build(base)
    build(os.path.join(BASE_DIR, "mock_selfpilot_approaching.db"), APPROACHING)
    build(os.path.join(BASE_DIR, "mock_selfpilot_milestone.db"), TIE)
    build(os.path.join(BASE_DIR, "mock_selfpilot_celebration.db"), CELEBRATION)

    print("\n############ 完成今日第 1 个任务后 ############")
    ok = True
    ok &= verify(base, "首任务弹干净态（远离里程碑 → 弹首任务鼓励）", (3, 6, "无里程碑"))
    ok &= verify(
        os.path.join(BASE_DIR, "mock_selfpilot_approaching.db"),
        "接近历史态（距历史记录 1 天 → 弹🚀里程碑）",
        (5, 6, "接近历史"),
    )
    ok &= verify(
        os.path.join(BASE_DIR, "mock_selfpilot_milestone.db"),
        "追平历史态（current=6=longest → 弹🚀里程碑）",
        (6, 6, "追平历史"),
    )
    ok &= verify_celebration(os.path.join(BASE_DIR, "mock_selfpilot_celebration.db"))

    print("\n全部验证:", "PASS" if ok else "存在 FAIL")
    raise SystemExit(0 if ok else 1)
