//! Gmail-syntax search parser & SQLite/FTS5 query compiler.
//!
//! Supports:
//! - Operators: `from:`, `to:`, `subject:`, `in:`, `is:`, `has:`, `label:`,
//!   `before:`, `after:`, `older_than:`, `newer_than:`, `larger:`, `smaller:`, `filename:`
//! - Boolean logic: implicit `AND`, explicit `OR`, parentheses `(...)`, `-negation`, `"quoted phrases"`
//! - Drives accent-folded SQLite FTS5 (`message_fts`) with relaxed `OR` / `LIKE` fallback.

use chrono::{NaiveDate, TimeZone, Utc};
use rusqlite::types::Value;
use rusqlite::{params, Connection};

use crate::error::CoreError;
use crate::ids::UnixMs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchExpr {
    And(Vec<SearchExpr>),
    Or(Vec<SearchExpr>),
    Not(Box<SearchExpr>),
    Term(String),
    From(String),
    To(String),
    Subject(String),
    InFolder(String),
    Is(String),
    Has(String),
    Label(String),
    Before(UnixMs),
    After(UnixMs),
    Larger(u64),
    Smaller(u64),
    Filename(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    LParen,
    RParen,
    Or,
    Minus,
    Word(String),
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch == '(' {
            tokens.push(Token::LParen);
            i += 1;
            continue;
        }
        if ch == ')' {
            tokens.push(Token::RParen);
            i += 1;
            continue;
        }
        if ch == '-' && i + 1 < chars.len() && !chars[i + 1].is_whitespace() {
            tokens.push(Token::Minus);
            i += 1;
            continue;
        }

        // Read a word or operator:value (preserving quoted phrases inside or standalone).
        let mut buf = String::new();
        while i < chars.len() {
            let c = chars[i];
            if c == '"' {
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    buf.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() && chars[i] == '"' {
                    i += 1;
                }
                continue;
            }
            if c.is_whitespace() || c == '(' || c == ')' {
                break;
            }
            buf.push(c);
            i += 1;
        }

        if buf == "OR" {
            tokens.push(Token::Or);
        } else if !buf.is_empty() {
            tokens.push(Token::Word(buf));
        }
    }
    tokens
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    now: UnixMs,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    fn parse_or(&mut self) -> Option<SearchExpr> {
        let mut branches = vec![self.parse_and()?];
        while matches!(self.peek(), Some(Token::Or)) {
            self.next();
            if let Some(rhs) = self.parse_and() {
                branches.push(rhs);
            }
        }
        if branches.len() == 1 {
            branches.pop()
        } else {
            Some(SearchExpr::Or(branches))
        }
    }

    fn parse_and(&mut self) -> Option<SearchExpr> {
        let mut items = Vec::new();
        while let Some(tok) = self.peek() {
            if matches!(tok, Token::RParen | Token::Or) {
                break;
            }
            if let Some(atom) = self.parse_unary() {
                items.push(atom);
            } else {
                break;
            }
        }
        if items.is_empty() {
            None
        } else if items.len() == 1 {
            items.pop()
        } else {
            Some(SearchExpr::And(items))
        }
    }

    fn parse_unary(&mut self) -> Option<SearchExpr> {
        if matches!(self.peek(), Some(Token::Minus)) {
            self.next();
            let inner = self.parse_unary()?;
            return Some(SearchExpr::Not(Box::new(inner)));
        }
        if matches!(self.peek(), Some(Token::LParen)) {
            self.next();
            let inner = self.parse_or();
            if matches!(self.peek(), Some(Token::RParen)) {
                self.next();
            }
            return inner;
        }
        match self.next()? {
            Token::Word(raw) => Some(self.parse_atom(&raw)),
            _ => None,
        }
    }

    fn parse_atom(&self, raw: &str) -> SearchExpr {
        if let Some((op, val)) = raw.split_once(':') {
            let val = val.trim();
            if !val.is_empty() {
                match op.to_ascii_lowercase().as_str() {
                    "from" => return SearchExpr::From(val.to_lowercase()),
                    "to" => return SearchExpr::To(val.to_lowercase()),
                    "subject" => return SearchExpr::Subject(val.to_lowercase()),
                    "in" => return SearchExpr::InFolder(val.to_lowercase()),
                    "is" => return SearchExpr::Is(val.to_lowercase()),
                    "has" => return SearchExpr::Has(val.to_lowercase()),
                    "label" => return SearchExpr::Label(val.to_lowercase()),
                    "filename" => return SearchExpr::Filename(val.to_lowercase()),
                    "before" => {
                        if let Some(ts) = parse_date_ms(val, false) {
                            return SearchExpr::Before(ts);
                        }
                    }
                    "after" => {
                        if let Some(ts) = parse_date_ms(val, true) {
                            return SearchExpr::After(ts);
                        }
                    }
                    "older_than" => {
                        if let Some(secs) = parse_relative_duration_secs(val) {
                            return SearchExpr::Before(UnixMs(self.now.0.saturating_sub(secs * 1000)));
                        }
                    }
                    "newer_than" => {
                        if let Some(secs) = parse_relative_duration_secs(val) {
                            return SearchExpr::After(UnixMs(self.now.0.saturating_sub(secs * 1000)));
                        }
                    }
                    "larger" => {
                        if let Some(bytes) = parse_size_bytes(val) {
                            return SearchExpr::Larger(bytes);
                        }
                    }
                    "smaller" => {
                        if let Some(bytes) = parse_size_bytes(val) {
                            return SearchExpr::Smaller(bytes);
                        }
                    }
                    _ => {}
                }
            }
        }
        SearchExpr::Term(raw.to_string())
    }
}

