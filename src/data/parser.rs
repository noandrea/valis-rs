use super::Entity;

/// Parse a text and returns a list of identified
/// entities
pub fn parse_text(txt: &str) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    let (open_tag, close_tag) = ("[[", "]]");
    // moving offset for finding labels
    let mut offset = 0;
    // while we get a match for the opening tag
    while let Some(oi) = txt[offset..].find(open_tag) {
        // move the offset after the tag
        offset += oi + open_tag.len();
        // search for the closing tag
        match txt[offset..].find(close_tag) {
            Some(eob) => {
                // if there is a closing tag then we
                let (b, e) = (offset, offset + eob);
                labels.push(txt[b..e].to_owned());
                offset += eob + close_tag.len();
            }
            _ => {}
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_txt() {
        let tests = vec![
            (
                "Today we talked with [[Mark]] about the [[VALIS]] project. [[Theresa]] was also there",
                vec!["Mark", "VALIS", "Theresa"],
            ),
            (
                "Nothing here instead",
                vec![],
            ),
            (
                "A big [[ mistake ",
                vec![],
            ),
            (
                "Something [[Good]] something ]]bad[[ something wrong[[",
                vec!["Good"],
            ),
            (
                "[[Good]] or bad [[ something wrong[[]]",
                vec!["Good", ""],
            ),
        ];

        for (i, t) in tests.iter().enumerate() {
            println!("test_getters#{}", i);
            let (text, labels) = t;

            let r = parse_text(text);
            assert_eq!(r, *labels);
        }
    }
}
