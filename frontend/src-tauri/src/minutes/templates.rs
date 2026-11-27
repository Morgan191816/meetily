//! 会议类型模板注册表。
//!
//! 模板只改提示词与渲染章节顺序，不增删 step2 JSON 顶层键；可变内容一律走
//! `custom_sections: [{heading, markdown}]`。

/// 渲染区块；`Custom` 展开为模板声明的自定义章节（按 heading 精确匹配）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Overview,
    Custom,
    Topics,
    Decisions,
    Actions,
    Risks,
    OpenIssues,
    NextMeeting,
}

/// 模板声明的自定义章节：标题用于提示词与渲染匹配，提示用于指导模型写什么。
pub struct CustomSectionSpec {
    pub heading: &'static str,
    pub hint: &'static str,
}

pub struct MinutesTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub overview_hint: &'static str,
    pub extra_instructions: &'static str,
    pub custom_sections: &'static [CustomSectionSpec],
    pub sections: &'static [Section],
}

pub const DEFAULT_ID: &str = "general";

static TEMPLATES: &[MinutesTemplate] = &[
    MinutesTemplate {
        id: DEFAULT_ID,
        name: "通用纪要",
        description: "适用于日常会议，完整记录摘要、议题、决策与行动项",
        overview_hint: "150-250 字，按「会议目的 → 主要结论 → 未决事项」组织",
        extra_instructions: "",
        custom_sections: &[],
        sections: &[
            Section::Overview,
            Section::Topics,
            Section::Decisions,
            Section::Actions,
            Section::Risks,
            Section::OpenIssues,
            Section::NextMeeting,
        ],
    },
    MinutesTemplate {
        id: "standup",
        name: "例会/周会",
        description: "快速同步进展与阻塞，按人/工作流组织",
        overview_hint: "80-150 字，条目化，先说结论",
        extra_instructions: "不写讨论过程，按人/工作流组织。",
        custom_sections: &[
            CustomSectionSpec {
                heading: "上周行动项回顾",
                hint: "逐条列出上周行动项的完成情况，未完成的说明原因",
            },
            CustomSectionSpec {
                heading: "本周进展",
                hint: "按人/工作流列出本周已完成与计划中的工作",
            },
            CustomSectionSpec {
                heading: "阻塞与风险",
                hint: "列出当前阻塞事项、影响与需要谁协助",
            },
        ],
        sections: &[
            Section::Overview,
            Section::Custom,
            Section::Decisions,
            Section::Actions,
            Section::NextMeeting,
        ],
    },
    MinutesTemplate {
        id: "review",
        name: "项目评审",
        description: "记录评审对象、材料、结论与修改要求",
        overview_hint: "150-250 字，点明评审对象与最终结论",
        extra_instructions: "结论必须明确写通过/有条件通过/不通过，并给出修改要求（人+期限）。",
        custom_sections: &[
            CustomSectionSpec {
                heading: "评审对象与材料",
                hint: "说明评审对象、依据的材料与评审范围",
            },
            CustomSectionSpec {
                heading: "评审结论",
                hint: "明确写通过/有条件通过/不通过，并列出修改要求（人+期限）",
            },
        ],
        sections: &[
            Section::Overview,
            Section::Custom,
            Section::Topics,
            Section::Decisions,
            Section::Actions,
            Section::Risks,
            Section::NextMeeting,
        ],
    },
    MinutesTemplate {
        id: "client",
        name: "客户沟通",
        description: "记录客户诉求与我方承诺边界",
        overview_hint: "150-250 字，点明客户诉求与我方承诺",
        extra_instructions: "严格区分客户说的与我们承诺的，不做内部评价。",
        custom_sections: &[
            CustomSectionSpec {
                heading: "客户现状与诉求",
                hint: "客观记录客户当前情况与提出的诉求",
            },
            CustomSectionSpec {
                heading: "我方承诺与边界",
                hint: "只写我方明确承诺的事项与不承诺的边界",
            },
        ],
        sections: &[
            Section::Overview,
            Section::Custom,
            Section::Topics,
            Section::OpenIssues,
            Section::Actions,
            Section::NextMeeting,
        ],
    },
    MinutesTemplate {
        id: "retro",
        name: "复盘",
        description: "记录做得好、待改进与可复用经验",
        overview_hint: "150-250 字，点明主要差距与改进方向",
        extra_instructions: "按差距—根因—改进组织，不做表扬流水账。",
        custom_sections: &[
            CustomSectionSpec {
                heading: "做得好的",
                hint: "列出本次做得好的具体做法及效果",
            },
            CustomSectionSpec {
                heading: "待改进与根因",
                hint: "按差距—根因组织，指出可验证的根因",
            },
            CustomSectionSpec {
                heading: "可复用经验",
                hint: "提炼可复用到后续项目的做法",
            },
        ],
        sections: &[
            Section::Overview,
            Section::Custom,
            Section::Actions,
            Section::OpenIssues,
        ],
    },
];

pub fn all() -> &'static [MinutesTemplate] {
    TEMPLATES
}

/// 未知 id 回退 `general` 并记 warn；模板 id 来自持久化/前端，不能因此失败。
pub fn find(id: &str) -> &'static MinutesTemplate {
    TEMPLATES.iter().find(|template| template.id == id).unwrap_or_else(|| {
        log::warn!("未知会议纪要模板 {id}，回退 {DEFAULT_ID}");
        &TEMPLATES[0]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_five_templates() {
        let ids: Vec<&str> = all().iter().map(|template| template.id).collect();
        assert_eq!(ids, vec!["general", "standup", "review", "client", "retro"]);
    }

    #[test]
    fn section_orders_match_template_definitions() {
        use Section::*;
        assert_eq!(
            find("general").sections,
            &[Overview, Topics, Decisions, Actions, Risks, OpenIssues, NextMeeting]
        );
        assert_eq!(
            find("standup").sections,
            &[Overview, Custom, Decisions, Actions, NextMeeting]
        );
        assert_eq!(
            find("review").sections,
            &[Overview, Custom, Topics, Decisions, Actions, Risks, NextMeeting]
        );
        assert_eq!(
            find("client").sections,
            &[Overview, Custom, Topics, OpenIssues, Actions, NextMeeting]
        );
        assert_eq!(
            find("retro").sections,
            &[Overview, Custom, Actions, OpenIssues]
        );
    }

    #[test]
    fn unknown_id_falls_back_to_general() {
        assert_eq!(find("unknown").id, DEFAULT_ID);
        assert_eq!(find("").id, DEFAULT_ID);
    }
}