/// Parse a Gmail-syntax search query into a [`SearchExpr`] AST.
pub fn parse_search_query(query: &str, now: UnixMs) -> Option<SearchExpr> {
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return None;
    }
    let mut parser = Parser {
        tokens,
        pos: 0,
        now,
    };
    parser.parse_or()
}

fn parse_date_ms(raw: &str, end_of_day: bool) -> Option<UnixMs> {
    let normalized = raw.replace('/', "-");
    let date = NaiveDate::parse_from_str(&normalized, "%Y-%m-%d").ok()?;
    let dt = if end_of_day {
        date.and_hms_opt(23, 59, 59)?
    } else {
        date.and_hms_opt(0, 0, 0)?
    };
    Some(UnixMs(Utc.from_utc_datetime(&dt).timestamp_millis()))
}

fn parse_relative_duration_secs(raw: &str) -> Option<i64> {
    let lower = raw.to_ascii_lowercase();
    let unit = lower.chars().last()?;
    let num: i64 = lower[..lower.len().saturating_sub(1)].parse().ok()?;
    if num <= 0 {
        return None;
    }
    let mult = match unit {
        'd' => 86_400,
        'w' => 7 * 86_400,
        'm' => 30 * 86_400,
        'y' => 365 * 86_400,
        _ => return None,
    };
    Some(num.saturating_mul(mult))
}

fn parse_size_bytes(raw: &str) -> Option<u64> {
    let upper = raw.to_ascii_uppercase();
    if let Some(prefix) = upper.strip_suffix('M') {
        prefix.parse::<u64>().ok().map(|n| n.saturating_mul(1_048_576))
    } else if let Some(prefix) = upper.strip_suffix('K') {
        prefix.parse::<u64>().ok().map(|n| n.saturating_mul(1_024))
    } else {
        upper.parse::<u64>().ok()
    }
}

