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
    // 空值必须留一个 <c> 元素，否则按单元格顺序导入会左移；而 write_blank 只在
    // 格式非默认（xf_index > 0）时才输出，所以这里必须挂一个无害的非默认格式。
    let plain = Format::new().set_num_format("General");

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
            // 空值写空白格；日期列空值与 Python 一样不套日期格式。
            let result = if value.is_empty() {
                sheet.write_blank(row, col as u16, &plain)
            } else if col == DATE_COL {
                sheet.write_string_with_format(row, col as u16, value, &date)
            } else {
                sheet.write_string(row, col as u16, value)
            };
            result.map_err(|e| e.to_string())?;
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
    use std::io::{Cursor, Read};

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 4).unwrap()
    }

    /// 用 calamine 读回工作表；空白格读作 `""`。
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
    fn every_data_row_has_twelve_cell_elements() {
        // calamine 按 dimension 补齐，分不清「缺失格」与「空格」，只能解包看 XML。
        let bytes =
            build_workbook_bytes(&[task_fixture(), task_with_deadline("")], today()).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut xml = String::new();
        archive
            .by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();

        let rows: Vec<&str> = xml.split("<row ").skip(1).collect();
        assert!(rows.len() >= 3, "表头 + 至少 2 行数据");
        for row in &rows[1..] {
            let body = row.split("</row>").next().unwrap();
            assert_eq!(body.matches("<c ").count(), 12, "每行必须 12 个 <c>：{row}");
        }
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
