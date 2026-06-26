use crate::ContentType;

pub const TYPE_TAG_PREFIX: &str = "type:";

pub fn resolved_content_type(content_type: ContentType, content: &str) -> ContentType {
    match content_type {
        ContentType::PlainText | ContentType::Unknown => infer_content_type(content),
        explicit => explicit,
    }
}

pub fn add_type_tag(content_type: ContentType, tags: &[String]) -> Vec<String> {
    let mut tagged = tags.to_vec();
    push_tag_once(
        &mut tagged,
        &format!("{TYPE_TAG_PREFIX}{}", content_type.as_str()),
    );
    tagged
}

pub fn infer_content_type(content: &str) -> ContentType {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return ContentType::PlainText;
    }

    if looks_like_url(trimmed) {
        return ContentType::Url;
    }

    if looks_like_json(trimmed) {
        return ContentType::Json;
    }

    if looks_like_diff(trimmed) {
        return ContentType::Diff;
    }

    if looks_like_stack_trace(trimmed) {
        return ContentType::StackTrace;
    }

    if looks_like_log(trimmed) {
        return ContentType::Log;
    }

    if looks_like_code(trimmed) {
        return ContentType::Code;
    }

    ContentType::PlainText
}

fn looks_like_url(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    !content.contains(char::is_whitespace)
        && (lower.starts_with("http://") || lower.starts_with("https://"))
}

fn looks_like_json(content: &str) -> bool {
    let starts_like_json = (content.starts_with('{') && content.ends_with('}'))
        || (content.starts_with('[') && content.ends_with(']'));
    starts_like_json && serde_json::from_str::<serde_json::Value>(content).is_ok()
}

fn looks_like_diff(content: &str) -> bool {
    content.starts_with("diff --git ")
        || content.starts_with("@@ ")
        || content
            .lines()
            .take(8)
            .any(|line| line.starts_with("+++ ") || line.starts_with("--- "))
}

fn looks_like_stack_trace(content: &str) -> bool {
    content.contains("stack backtrace:")
        || content.lines().any(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("at ")
                || trimmed.starts_with("File \"")
                || trimmed.starts_with("Traceback (most recent call last):")
        })
}

fn looks_like_log(content: &str) -> bool {
    content.lines().take(8).any(|line| {
        let line = line.trim_start();
        line.starts_with("ERROR")
            || line.starts_with("WARN")
            || line.starts_with("INFO")
            || line.starts_with("DEBUG")
            || line.starts_with("TRACE")
    })
}

fn looks_like_code(content: &str) -> bool {
    let code_markers = [
        "fn ",
        "let ",
        "const ",
        "function ",
        "class ",
        "import ",
        "use ",
        "=>",
        "pub ",
        "#include",
    ];
    content.lines().take(12).any(|line| {
        let trimmed = line.trim_start();
        code_markers
            .iter()
            .any(|marker| trimmed.starts_with(marker) || trimmed.contains(marker))
    })
}

fn push_tag_once(tags: &mut Vec<String>, tag: &str) {
    if !tags.iter().any(|existing| existing == tag) {
        tags.push(tag.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_common_content_types() {
        assert_eq!(
            infer_content_type("https://example.com/path?q=1"),
            ContentType::Url
        );
        assert_eq!(infer_content_type("{\"ok\":true}"), ContentType::Json);
        assert_eq!(
            infer_content_type("diff --git a/a.rs b/a.rs\n+added"),
            ContentType::Diff
        );
        assert_eq!(
            infer_content_type("Traceback (most recent call last):\n  File \"app.py\""),
            ContentType::StackTrace
        );
        assert_eq!(infer_content_type("ERROR request failed"), ContentType::Log);
        assert_eq!(infer_content_type("fn main() {}"), ContentType::Code);
        assert_eq!(infer_content_type("normal note"), ContentType::PlainText);
    }

    #[test]
    fn explicit_content_type_is_preserved() {
        assert_eq!(
            resolved_content_type(ContentType::Code, "normal note"),
            ContentType::Code
        );
    }

    #[test]
    fn adds_type_tag_without_duplicates() {
        let tags = vec!["demo".to_string(), "type:json".to_string()];

        assert_eq!(
            add_type_tag(ContentType::Json, &tags),
            vec!["demo".to_string(), "type:json".to_string()]
        );
    }
}