/// Compile a parsed [`SearchExpr`] into a parameterized SQLite `WHERE` fragment on `thread t`.
pub(crate) fn compile_search_sql(
    conn: &mut Connection,
    expr: &SearchExpr,
) -> Result<(String, Vec<Value>), CoreError> {
    match expr {
        SearchExpr::And(items) => {
            let mut parts = Vec::with_capacity(items.len());
            let mut values = Vec::new();
            for item in items {
                let (sql, vals) = compile_search_sql(conn, item)?;
                parts.push(format!("({sql})"));
                values.extend(vals);
            }
            Ok((parts.join(" AND "), values))
        }
        SearchExpr::Or(items) => {
            let mut parts = Vec::with_capacity(items.len());
            let mut values = Vec::new();
            for item in items {
                let (sql, vals) = compile_search_sql(conn, item)?;
                parts.push(format!("({sql})"));
                values.extend(vals);
            }
            Ok((parts.join(" OR "), values))
        }
        SearchExpr::Not(inner) => {
            let (sql, vals) = compile_search_sql(conn, inner)?;
            Ok((format!("NOT ({sql})"), vals))
        }
        SearchExpr::Term(raw) => compile_term_sql(conn, raw),
        SearchExpr::From(val) => {
            let pat = format!("%{val}%");
            Ok((
                "(lower(t.sender_email) LIKE ? OR lower(t.sender_name) LIKE ? OR EXISTS (
                    SELECT 1 FROM message m WHERE m.thread_id = t.id
                      AND (lower(m.from_email) LIKE ? OR lower(COALESCE(m.from_name, '')) LIKE ?)
                ))"
                .to_string(),
                vec![
                    Value::Text(pat.clone()),
                    Value::Text(pat.clone()),
                    Value::Text(pat.clone()),
                    Value::Text(pat),
                ],
            ))
        }
        SearchExpr::To(val) => {
            let pat = format!("%{val}%");
            Ok((
                "EXISTS (SELECT 1 FROM message m WHERE m.thread_id = t.id
                   AND (lower(m.to_json) LIKE ? OR lower(m.cc_json) LIKE ?))"
                    .to_string(),
                vec![Value::Text(pat.clone()), Value::Text(pat)],
            ))
        }
        SearchExpr::Subject(val) => {
            let pat = format!("%{val}%");
            Ok((
                "(lower(t.subject) LIKE ? OR EXISTS (SELECT 1 FROM message m WHERE m.thread_id = t.id AND lower(m.subject) LIKE ?))"
                    .to_string(),
                vec![Value::Text(pat.clone()), Value::Text(pat)],
            ))
        }
        SearchExpr::InFolder(folder) => {
            let clause = match folder.as_str() {
                "inbox" => "t.archived = 0 AND t.trashed = 0 AND t.spam = 0",
                "archive" => "t.archived = 1 AND t.trashed = 0 AND t.spam = 0",
                "trash" => "t.trashed = 1",
                "spam" | "junk" => "t.spam = 1 AND t.trashed = 0",
                "starred" => "t.starred = 1 AND t.trashed = 0",
                "sent" => {
                    "EXISTS (SELECT 1 FROM message m WHERE m.thread_id = t.id
                       AND lower(m.from_email) IN (SELECT lower(address) FROM identity))"
                }
                "snoozed" => {
                    "EXISTS (SELECT 1 FROM job j WHERE j.thread_id = t.id AND j.kind = 'snooze'
                       AND j.live = 1 AND json_extract(j.state_json, '$.kind') = 'armed')"
                }
                "drafts" => {
                    "EXISTS (SELECT 1 FROM draft d WHERE d.reply_to_thread = t.id
                       AND json_extract(d.state_json, '$.kind') IN ('editing','pendingApproval','queued'))"
                }
                "anywhere" => "1 = 1",
                _ => "1 = 1",
            };
            Ok((clause.to_string(), Vec::new()))
        }
        SearchExpr::Is(flag) => {
            let clause = match flag.as_str() {
                "unread" => "t.unread_count > 0",
                "read" => "t.unread_count = 0",
                "starred" => "t.starred = 1",
                "vip" => "lower(t.sender_email) IN (SELECT lower(address) FROM vip_sender)",
                _ => "1 = 1",
            };
            Ok((clause.to_string(), Vec::new()))
        }
        SearchExpr::Has(what) => {
            let clause = match what.as_str() {
                "attachment" | "attachments" => "t.has_attachments = 1",
                "invite" => {
                    "EXISTS (SELECT 1 FROM message m JOIN message_invite mi ON mi.message_id = m.id
                       WHERE m.thread_id = t.id)"
                }
                _ => "1 = 1",
            };
            Ok((clause.to_string(), Vec::new()))
        }
        SearchExpr::Label(name) => Ok((
            "EXISTS (SELECT 1 FROM thread_label tl JOIN label l ON l.id = tl.label_id
               WHERE tl.thread_id = t.id AND lower(l.name) = ?)"
                .to_string(),
            vec![Value::Text(name.clone())],
        )),
        SearchExpr::Before(ts) => Ok(("t.last_at < ?".to_string(), vec![Value::Integer(ts.0)])),
        SearchExpr::After(ts) => Ok(("t.last_at > ?".to_string(), vec![Value::Integer(ts.0)])),
        SearchExpr::Larger(bytes) => Ok((
            "EXISTS (SELECT 1 FROM message m WHERE m.thread_id = t.id
               AND (length(m.body_text) + COALESCE(length(m.raw_html), 0)
                    + COALESCE((SELECT SUM(a.size) FROM attachment a WHERE a.message_id = m.id), 0)) >= ?)"
                .to_string(),
            vec![Value::Integer(*bytes as i64)],
        )),
        SearchExpr::Smaller(bytes) => Ok((
            "EXISTS (SELECT 1 FROM message m WHERE m.thread_id = t.id
               AND (length(m.body_text) + COALESCE(length(m.raw_html), 0)
                    + COALESCE((SELECT SUM(a.size) FROM attachment a WHERE a.message_id = m.id), 0)) <= ?)"
                .to_string(),
            vec![Value::Integer(*bytes as i64)],
        )),
        SearchExpr::Filename(name) => {
            let pat = format!("%{name}%");
            Ok((
                "EXISTS (SELECT 1 FROM message m JOIN attachment a ON a.message_id = m.id
                   WHERE m.thread_id = t.id AND lower(a.filename) LIKE ?)"
                    .to_string(),
                vec![Value::Text(pat)],
            ))
        }
    }
}

