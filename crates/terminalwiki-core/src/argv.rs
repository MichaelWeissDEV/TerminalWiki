//! Small, shell-free argv tokenizer for configured local commands.

use crate::{Error, Result};

/// Splits a command setting into argv without evaluating shell syntax.
///
/// Single and double quotes group whitespace; backslash escapes the next
/// character except within single quotes. Pipes, redirects, substitutions and
/// variable expansions are ordinary argument characters, never executable
/// syntax.
pub fn parse_command_argv(command: &str) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;

    for ch in command.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if matches!(ch, '\'' | '"') {
            if quote == Some(ch) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(ch);
            } else {
                current.push(ch);
            }
            continue;
        }
        if ch.is_whitespace() && quote.is_none() {
            if !current.is_empty() {
                args.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }

    if escaped || quote.is_some() {
        return Err(Error::invalid_arguments(
            "Command has an unfinished quote or escape",
        ));
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::parse_command_argv;

    #[test]
    fn command_is_tokenized_without_a_shell() {
        assert_eq!(
            parse_command_argv("nvim -u 'my vimrc' --clean").unwrap(),
            ["nvim", "-u", "my vimrc", "--clean"]
        );
    }

    #[test]
    fn malformed_command_is_rejected() {
        assert!(parse_command_argv("nvim 'unterminated").is_err());
    }
}
