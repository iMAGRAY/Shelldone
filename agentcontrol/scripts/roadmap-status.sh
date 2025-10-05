#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
LC_ALL=C.UTF-8

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib/common.sh"

MODE="${1:-full}"
case "$MODE" in
  full|compact|json)
    shift || true
    ;;
  *)
    MODE="full"
    ;;
esac

PROJECT_ROOT="$(cd "$SDK_ROOT/.." && pwd)"
STATUS_PATH="$SDK_ROOT/reports/status.json"

if [[ -z "${ROADMAP_SKIP_PROGRESS:-}" ]]; then
  "$SDK_ROOT/scripts/progress.py" || sdk::log "WRN" "progress завершился с предупреждением"
  printf '\n'
fi

if [[ ! -f "$STATUS_PATH" ]]; then
  sdk::log "INF" "reports/status.json отсутствует — выполняю progress.py"
  "$SDK_ROOT/scripts/progress.py" || sdk::die "progress завершился с ошибкой"
fi

python3 - "$MODE" "$STATUS_PATH" <<'PY'
import json
import sys
from datetime import date

mode = sys.argv[1]
status_path = sys.argv[2]
status = json.loads(open(status_path, encoding="utf-8").read())
roadmap = status.get("roadmap", {})
program = roadmap.get("program", {})
phase_progress = program.get("phase_progress", {})
if "computed_progress_pct" not in program and "progress_pct" in program:
    program["computed_progress_pct"] = program["progress_pct"]

def table(title, headers, rows):
    if not rows:
        return ""
    widths = [len(h) for h in headers]
    for row in rows:
        for idx, cell in enumerate(row):
            widths[idx] = max(widths[idx], len(cell))

    def border(char):
        return "+" + "+".join(char * (w + 2) for w in widths) + "+"

    def render_row(cells):
        return "|" + "|".join(f" {cell.ljust(widths[idx])} " for idx, cell in enumerate(cells)) + "|"

    lines = [title, border("-"), render_row(headers), border("=")]
    for row in rows:
        lines.append(render_row(row))
    lines.append(border("-"))
    return "\n".join(lines)

if mode == "json":
    output = dict(roadmap)
    output["generated_at"] = status.get("generated_at", date.today().isoformat())
    print(json.dumps(output, ensure_ascii=False))
    sys.exit(0)

program_row = [[
    program.get("name", "Program"),
    f"{program.get('progress_pct', 0)}%",
    program.get("health", "n/a"),
]]
program_table = table("Программа", ["Название", "Прогресс", "Состояние"], program_row)
phase_rows = [[phase, f"{value}%"] for phase, value in phase_progress.items()]
phase_table = table("Фазы", ["Фаза", "Прогресс"], phase_rows)

milestones = program.get("milestones", [])
milestone_rows = [[m.get("title", ""), m.get("due", "n/a"), m.get("status", "planned")] for m in milestones]
milestone_table = table("Вехи", ["Веха", "Срок", "Статус"], milestone_rows)

epic_rows = [
    [
        epic.get("id", ""),
        epic.get("title", ""),
        epic.get("status", ""),
        f"{epic.get('computed_progress_pct', epic.get('progress_pct', 0))}%",
        str(epic.get("size_points", 0)),
    ]
    for epic in roadmap.get("epics", [])
]
epic_table = table("Эпики", ["ID", "Название", "Статус", "Прогресс", "Размер"], epic_rows)

big_rows = [
    [
        task.get("id", ""),
        task.get("title", ""),
        task.get("status", ""),
        f"{task.get('computed_progress_pct', task.get('progress_pct', 0))}%",
        task.get("parent_epic", ""),
        str(task.get("size_points", 0)),
    ]
    for task in roadmap.get("big_tasks", [])
]
big_table = table("Big Tasks", ["ID", "Название", "Статус", "Прогресс", "Эпик", "Размер"], big_rows)

warnings = roadmap.get("warnings", [])

if mode == "compact":
    print(program_table)
    print()
    if warnings:
        print("Предупреждения:")
        for warning in warnings:
            print(f"- {warning}")
        print()
    if phase_table:
        print(phase_table)
    focus_rows = [
        [epic.get("id"), epic.get("title", ""), f"{epic.get('computed_progress_pct', epic.get('progress_pct', 0))}%"]
        for epic in roadmap.get("epics", [])
        if epic.get("status") in {"in_progress", "review"}
    ]
    if focus_rows:
        print()
        print(table("Активные эпики", ["ID", "Название", "Прогресс"], focus_rows))
    upcoming = [m for m in milestones if m.get("status") != "done"]
    if upcoming:
        upcoming.sort(key=lambda m: m.get("due", ""))
        print()
        print(table("Ближайшая веха", ["Веха", "Срок", "Статус"], [[
            upcoming[0].get("title", ""),
            upcoming[0].get("due", "n/a"),
            upcoming[0].get("status", "planned"),
        ]]))
    sys.exit(0)

sections = [program_table, "", phase_table, "", milestone_table, "", epic_table, "", big_table]
print("\n".join(section for section in sections if section))
if warnings:
    print()
    print("Предупреждения:")
    for warning in warnings:
        print(f"- {warning}")
PY
