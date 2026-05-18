//! Build snippet context for a Finding. Pulls ±2 lines around a byte offset
//! from the source so the user sees enough context to act.

const CONTEXT_LINES: usize = 2;

pub fn snippet(src: &str, line_1based: u32) -> String {
    let line = line_1based.saturating_sub(1) as usize;
    let start = line.saturating_sub(CONTEXT_LINES);
    let end = line + CONTEXT_LINES + 1;
    src.lines()
        .enumerate()
        .skip(start)
        .take(end - start)
        .map(|(i, l)| {
            let marker = if i == line { ">" } else { " " };
            format!("{}{:>5} | {}", marker, i + 1, l)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_includes_context_lines() {
        let src = "a\nb\nc\nd\ne\n";
        let snip = snippet(src, 3);
        assert!(snip.contains(">    3 | c"));
        assert!(snip.contains("     1 | a"));
        assert!(snip.contains("     5 | e"));
    }
}
