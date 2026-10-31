//! Translate full IME buffers into the shared text editor's Unicode operations.
use crate::osk::Output;

pub fn changes(old: &str, new: &str) -> Vec<Output> {
    let prefix = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let mut out: Vec<_> = (prefix..old.chars().count())
        .map(|_| Output::Backspace)
        .collect();
    let tail: String = new.chars().skip(prefix).collect();
    if !tail.is_empty() {
        out.push(Output::Text(tail));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_composition_replacement_and_deletion() {
        for (old, new) in [
            ("привет", "привёт🙂"),
            ("abc🙂", "ab"),
            ("teh", "the"),
            ("", "你好"),
        ] {
            let mut text = old.to_owned();
            for op in changes(old, new) {
                match op {
                    Output::Backspace => {
                        text.pop();
                    }
                    Output::Text(s) => text.push_str(&s),
                    _ => unreachable!(),
                }
            }
            assert_eq!(text, new);
        }
    }
}
