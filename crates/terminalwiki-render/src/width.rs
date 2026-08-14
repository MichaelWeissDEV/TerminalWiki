use unicode_width::UnicodeWidthStr;

pub fn display_width(s: &str) -> usize {
    s.width()
}

pub fn wrap_text(s: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    for word in s.split_whitespace() {
        if current_line.is_empty() {
            current_line.push_str(word);
        } else if display_width(&current_line) + 1 + display_width(word) <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    lines
}
