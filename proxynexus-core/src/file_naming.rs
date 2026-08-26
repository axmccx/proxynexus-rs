use std::path::Path;

pub fn back_index(label: &str) -> Option<u32> {
    let rest = label.strip_prefix("back")?;
    if rest.is_empty() {
        return Some(1);
    }
    rest.parse::<u32>().ok().filter(|index| *index >= 1)
}

pub fn back_label(index: u32) -> String {
    if index == 1 {
        "back".to_string()
    } else {
        format!("back{}", index)
    }
}

/// Splits `{card_id}@{printing}[~{side}][.bleed].{extension}` into its parts.
pub fn parse_filename(path: &Path) -> Option<(String, String, String, bool)> {
    let mut stem = path.file_stem()?.to_str()?;

    let has_bleed = if let Some(stripped) = stem.strip_suffix(".bleed") {
        stem = stripped;
        true
    } else {
        false
    };

    let (card_id, rest) = stem.split_once('@')?;

    if rest.contains('@') {
        return None;
    }

    let (printing, side) = if let Some((pr, pt)) = rest.split_once('~') {
        if pt.contains('~') {
            return None;
        }
        (pr.to_string(), pt.to_string())
    } else {
        (rest.to_string(), "front".to_string())
    };

    Some((card_id.to_string(), printing, side, has_bleed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_labels_carry_a_one_based_index() {
        assert_eq!(back_index("back"), Some(1));
        assert_eq!(back_index("back2"), Some(2));
        assert_eq!(back_index("back10"), Some(10));
    }

    #[test]
    fn labels_that_name_no_back_have_no_index() {
        for label in ["front", "front2", "back0", "backside", "face2", "insert"] {
            assert_eq!(back_index(label), None, "label: {}", label);
        }
    }

    #[test]
    fn the_first_back_is_spelled_without_its_number() {
        assert_eq!(back_label(1), "back");
        assert_eq!(back_label(2), "back2");
    }

    #[test]
    fn test_parse_filename_variants() {
        assert_eq!(
            parse_filename(Path::new("hedge_fund@system_gateway.jpg")),
            Some((
                "hedge_fund".to_string(),
                "system_gateway".to_string(),
                "front".to_string(),
                false
            ))
        );

        assert_eq!(
            parse_filename(Path::new("a-legion-of-one@emerald-core-set.jpg")),
            Some((
                "a-legion-of-one".to_string(),
                "emerald-core-set".to_string(),
                "front".to_string(),
                false
            ))
        );

        assert_eq!(
            parse_filename(Path::new(
                "sync_everything_everywhere@data_and_destiny~back.png"
            )),
            Some((
                "sync_everything_everywhere".to_string(),
                "data_and_destiny".to_string(),
                "back".to_string(),
                false
            ))
        );

        assert_eq!(
            parse_filename(Path::new("hedge_fund@system_gateway~front.bleed.jpg")),
            Some((
                "hedge_fund".to_string(),
                "system_gateway".to_string(),
                "front".to_string(),
                true
            ))
        );

        assert_eq!(
            parse_filename(Path::new("hedge_fund@system_gateway.bleed.png")),
            Some((
                "hedge_fund".to_string(),
                "system_gateway".to_string(),
                "front".to_string(),
                true
            ))
        );

        assert_eq!(parse_filename(Path::new("hedge_fund~front.jpg")), None);
        assert_eq!(
            parse_filename(Path::new("hedge_fund@multiple@ats.jpg")),
            None
        );
        assert_eq!(
            parse_filename(Path::new("hedge_fund@dark-theme~back~extra.png")),
            None
        );
    }
}
