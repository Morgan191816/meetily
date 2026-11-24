//! 06 任务导入 Excel — 移植自 `meeting_transcribe/assistant/tools/excel_export.py`。
//!
//! 空单元格一律 `""`（不留空格/横线），不产生空行；`上级任务` / `开始日期` /
//! `开放下级任务` 恒空（Python 侧除非原文明确指定，否则留空）。

use chrono::NaiveDate;
use rust_xlsxwriter::{Format, Workbook};

use crate::minutes::llm::TaskItem;
use crate::minutes::urgency::determine_urgency;

/// OA 导入标准 12 列，顺序固定。
const HEADERS: [&str; 12] = [
    "任务名称",
    "责任人",
    "参与人",
    "结束日期",
    "上级任务",
    "紧急程度",
    "开始日期",
    "描述",
    "风险点",
    "难度点",
    "需协助点",
    "开放下级任务",
];

const SHEET_NAME: &str = "任务导入";
const COLUMN_WIDTH: f64 = 18.0;
/// `结束日期` 列（0 起）。
const DATE_COL: usize = 3;

/// 生成 OA 导入 xlsx 的字节内容；无任务时只有表头行。
pub fn build_workbook_bytes(tasks: &[TaskItem], today: NaiveDate) -> Result<Vec<u8>, String> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name(SHEET_NAME).map_err(|e| e.to_string())?;

    let bold = Format::new().set_bold();
    let date = Format::new().set_num_format("YYYY-MM-DD");

    for (col, header) in HEADERS.iter().enumerate() {
        let col = col as u16;
        sheet
            .write_string_with_format(0, col, *header, &bold)
            .map_err(|e| e.to_string())?;
        sheet
            .set_column_width(col, COLUMN_WIDTH)
            .map_err(|e| e.to_string())?;
    }

    for (row, task) in tasks.iter().enumerate() {
        let row = row as u32 + 1;
        for (col, value) in build_row(task, today).iter().enumerate() {
            // 空值写空串（xlsxwriter 省略该单元格），日期列空值与 Python 一样不套格式。
            if col == DATE_COL && !value.is_empty() {
                sheet
                    .write_string_with_format(row, DATE_COL as u16, value, &date)
                    .map_err(|e| e.to_string())?;
            } else {
                sheet
                    .write_string(row, col as u16, value)
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    workbook.save_to_buffer().map_err(|e| e.to_string())
}

/// 单行恰好 12 列；空值为 `""`。
fn build_row(task: &TaskItem, today: NaiveDate) -> [String; 12] {
    let deadline = task.deadline.trim();
    let urgency = determine_urgency(
        (!deadline.is_empty()).then_some(deadline),
        &task.task_name,
        &task.description,
        today,
    );

    [
        cell(&task.task_name),
        cell(&task.owner),
        cell(&task.participants),
        cell(&task.deadline),
        String::new(), // 上级任务
        urgency.to_string(),
        String::new(), // 开始日期
        cell(&task.description),
        cell(&task.risk),
        cell(&task.difficulty),
        cell(&task.help_needed),
        String::new(), // 开放下级任务
    ]
}

/// 对齐 Python `_get`：去首尾空白，纯空白视为空串。
fn cell(raw: &str) -> String {
    raw.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{open_workbook_auto_from_rs, DataType, Reader};
    use std::io::Cursor;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 4).unwrap()
    }

    /// 用 calamine 读回工作表；被省略的空单元格一律得到 `""`。
    fn read_rows(bytes: &[u8]) -> Vec<Vec<String>> {
        let cursor = Cursor::new(bytes.to_vec());
        let mut workbook = open_workbook_auto_from_rs(cursor).unwrap();
        let range = workbook.worksheet_range(SHEET_NAME).unwrap();
        range
            .rows()
            .map(|row| {
                row.iter()
                    .map(|cell| cell.as_string().unwrap_or_default())
                    .collect()
            })
            .collect()
    }

    fn task_fixture() -> TaskItem {
        TaskItem {
            task_name: "跟进冯亚项目".into(),
            owner: "许日政".into(),
            participants: "冯亚、李四".into(),
            deadline: "2026-08-05".into(),
            description: "确认进度".into(),
            risk: "供应商延期".into(),
            difficulty: "跨部门".into(),
            help_needed: "采购协助".into(),
        }
    }

    fn task_with_deadline(deadline: &str) -> TaskItem {
        TaskItem {
            deadline: deadline.into(),
            ..task_fixture()
        }
    }

    #[test]
    fn header_is_twelve_columns_in_order() {
        let bytes = build_workbook_bytes(&[], today()).unwrap();
        let rows = read_rows(&bytes);
        assert_eq!(rows.len(), 1); // 无任务时只有表头
        assert_eq!(
            rows[0],
            vec![
                "任务名称",
                "责任人",
                "参与人",
                "结束日期",
                "上级任务",
                "紧急程度",
                "开始日期",
                "描述",
                "风险点",
                "难度点",
                "需协助点",
                "开放下级任务"
            ]
        );
    }

    #[test]
    fn empty_cells_are_empty_strings_and_no_blank_rows() {
        let rows = read_rows(&build_workbook_bytes(&[task_fixture()], today()).unwrap());
        assert_eq!(rows.len(), 2); // 表头 + 1 行
        assert_eq!(rows[1][4], ""); // 上级任务
        assert_eq!(rows[1][6], ""); // 开始日期
        assert_eq!(rows[1][11], ""); // 开放下级任务
        assert_eq!(rows[1][0], "跟进冯亚项目");
        assert_eq!(rows[1][3], "2026-08-05");
    }

    #[test]
    fn urgency_is_computed() {
        let rows =
            read_rows(&build_workbook_bytes(&[task_with_deadline("2026-08-05")], today()).unwrap());
        assert_eq!(rows[1][5], "紧急");
    }

    #[test]
    fn whitespace_only_values_become_empty() {
        let mut task = task_fixture();
        task.risk = "   ".into();
        task.owner = " 许日政 ".into();
        let rows = read_rows(&build_workbook_bytes(&[task], today()).unwrap());
        assert_eq!(rows[1][8], "");
        assert_eq!(rows[1][1], "许日政");
    }
}
