"""临时脚本：用 Python 桥接层生成 parity 黄金结构（只打印 JSON，不写仓库文件）。

复现：
    cd "D:/claude project/local meeting"
    PYTHONIOENCODING=utf-8 python reference/meetily/frontend/src-tauri/tests/fixtures/minutes/gen_expected.py
"""

import json
import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(r"D:/claude project/local meeting")
sys.path.insert(0, str(ROOT))

from meeting_transcribe.assistant_bridge import run_meeting_assistant  # noqa: E402

FIX = ROOT / "reference/meetily/frontend/src-tauri/tests/fixtures/minutes"
TITLE = "测试会议"


class StubLLM:
    def __init__(self, step2, tasks):
        self._step2 = step2
        self._tasks = tasks
        self.calls = []

    @staticmethod
    def extract_json(text, expect=None):
        try:
            data = json.loads(text)
        except Exception:
            return None
        return data if expect is None or isinstance(data, expect) else None

    def chat(self, prompt, system_prompt=None, temperature=0.3, max_tokens=2048):
        self.calls.append(prompt)
        if "行动项" in prompt:
            return json.dumps(self._tasks, ensure_ascii=False)
        return json.dumps(self._step2, ensure_ascii=False)


def h2_sections(md):
    return re.findall(r"^## (.+)$", md, re.M)


def frontmatter_keys(md):
    assert md.startswith("---\n")
    block = md.split("---\n", 2)[1]
    return [line.split(":", 1)[0] for line in block.splitlines() if ":" in line]


def marker_of(text):
    match = re.search(r"〔(已溯源|推论|待核实)〕", text)
    return match.group(1) if match else ""


def main():
    step2 = json.loads((FIX / "step2_response.json").read_text(encoding="utf-8"))
    tasks = json.loads((FIX / "step3_response.json").read_text(encoding="utf-8"))
    members = json.loads((FIX / "members.json").read_text(encoding="utf-8"))

    tmp = Path(tempfile.mkdtemp(prefix="minutes-parity-"))
    out = tmp / "out"
    meeting_dir = out / TITLE
    meeting_dir.mkdir(parents=True)
    (meeting_dir / "members.json").write_text(
        json.dumps(members, ensure_ascii=False), encoding="utf-8")
    transcript = tmp / "01_原文.md"
    transcript.write_text(
        (FIX / "transcript_01.md").read_text(encoding="utf-8"), encoding="utf-8")

    files = run_meeting_assistant(
        transcript_path=transcript, output_dir=out, title=TITLE,
        llm=StubLLM(step2, tasks))

    md = Path(files["标准纪要"]).read_text(encoding="utf-8")
    report = Path(files["质检"]).read_text(encoding="utf-8")

    decision_lines = re.findall(r"^- 决策-\d+：(.*)$", md, re.M)
    action_rows = re.findall(
        r"^\| (\d+) \| ([^|]*) \| ([^|]*) \| ([^|]*) \| ([^|]*) \| ([^|]*) \|$",
        md, re.M)
    action_header = next(
        line for line in md.splitlines() if line.startswith("| 序号 | 任务名称 |"))

    from openpyxl import load_workbook
    wb = load_workbook(files["导入"])
    ws = wb.active
    rows = [[("" if cell.value is None else str(cell.value)) for cell in row]
            for row in ws.iter_rows()]

    stats = [line for line in report.splitlines() if line.startswith("- ✅ 已验证:")]
    quality_stats = [line for line in report.splitlines() if line.startswith("- [错误]")]

    result = {
        "minutes": {
            "frontmatter_keys": frontmatter_keys(md),
            "h2_sections": h2_sections(md),
            "core_conclusion_count": len(re.findall(r"^- .+$", md.split("## 核心结论")[1].split("## ")[0], re.M)) if "## 核心结论" in md else 0,
            "decision_count": len(decision_lines),
            "decision_markers": [marker_of(line) for line in decision_lines],
            "action_table_header": action_header,
            "action_row_count": len(action_rows),
            "action_markers": [marker_of(row[1]) for row in action_rows],
            "has_open_issues": "## 待决问题" in md,
            "has_next_meeting": "## 下次会议" in md,
            "has_evidence": "## 溯源出处" in md,
        },
        "excel": {
            "headers": rows[0],
            "row_count": len(rows),
            "data_row_count": len(rows) - 1,
            "rows": rows[1:],
        },
        "quality": {
            "h2_sections": h2_sections(report),
            "traceability_stats": stats,
            "quality_stats": quality_stats,
            "has_pending_feedback_section": "## 待确认沉淀候选" in report,
        },
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
