use regex::Regex;
use std::sync::OnceLock;

pub const SECRET_TAG: &str = "secret";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretDetection {
    pub kind: SecretKind,
    pub tag: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    AwsAccessKey,
    GitHubToken,
    PrivateKey,
    BearerToken,
    Assignment,
}

pub fn detect_secret_like_content(content: &str) -> Vec<SecretDetection> {
    secret_patterns()
        .iter()
        .filter(|pattern| pattern.regex.is_match(content))
        .map(|pattern| SecretDetection {
            kind: pattern.kind,
            tag: pattern.tag,
        })
        .collect()
}

pub fn add_secret_tags(content: &str, tags: &[String]) -> Vec<String> {
    let detections = detect_secret_like_content(content);
    if detections.is_empty() {
        return tags.to_vec();
    }

    let mut tagged = tags.to_vec();
    push_tag_once(&mut tagged, SECRET_TAG);
    for detection in detections {
        push_tag_once(&mut tagged, detection.tag);
    }
    tagged
}

#[derive(Debug)]
struct SecretPattern {
    regex: Regex,
    kind: SecretKind,
    tag: &'static str,
}

fn secret_patterns() -> &'static [SecretPattern] {
    static PATTERNS: OnceLock<Vec<SecretPattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            SecretPattern {
                regex: Regex::new(r"\bAKIA[0-9A-Z]{16}\b").expect("valid AWS key regex"),
                kind: SecretKind::AwsAccessKey,
                tag: "secret:aws_access_key",
            },
            SecretPattern {
                regex: Regex::new(r"\bgh[pousr]_[A-Za-z0-9_]{36,}\b")
                    .expect("valid GitHub token regex"),
                kind: SecretKind::GitHubToken,
                tag: "secret:github_token",
            },
            SecretPattern {
                regex: Regex::new(
                    r"(?m)-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
                )
                .expect("valid private key regex"),
                kind: SecretKind::PrivateKey,
                tag: "secret:private_key",
            },
            SecretPattern {
                regex: Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{20,}\b")
                    .expect("valid bearer token regex"),
                kind: SecretKind::BearerToken,
                tag: "secret:bearer_token",
            },
            SecretPattern {
                regex: Regex::new(
                    r#"(?im)\b(?:api[_-]?key|access[_-]?token|auth[_-]?token|client[_-]?secret|password|secret)\b\s*[:=]\s*['"]?[^\s'",;]{8,}"#,
                )
                .expect("valid assignment secret regex"),
                kind: SecretKind::Assignment,
                tag: "secret:assignment",
            },
        ]
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
    fn detects_common_secret_patterns_deterministically() {
        let content = "aws=AKIA1234567890ABCDEF\n\
            github=ghp_1234567890abcdefghijklmnopqrstuvwxyzABCD\n\
            Authorization: Bearer abcdefghijklmnopqrstuvwxyz123456\n\
            password = correct-horse-battery-staple";

        let first = detect_secret_like_content(content);
        let second = detect_secret_like_content(content);

        assert_eq!(first, second);
        assert_eq!(
            first
                .iter()
                .map(|detection| detection.kind)
                .collect::<Vec<_>>(),
            vec![
                SecretKind::AwsAccessKey,
                SecretKind::GitHubToken,
                SecretKind::BearerToken,
                SecretKind::Assignment,
            ]
        );
    }

    #[test]
    fn detects_private_key_blocks() {
        let content = "-----BEGIN PRIVATE KEY-----\nabc123\n-----END PRIVATE KEY-----";

        assert_eq!(
            detect_secret_like_content(content),
            vec![SecretDetection {
                kind: SecretKind::PrivateKey,
                tag: "secret:private_key",
            }]
        );
    }

    #[test]
    fn avoids_obvious_false_positives() {
        let content = "The password field is required. Visit https://example.com/api/key/docs.";

        assert!(detect_secret_like_content(content).is_empty());
    }

    #[test]
    fn adds_stable_secret_tags_without_duplicates() {
        let tags = vec!["demo".to_string(), "secret".to_string()];

        let tagged = add_secret_tags("api_key = abcdef1234567890", &tags);

        assert_eq!(
            tagged,
            vec![
                "demo".to_string(),
                "secret".to_string(),
                "secret:assignment".to_string(),
            ]
        );
    }
}
