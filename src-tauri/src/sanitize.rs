//! 富文本描述（description）HTML 净化
//!
//! description 由前端 Tiptap 编辑器产出 HTML，存储前必须白名单净化：
//! - 防止 XSS（script/事件属性/危险 URL）
//! - 防止 CSS 注入（如 position:fixed 遮罩、background:url 外链）
//! - 备份导入（import_data）绕过命令层，是主要注入向量，必须同样走此净化
//!
//! 允许的富文本能力与前端 Tiptap 配置对齐：加粗/斜体/下划线/删除线、
//! 字号/颜色（span style）、标题、列表、引用、链接。

use ammonia::{Builder, UrlRelative};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

/// 描述字段长度上限（与 models.rs 中 validate(length(max = 20000)) 对齐）
pub const DESCRIPTION_MAX_LEN: usize = 20000;

/// 对富文本 HTML 做白名单净化，返回安全的 HTML。
///
/// 只放行 style 中 color / background-color / font-size / text-align 声明，
/// 且值不得包含 `url(`（禁止外链资源），防止 CSS 注入。
pub fn sanitize_description_html(html: &str) -> String {
    Builder::default()
        // 描述不需要图片（img 默认放行，移除）
        .rm_tags(&["img"])
        // 放行 span/p 等的 style 属性（其他属性一律丢弃）
        .generic_attributes(HashSet::from(["style"]))
        // 链接属性白名单（注意：rel 由 ammonia 统一管理并自动追加 noopener noreferrer，
        // 不允许在此显式放行，否则触发 ammonia 断言）
        .tag_attributes(HashMap::from([("a", HashSet::from(["href", "target"]))]))
        // 值级过滤：仅保留安全 CSS 声明；其余属性原样放行
        .attribute_filter(|_tag: &str, attr: &str, value: &str| -> Option<Cow<'_, str>> {
            if attr == "style" {
                const ALLOWED: [&str; 4] = ["color", "background-color", "font-size", "text-align"];
                let kept: Vec<String> = value
                    .split(';')
                    .filter_map(|decl| {
                        let mut it = decl.splitn(2, ':');
                        let prop = it.next()?.trim();
                        let val = it.next()?.trim();
                        let prop_ok = ALLOWED.contains(&prop);
                        let val_ok = !val.to_ascii_lowercase().contains("url(");
                        (prop_ok && val_ok).then(|| format!("{prop}: {val}"))
                    })
                    .collect();
                if kept.is_empty() {
                    None
                } else {
                    Some(Cow::Owned(kept.join("; ")))
                }
            } else {
                Some(Cow::Owned(value.to_string()))
            }
        })
        .url_relative(UrlRelative::PassThrough)
        .clean(html)
        .to_string()
}

/// 去除 HTML 标签（状态机，仅用于"内容是否为空"判断，不做实体解码）
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// 归一化描述：净化 → 去标签后空白则视为未设置（None）→ 截断兜底
pub fn normalize_description(desc: Option<String>) -> Option<String> {
    let clean = sanitize_description_html(&desc?);
    if strip_tags(&clean).trim().is_empty() {
        None
    } else {
        Some(clean.chars().take(DESCRIPTION_MAX_LEN).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_basic_formatting() {
        let html = r#"<p><strong>粗体</strong><em>斜体</em><u>下划线</u></p>"#;
        let out = sanitize_description_html(html);
        assert!(out.contains("<strong>"));
        assert!(out.contains("<em>"));
        assert!(out.contains("<u>"));
    }

    #[test]
    fn keeps_span_color_and_font_size() {
        let html = r#"<p>正常<span style="color: #ff0000; font-size: 16px;">红字大号</span></p>"#;
        let out = sanitize_description_html(html);
        assert!(out.contains("color: #ff0000"));
        assert!(out.contains("font-size: 16px"));
    }

    #[test]
    fn strips_img_and_event_attributes() {
        let html = r#"<p>x<img src="x.png" onerror="alert(1)">y</p>"#;
        let out = sanitize_description_html(html);
        assert!(!out.contains("<img"));
        assert!(!out.contains("onerror"));
        assert!(out.contains("x"));
        assert!(out.contains("y"));
    }

    #[test]
    fn strips_script_and_style_url() {
        let html = r#"<p onclick="evil()"><script>alert(1)</script>安全<span style="color:red; background-image:url(https://evil/x)">s</span></p>"#;
        let out = sanitize_description_html(html);
        assert!(!out.contains("<script"));
        assert!(!out.contains("onclick"));
        assert!(!out.contains("url("));
        assert!(out.contains("color: red"));
        assert!(out.contains("安全"));
    }

    #[test]
    fn keeps_plain_text_newlines() {
        let out = sanitize_description_html("第一行\n第二行");
        assert!(out.contains("第一行"));
        assert!(out.contains("第二行"));
        assert!(out.contains('\n'));
    }

    #[test]
    fn normalize_empties_to_none() {
        assert_eq!(normalize_description(Some("<p></p>".into())), None);
        assert_eq!(normalize_description(Some("  ".into())), None);
        assert_eq!(normalize_description(None), None);
        assert_eq!(
            normalize_description(Some("<p>hi</p>".into())),
            Some("<p>hi</p>".to_string())
        );
    }
}