/// Compile a free-text term: accent-folded FTS5 MATCH first (AND, then relaxed OR if
/// multi-word), falling back to substring LIKE for very short/partial tokens.
fn compile_term_sql(conn: &mut Connection, raw: &str) -> Result<(String, Vec<Value>), CoreError> {
    let tokens: Vec<String> = raw
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .take(12)
        .map(|t| t.to_lowercase())
        .collect();
    if tokens.is_empty() {
        return Ok(("0 = 1".to_string(), Vec::new()));
    }

    let fts_and = tokens
        .iter()
        .map(|t| format!("\"{t}\"*"))
        .collect::<Vec<_>>()
        .join(" AND ");

    let fts_hit: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM message m WHERE m.rowid IN (
                 SELECT rowid FROM message_fts WHERE message_fts MATCH ?1))",
            params![fts_and],
            |r| r.get(0),
        )
        .unwrap_or(false);

    if fts_hit {
        return Ok((
            "t.id IN (SELECT m.thread_id FROM message m WHERE m.rowid IN (
                 SELECT rowid FROM message_fts WHERE message_fts MATCH ?))"
                .to_string(),
            vec![Value::Text(fts_and)],
        ));
    }

    // Partial / short tokens: substring match across thread and message text.
    let mut conds = Vec::new();
    let mut values = Vec::new();
    for token in &tokens {
        conds.push(
            "(lower(m.subject) LIKE ? OR lower(m.from_email) LIKE ? OR lower(m.body_text) LIKE ?
              OR lower(t.sender_name) LIKE ? OR lower(t.sender_email) LIKE ?)",
        );
        let pattern = Value::Text(format!("%{token}%"));
        for _ in 0..5 {
            values.push(pattern.clone());
        }
    }
    Ok((
        format!(
            "t.id IN (SELECT m.thread_id FROM message m WHERE {})",
            conds.join(" AND ")
        ),
        values,
    ))
}

/// Whether the query explicitly includes `in:trash`, `in:spam`, or `in:anywhere` so
/// the caller shouldn't filter out trashed/spam threads by default.
pub(crate) fn includes_trashed_or_spam(expr: &SearchExpr) -> bool {
    match expr {
        SearchExpr::InFolder(f) => matches!(f.as_str(), "trash" | "spam" | "junk" | "anywhere"),
        SearchExpr::And(items) | SearchExpr::Or(items) => {
            items.iter().any(includes_trashed_or_spam)
        }
        SearchExpr::Not(inner) => includes_trashed_or_spam(inner),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: UnixMs = UnixMs(1_760_000_000_000);

    #[test]
    fn parses_gmail_operators_booleans_parens_and_negation() {
        let ast = parse_search_query(
            r#"(from:dana@ferngate.co OR from:billing@figma.com) has:attachment -is:read filename:invoice.pdf"#,
            NOW,
        )
        .expect("parse");

        match ast {
            SearchExpr::And(parts) => {
                assert_eq!(parts.len(), 4);
                assert!(matches!(&parts[0], SearchExpr::Or(b) if b.len() == 2));
                assert_eq!(parts[1], SearchExpr::Has("attachment".into()));
                assert_eq!(
                    parts[2],
                    SearchExpr::Not(Box::new(SearchExpr::Is("read".into())))
                );
                assert_eq!(parts[3], SearchExpr::Filename("invoice.pdf".into()));
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn parses_dates_relative_durations_and_sizes() {
        let ast = parse_search_query(
            "after:2025-01-01 before:2026-01-01 older_than:7d newer_than:2w larger:2M smaller:500K",
            NOW,
        )
        .expect("parse");
        match ast {
            SearchExpr::And(parts) => {
                assert_eq!(parts.len(), 6);
                assert!(matches!(parts[0], SearchExpr::After(_)));
                assert!(matches!(parts[1], SearchExpr::Before(_)));
                assert!(matches!(parts[2], SearchExpr::Before(_)));
                assert!(matches!(parts[3], SearchExpr::After(_)));
                assert_eq!(parts[4], SearchExpr::Larger(2 * 1_048_576));
                assert_eq!(parts[5], SearchExpr::Smaller(500 * 1_024));
            }
            other => panic!("expected And, got {other:?}"),
        }
    }
}
