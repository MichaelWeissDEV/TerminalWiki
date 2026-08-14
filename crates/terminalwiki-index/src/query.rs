use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub terms: Vec<QueryTerm>,
}

impl FromStr for Query {
    type Err = ();

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

            let term = if let Some(idx) = token.find(':') {
                let key = &token[..idx];
                let value = &token[idx+1..];
                // strip quotes from value if present
                let value = if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                    &value[1..value.len()-1]
                } else {
                    value
                };

                match key {
                    "tag" => QueryTerm::Tag(value.to_string()),
                    "wiki" => QueryTerm::Wiki(value.to_string()),
                    "type" => QueryTerm::Type(value.to_string()),
                    "ext" => QueryTerm::Ext(value.to_string()),
                    "path" => QueryTerm::Path(value.to_string()),
                    "title" => QueryTerm::Title(value.to_string()),
                    "linksto" => QueryTerm::LinksTo(value.to_string()),
                    "backlink" => QueryTerm::Backlink(value.to_string()),
                    _ => QueryTerm::Text(token.clone()), // Fallback
                }
            } else {
                let val = if token.starts_with('"') && token.ends_with('"') && token.len() >= 2 {
                    &token[1..token.len()-1]
                } else {
                    &token
                };
                QueryTerm::Text(val.to_string())
            };

            let term = if is_not {
                QueryTerm::Not(Box::new(term))
            } else {
                term
            };

            terms.push(term);
        }

        Ok(Query { terms })
    }
}
