//! 内嵌 OA 人员名单 — 对齐 Python 桥接自动加载 `assistant/members.json` 的行为。
//!
//! shortcut: 名单编译期内嵌，更新名单需重新打包

use std::sync::OnceLock;

/// 编译期内嵌，避免打包后的资源路径问题。
const MEMBERS_JSON: &str = include_str!("../../resources/members.json");

#[derive(serde::Deserialize)]
struct Roster {
    members: Vec<Member>,
}

#[derive(serde::Deserialize)]
struct Member {
    name: String,
}

/// 名单姓名（首次调用时解析并缓存）；解析失败返回空名单并告警，不 panic。
pub fn member_names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| match serde_json::from_str::<Roster>(MEMBERS_JSON) {
        Ok(roster) => roster
            .members
            .into_iter()
            .map(|member| member.name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect(),
        Err(error) => {
            log::warn!("内置人员名单解析失败，按空名单处理：{error}");
            Vec::new()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_roster_has_104_names_starting_with_cao_lei() {
        let names = member_names();
        assert_eq!(names.len(), 104);
        assert_eq!(names[0], "曹磊");
    }
}
