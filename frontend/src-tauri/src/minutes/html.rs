//! 会议纪要 Markdown → 自包含 HTML（导出与预览共用）。
//!
//! 输出不依赖外部字体、脚本或网络：一份文件即可双击打开或打印。
//! 安全策略：原始 HTML 整块丢弃；链接仅放行 http/https/mailto/页内锚点；
//! 图片整段丢弃（纪要不需要）。

use pulldown_cmark::{html, Event, Options, Parser, Tag, TagEnd};

/// 溯源标记降噪：只改 HTML 渲染结果，Markdown 源保持原样。
const TRACEABILITY_MARKERS: [(&str, &str); 3] = [
    ("〔已溯源〕", ""),
    ("〔推论〕", r#"<sup class="tag tag-inferred">推论</sup>"#),
    (
        "〔待核实〕",
        r#"<sup class="tag tag-unverified">待核实</sup>"#,
    ),
];

const STYLE: &str = r#"
* { box-sizing: border-box; }
body {
  margin: 0 auto;
  max-width: 820px;
  padding: 32px 24px 64px;
  font-family: "Microsoft YaHei UI","Microsoft YaHei","PingFang SC","Hiragino Sans GB","Source Han Sans SC","Noto Sans SC",system-ui,sans-serif;
  font-size: 16px;
  line-height: 1.7;
  color: #1f2328;
  background: #fff;
  -webkit-print-color-adjust: exact;
  print-color-adjust: exact;
}
p { margin: 0.9em 0; }
h1 { margin: 0 0 1.2em; font-size: 24px; text-align: center; }
h2 { margin: 1.8em 0 0.7em; padding-bottom: 0.3em; font-size: 19px; border-bottom: 1px solid #d0d7de; break-after: avoid; }
h3 { margin: 1.4em 0 0.5em; font-size: 17px; }
ul, ol { margin: 0.9em 0; padding-left: 1.6em; }
li { margin: 0.25em 0; }
table { width: 100%; margin: 1em 0; border-collapse: collapse; font-size: 9.5pt; }
th, td { padding: 4px 8px; border: 1px solid #d0d7de; text-align: left; vertical-align: top; }
thead th { background: #f3f4f6; font-weight: 600; }
/* 行动项首列为序号：收窄并右对齐 */
td:first-child { width: 1%; white-space: nowrap; text-align: right; }
blockquote { margin: 1em 0; padding: 0.2em 1em; border-left: 4px solid #d0d7de; color: #57606a; }
code { padding: 0.1em 0.3em; border-radius: 3px; background: #f3f4f6; font-size: 0.9em; }
pre { padding: 12px; border-radius: 6px; background: #f6f8fa; overflow: auto; }
pre code { padding: 0; background: none; }
a { color: #0969da; }
hr { margin: 1.5em 0; border: 0; border-top: 1px solid #d0d7de; }
details { margin: 0.8em 0; }
summary { color: #57606a; cursor: pointer; }
.tag { margin-left: 0.15em; font-size: 0.75em; font-weight: 600; vertical-align: super; }
.tag-inferred { color: #b45309; }
.tag-unverified { color: #b91c1c; }
@page { size: A4; margin: 18mm 16mm; }
@media print {
  body { max-width: none; padding: 0; font-size: 12pt; }
  h2 { break-after: avoid; }
  tr, li { break-inside: avoid; }
  thead { display: table-header-group; }
  p { orphans: 3; widows: 3; }
  details { display: block; }
  .no-print { display: none !important; }
  a { color: inherit; text-decoration: none; }
}
"#;

/// 把纪要 Markdown 渲染为完整、自包含的 HTML 文档。
pub fn render_html(markdown: &str) -> String {
    let body = strip_frontmatter(markdown);
    let title = extract_title(body);

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut rendered = String::new();
    html::push_html(
        &mut rendered,
        sanitize(Parser::new_ext(body, options)).into_iter(),
    );
    for (marker, replacement) in TRACEABILITY_MARKERS {
        rendered = rendered.replace(marker, replacement);
    }

    format!(
        "<!doctype html>\n<html lang=\"zh-CN\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\">\n\
<title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n{rendered}</body>\n</html>\n",
        escape_html(&title)
    )
}

/// 剥掉开头由 `---` 包裹的 YAML frontmatter；未闭合时原样保留。
fn strip_frontmatter(markdown: &str) -> &str {
    let text = markdown.strip_prefix('\u{feff}').unwrap_or(markdown);
    let Some(rest) = text.strip_prefix("---") else {
        return text;
    };
    // 开头必须是独立的 `---` 行，而不是 `----` 或 `---abc`。
    if !rest.split('\n').next().unwrap_or("").trim().is_empty() {
        return text;
    }
    let mut offset = text.len() - rest.len();
    for line in rest.split_inclusive('\n') {
        offset += line.len();
        if line.trim_end_matches(['\r', '\n']).trim_end() == "---" {
            return &text[offset..];
        }
    }
    text
}

/// `<title>` 取正文第一个 `# ` 标题，无则用「会议纪要」。
fn extract_title(markdown: &str) -> String {
    markdown
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
        .unwrap_or("会议纪要")
        .to_string()
}

/// 过滤事件流：丢原始 HTML、丢弃非白名单链接的标签（保留文字）、整段丢弃图片。
fn sanitize<'a>(parser: impl Iterator<Item = Event<'a>>) -> Vec<Event<'a>> {
    let mut events = Vec::new();
    let mut image_depth = 0usize;
    let mut dropped_links = 0usize;
    for event in parser {
        if image_depth > 0 {
            match event {
                Event::Start(Tag::Image { .. }) => image_depth += 1,
                Event::End(TagEnd::Image) => image_depth -= 1,
                _ => {}
            }
            continue;
        }
        match event {
            // 纪要不需要原始 HTML：<script>/<style>/<details> 等一律丢弃。
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::Start(Tag::Image { .. }) => image_depth = 1,
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                if allowed_url(&dest_url) {
                    events.push(Event::Start(Tag::Link {
                        link_type,
                        dest_url,
                        title,
                        id,
                    }));
                } else {
                    dropped_links += 1;
                }
            }
            Event::End(TagEnd::Link) if dropped_links > 0 => dropped_links -= 1,
            other => events.push(other),
        }
    }
    events
}

/// 链接白名单：http、https、mailto、页内锚点。相对路径与 javascript:/data:/file: 一律拒绝。
fn allowed_url(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("mailto:")
        || url.starts_with('#')
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_yaml_frontmatter() {
        let html = render_html("---\ntype: minutes\ndate: 2026-01-01\n---\n# 标题\n\n正文");
        assert!(!html.contains("type: minutes"), "frontmatter 应被剥离");
        assert!(html.contains("<h1>标题</h1>"));
        assert!(!html.contains("<hr />"), "frontmatter 不应渲染成 hr");
    }

    #[test]
    fn drops_raw_html() {
        let html = render_html("# 标题\n\n<script>alert(1)</script>\n\n正文");
        assert!(!html.contains("<script>"), "原始 HTML 应被丢弃");
        assert!(!html.contains("alert(1)"));
        assert!(html.contains("正文"));
    }

    #[test]
    fn drops_javascript_links_but_keeps_text() {
        let html = render_html("[点我](javascript:alert(1))");
        assert!(!html.contains("javascript:"), "危险链接应被丢弃");
        assert!(!html.contains("<a "), "不应生成链接标签");
        assert!(html.contains("点我"), "链接文字应保留");
    }

    #[test]
    fn drops_relative_links_but_keeps_https_links() {
        let html = render_html("[本地](file:///etc/passwd) [网页](https://example.com)");
        assert!(!html.contains("file:///"));
        assert!(html.contains(r#"<a href="https://example.com">网页</a>"#));
    }

    #[test]
    fn renders_traceability_markers() {
        let html = render_html("- 决策〔已溯源〕〔推论〕〔待核实〕");
        assert!(!html.contains("〔已溯源〕"), "已溯源标记应删除");
        assert!(html.contains(r#"<sup class="tag tag-inferred">推论</sup>"#));
        assert!(html.contains(r#"<sup class="tag tag-unverified">待核实</sup>"#));
    }

    #[test]
    fn renders_tables() {
        let html = render_html("| 序号 | 任务 |\n|---|---|\n| 1 | 写代码 |");
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>序号</th>"));
        assert!(html.contains("<td>写代码</td>"));
    }

    #[test]
    fn title_comes_from_first_h1() {
        let html = render_html("# 项目周会\n\n## 议题");
        assert!(html.contains("<title>项目周会</title>"));
    }

    #[test]
    fn default_title_when_no_heading() {
        assert!(render_html("正文").contains("<title>会议纪要</title>"));
    }
}
