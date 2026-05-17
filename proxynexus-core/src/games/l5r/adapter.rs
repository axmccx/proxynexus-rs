pub struct L5rAdapter {}

impl Default for L5rAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl L5rAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

fn build_title(name: &str, name_extra: Option<&str>) -> String {
    match name_extra {
        Some(extra) if !extra.is_empty() => format!("{} ({})", name, extra),
        _ => name.to_string(),
    }
}

fn parse_position(s: Option<&str>) -> Option<i64> {
    s.and_then(|v| v.parse::<i64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_title_without_extra_returns_name_only() {
        assert_eq!(build_title("A Bad Death", None), "A Bad Death");
    }

    #[test]
    fn build_title_with_empty_extra_returns_name_only() {
        assert_eq!(build_title("A Bad Death", Some("")), "A Bad Death");
    }

    #[test]
    fn build_title_with_extra_appends_parenthesized_suffix() {
        assert_eq!(
            build_title("A Fate Worse Than Death", Some("2")),
            "A Fate Worse Than Death (2)"
        );
    }

    #[test]
    fn parse_position_returns_some_for_numeric_string() {
        assert_eq!(parse_position(Some("168")), Some(168));
    }

    #[test]
    fn parse_position_returns_none_for_none_input() {
        assert_eq!(parse_position(None), None);
    }

    #[test]
    fn parse_position_returns_none_for_non_numeric_string() {
        assert_eq!(parse_position(Some("xyz")), None);
    }

    #[test]
    fn parse_position_returns_none_for_empty_string() {
        assert_eq!(parse_position(Some("")), None);
    }
}
