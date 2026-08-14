//! Query AST and robust parser (spec §13, §14, §15).

use std::str::FromStr;

/// A single term or filter in a search query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryTerm {
    Text(String),
    Tag(String),
    Wiki(String),
    Type(String),
    Ext(String),
    Path(String),
    Title(String),
    LinksTo(String),
    Backlink(String),
    Not(Box<QueryTerm>),
}

/// A structured query containing multiple conjunction terms.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub terms: Vec<QueryTerm>,
}

impl Query {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

impl FromStr for Query {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut terms = Vec::new();
        let mut chars = s.chars().peekable();

        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
                continue;
            }

            let is_not = if c == '-' {
                chars.next();
                if let Some(&nc) = chars.peek() {
                    if nc.is_whitespace() {
                        terms.push(QueryTerm::Text("-".to_string()));
                        continue;
                    }
                } else {
                    terms.push(QueryTerm::Text("-".to_string()));
                    break;
                }
                true
            } else {
                false
            };

            let mut token = String::new();
            let mut in_quotes = false;
            let mut has_colon = false;

            while let Some(&c) = chars.peek() {
                if c == '"' {
                    chars.next();
                    in_quotes = !in_quotes;
                } else if c == ':' && !in_quotes && !has_colon {
                    has_colon = true;
                    token.push(c);
                    chars.next();
                } else if c.is_whitespace() && !in_quotes {
                    break;
                } else {
                    token.push(c);
                    chars.next();
                }
            }

            if in_quotes {
                return Err("Unclosed quote in search query".to_string());
            }

            let term = if let Some(idx) = token.find(':') {
                let key = &token[..idx];
                let val = token[idx + 1..].trim_matches('"');

                match key.to_ascii_lowercase().as_str() {
                    "tag" => QueryTerm::Tag(val.to_string()),
                    "wiki" => QueryTerm::Wiki(val.to_string()),
                    "type" => QueryTerm::Type(val.to_string()),
                    "ext" => QueryTerm::Ext(val.to_string()),
                    "path" => QueryTerm::Path(val.to_string()),
                    "title" => QueryTerm::Title(val.to_string()),
                    "linksto" => QueryTerm::LinksTo(val.to_string()),
                    "backlink" => QueryTerm::Backlink(val.to_string()),
                    _ => QueryTerm::Text(token),
                }
            } else {
                let val = token.trim_matches('"');
                QueryTerm::Text(val.to_string())
            };

            let final_term = if is_not {
                QueryTerm::Not(Box::new(term))
            } else {
                term
            };

            terms.push(final_term);
        }

        Ok(Query { terms })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_text() {
        let q = Query::from_str("heap exploitation").unwrap();
        assert_eq!(q.terms.len(), 2);
        assert_eq!(q.terms[0], QueryTerm::Text("heap".into()));
        assert_eq!(q.terms[1], QueryTerm::Text("exploitation".into()));
    }

    #[test]
    fn parses_quoted_phrases() {
        let q = Query::from_str("title:\"memory management\"").unwrap();
        assert_eq!(q.terms.len(), 1);
        assert_eq!(q.terms[0], QueryTerm::Title("memory management".into()));
    }

    #[test]
    fn parses_negative_filters() {
        let q = Query::from_str("tag:security -tag:web heap").unwrap();
        assert_eq!(q.terms.len(), 3);
        assert_eq!(q.terms[0], QueryTerm::Tag("security".into()));
        assert_eq!(
            q.terms[1],
            QueryTerm::Not(Box::new(QueryTerm::Tag("web".into())))
        );
        assert_eq!(q.terms[2], QueryTerm::Text("heap".into()));
    }

    #[test]
    fn errors_on_unclosed_quotes() {
        let q = Query::from_str("title:\"unclosed");
        assert!(q.is_err());
    }
}
