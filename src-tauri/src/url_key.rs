use std::time::{SystemTime, UNIX_EPOCH};

pub fn url_key(raw: &str) -> String {
    let trimmed = raw.trim();
    url::Url::parse(trimmed)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| trimmed.to_string())
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_scheme_and_host_case() {
        assert_eq!(
            url_key("HTTPS://Example.COM/Path"),
            "https://example.com/Path"
        );
    }

    #[test]
    fn adds_trailing_slash_for_bare_origin() {
        assert_eq!(url_key("https://x.com"), "https://x.com/");
    }

    #[test]
    fn keeps_query_and_fragment() {
        assert_eq!(
            url_key("https://example.com/p?q=1#frag"),
            "https://example.com/p?q=1#frag"
        );
    }

    #[test]
    fn keeps_scheme_less_bookmarklet_style_input_as_is() {
        assert_eq!(
            url_key("  alert(document.cookie)  "),
            "alert(document.cookie)"
        );
    }

    #[test]
    fn javascript_bookmarklets_parse_as_opaque_urls() {
        assert_eq!(url_key("  javascript:alert(1)  "), "javascript:alert(1)");
    }
}
