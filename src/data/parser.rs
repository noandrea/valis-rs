///advance in a string search for the last consecutive index  of a search string
fn last_consecutive_index(txt: &str, from: usize, search: &str) -> usize {
    let mut index = from + 1;
    if index >= txt.len() {
        return from;
    }
    while txt[index..].starts_with(search) {
        index += 1;
    }
    index - 1
}

/// Parse a text and extract labels matching [[..]] pattern
pub fn find_labels(txt: &str) -> Vec<String> {
    let (open_tag, close_tag) = ("[[", "]]");
    // keep track of all starting offsets
    let mut offsets: Vec<(usize, usize)> = Vec::new();
    // moving offset for finding labels
    match txt.find(open_tag) {
        Some(first_index) => {
            let mut offset = first_index;
            'main: loop {
                match txt[offset..].find(open_tag) {
                    Some(index) => {
                        let index = last_consecutive_index(&txt[offset..], index, open_tag);
                        offsets.push((offset, offset + index));
                        offset += index + open_tag.len();
                    }
                    None => {
                        offsets.push((offset, txt.len()));
                        break 'main;
                    }
                }
            }
        }
        _ => {}
    }
    // now we have the list of indexes [[ ... [[ ... [[
    // can loop and find the closest matching closing tag
    offsets
        .iter()
        .map(|(b, e)| match txt[*b..*e].find(close_tag) {
            Some(ci) => Some(txt[*b..*b + ci].to_owned()),
            _ => None,
        })
        .filter(|v| match v {
            Some(label) => !label.is_empty(),
            _ => false,
        })
        .map(|v| v.unwrap())
        .collect()
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
                "[[ [[Good]] or bad [[ something wrong[[]][[[[[",
                vec!["Good"],
            ),
            (
                "[[[Good]] or [[bad]]] [[ something wrong[[]]",
                vec!["Good", "bad"],
            ),
        ];

        for (i, t) in tests.iter().enumerate() {
            println!("test_getters#{}", i);
            let (text, labels) = t;

            let r = find_labels(text);
            assert_eq!(r, *labels);
        }
    }
}
