//! Tolerant PostgreSQL lexer for the editor's language service.
//!
//! Unlike the read-only policy lexer (`pg::policy`), this one never stops at the first problem:
//! the editor needs tokens for half-typed SQL. Problems such as an unterminated string are
//! recorded in `errors` and the token simply runs to the end of the text.
//!
//! Positions are character (code point) indices. [`Lexed::to_utf16`] converts them into the
//! UTF-16 offsets the editor uses, so non-ASCII text lines up exactly.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Unquoted identifier or keyword.
    Word,
    QuotedIdent,
    /// Any string literal: '…', E'…', B'…', X'…', $tag$…$tag$.
    String,
    Number,
    /// An Intalix variable: `{{ name }}` or `{{ name.part }}`.
    Variable,
    /// A positional parameter such as `$1`.
    Param,
    /// `::`
    Cast,
    Operator,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Semicolon,
    Dot,
    Colon,
    LineComment,
    BlockComment,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    /// First character (inclusive).
    pub start: usize,
    /// One past the last character.
    pub end: usize,
}

impl Token {
    pub fn is_trivia(&self) -> bool {
        matches!(self.kind, Kind::LineComment | Kind::BlockComment)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub start: usize,
    pub end: usize,
    pub message: &'static str,
}

#[derive(Debug)]
pub struct Lexed {
    pub chars: Vec<char>,
    pub tokens: Vec<Token>,
    pub errors: Vec<LexError>,
    /// `utf16[i]` is the UTF-16 offset of character `i`; one extra entry for the end.
    utf16: Vec<usize>,
}

/// A parsed `{{ name.part }}` reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableRef {
    /// Lower-cased variable name.
    pub name: String,
    /// Lower-cased part after the dot, e.g. `from` in `{{period.from}}`.
    pub part: Option<String>,
}

const OPERATOR_CHARS: &str = "+-*/<>=~!@#%^&|`?";

impl Lexed {
    pub fn text(&self, token: &Token) -> String {
        self.chars[token.start..token.end].iter().collect()
    }

    /// Lower-cased text of a `Word` token; `None` for any other kind.
    pub fn word(&self, token: &Token) -> Option<String> {
        (token.kind == Kind::Word).then(|| self.text(token).to_lowercase())
    }

    /// The identifier a token names: unquoted words are folded to lower case (as PostgreSQL
    /// does) and quoted identifiers are unquoted.
    pub fn identifier(&self, token: &Token) -> Option<String> {
        match token.kind {
            Kind::Word => Some(self.text(token).to_lowercase()),
            Kind::QuotedIdent => {
                let text = self.text(token);
                let inner = text.strip_prefix('"')?;
                let inner = inner.strip_suffix('"').unwrap_or(inner);
                Some(inner.replace("\"\"", "\""))
            }
            _ => None,
        }
    }

    /// Parses a `Variable` token.
    pub fn variable(&self, token: &Token) -> Option<VariableRef> {
        if token.kind != Kind::Variable {
            return None;
        }
        parse_variable(&self.text(token))
    }

    pub fn to_utf16(&self, char_index: usize) -> usize {
        let i = char_index.min(self.chars.len());
        self.utf16.get(i).copied().unwrap_or(0)
    }

    /// Character index for a UTF-16 offset (an offset inside a surrogate pair rounds down).
    pub fn char_index(&self, utf16_offset: usize) -> usize {
        match self.utf16.binary_search(&utf16_offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }

    /// Tokens that are not comments.
    pub fn significant(&self) -> Vec<Token> {
        self.tokens
            .iter()
            .filter(|t| !t.is_trivia())
            .copied()
            .collect()
    }
}

/// Parses the text of a variable token, e.g. `{{ Period.From }}`.
///
/// # Examples
/// - `"{{region}}"` → name `region`, no part
/// - `"{{ Period.From }}"` → name `period`, part `from`
/// - `"{{}}"` → `None`
pub fn parse_variable(text: &str) -> Option<VariableRef> {
    let inner = text.strip_prefix("{{")?.strip_suffix("}}")?.trim();
    let (name, part) = match inner.split_once('.') {
        Some((n, p)) => (n, Some(p.to_lowercase())),
        None => (inner, None),
    };
    if name.is_empty() {
        return None;
    }
    Some(VariableRef {
        name: name.to_lowercase(),
        part,
    })
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Splits SQL into tokens.
///
/// # Examples
/// - `"SELECT a::int FROM t"` → `Word Word Cast Word Word Word`
/// - `"WHERE d >= {{start}}"` → `Word Word Operator Variable`
/// - `"SELECT 'oops"` → `Word String` plus an "Unterminated string" error
pub fn lex(sql: &str) -> Lexed {
    let chars: Vec<char> = sql.chars().collect();
    let mut utf16 = Vec::with_capacity(chars.len() + 1);
    let mut offset = 0usize;
    for c in &chars {
        utf16.push(offset);
        offset += c.len_utf16();
    }
    utf16.push(offset);

    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let at = |i: usize| chars.get(i).copied();
    let mut i = 0usize;

    while let Some(c) = at(i) {
        let start = i;
        let mut push = |kind: Kind, end: usize| tokens.push(Token { kind, start, end });

        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // -- line comment
        if c == '-' && at(i + 1) == Some('-') {
            while at(i).is_some_and(|ch| ch != '\n') {
                i += 1;
            }
            push(Kind::LineComment, i);
            continue;
        }
        // /* block comment */, nested as in PostgreSQL
        if c == '/' && at(i + 1) == Some('*') {
            let mut depth = 0usize;
            loop {
                match (at(i), at(i + 1)) {
                    (Some('/'), Some('*')) => {
                        depth += 1;
                        i += 2;
                    }
                    (Some('*'), Some('/')) => {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    }
                    (Some(_), _) => i += 1,
                    (None, _) => {
                        errors.push(LexError {
                            start,
                            end: start + 2,
                            message: "This /* comment is never closed.",
                        });
                        break;
                    }
                }
            }
            push(Kind::BlockComment, i);
            continue;
        }
        // Intalix variable {{ name }} / {{ name.part }}
        if c == '{' && at(i + 1) == Some('{') {
            let mut j = i + 2;
            while at(j).is_some_and(|ch| ch == ' ' || ch == '\t') {
                j += 1;
            }
            let name_start = j;
            if at(j).is_some_and(is_ident_start) {
                while at(j).is_some_and(|ch| ch.is_alphanumeric() || ch == '_') {
                    j += 1;
                }
                if at(j) == Some('.') && at(j + 1).is_some_and(is_ident_start) {
                    j += 1;
                    while at(j).is_some_and(|ch| ch.is_alphanumeric() || ch == '_') {
                        j += 1;
                    }
                }
            }
            let has_name = j > name_start;
            while at(j).is_some_and(|ch| ch == ' ' || ch == '\t') {
                j += 1;
            }
            if has_name && at(j) == Some('}') && at(j + 1) == Some('}') {
                i = j + 2;
                push(Kind::Variable, i);
            } else {
                errors.push(LexError {
                    start,
                    end: (j.max(start + 2)).min(chars.len()),
                    message: "This variable is not complete. Write it as {{name}}.",
                });
                i += 2;
                push(Kind::Other, i);
            }
            continue;
        }
        // E'…' escape string (backslash escapes)
        if (c == 'e' || c == 'E') && at(i + 1) == Some('\'') {
            i += 2;
            loop {
                match at(i) {
                    Some('\\') => i += 2,
                    Some('\'') if at(i + 1) == Some('\'') => i += 2,
                    Some('\'') => {
                        i += 1;
                        break;
                    }
                    Some(_) => i += 1,
                    None => {
                        errors.push(unterminated_string(start));
                        break;
                    }
                }
            }
            i = i.min(chars.len());
            push(Kind::String, i);
            continue;
        }
        // '…', B'…', X'…'
        let prefixed = matches!(c, 'b' | 'B' | 'x' | 'X') && at(i + 1) == Some('\'');
        if c == '\'' || prefixed {
            i += if prefixed { 2 } else { 1 };
            loop {
                match at(i) {
                    Some('\'') if at(i + 1) == Some('\'') => i += 2,
                    Some('\'') => {
                        i += 1;
                        break;
                    }
                    Some(_) => i += 1,
                    None => {
                        errors.push(unterminated_string(start));
                        break;
                    }
                }
            }
            push(Kind::String, i);
            continue;
        }
        // "quoted identifier"
        if c == '"' {
            i += 1;
            loop {
                match at(i) {
                    Some('"') if at(i + 1) == Some('"') => i += 2,
                    Some('"') => {
                        i += 1;
                        break;
                    }
                    Some(_) => i += 1,
                    None => {
                        errors.push(LexError {
                            start,
                            end: start + 1,
                            message: "This quoted name is never closed. Add the closing \".",
                        });
                        break;
                    }
                }
            }
            push(Kind::QuotedIdent, i);
            continue;
        }
        // $tag$ dollar quoting, or a $1 parameter
        if c == '$' {
            let mut j = i + 1;
            while at(j).is_some_and(|ch| ch.is_alphanumeric() || ch == '_') {
                j += 1;
            }
            let digits = j > i + 1 && chars[i + 1..j].iter().all(char::is_ascii_digit);
            if digits {
                i = j;
                push(Kind::Param, i);
                continue;
            }
            if at(j) == Some('$') {
                let tag: Vec<char> = chars[i..=j].to_vec();
                i = j + 1;
                loop {
                    if i + tag.len() > chars.len() {
                        errors.push(LexError {
                            start,
                            end: start + tag.len(),
                            message: "This $$-quoted text is never closed.",
                        });
                        i = chars.len();
                        break;
                    }
                    if chars[i..i + tag.len()] == tag[..] {
                        i += tag.len();
                        break;
                    }
                    i += 1;
                }
                push(Kind::String, i);
                continue;
            }
            i += 1;
            push(Kind::Other, i);
            continue;
        }
        if is_ident_start(c) {
            while at(i).is_some_and(is_ident_char) {
                i += 1;
            }
            push(Kind::Word, i);
            continue;
        }
        // Numbers, including .5 and 1e-3
        if c.is_ascii_digit() || (c == '.' && at(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            while let Some(ch) = at(i) {
                let exponent_sign = (ch == '+' || ch == '-')
                    && i > start
                    && matches!(chars[i - 1], 'e' | 'E')
                    && chars[start..i - 1]
                        .iter()
                        .all(|d| d.is_ascii_digit() || *d == '.');
                if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || exponent_sign {
                    // "1..5" is not a number followed by a range; stop before a second dot pair.
                    if ch == '.' && at(i + 1) == Some('.') {
                        break;
                    }
                    i += 1;
                } else {
                    break;
                }
            }
            push(Kind::Number, i);
            continue;
        }
        if c == ':' && at(i + 1) == Some(':') {
            i += 2;
            push(Kind::Cast, i);
            continue;
        }
        if OPERATOR_CHARS.contains(c) {
            while let Some(ch) = at(i) {
                let comment_starts =
                    (ch == '-' && at(i + 1) == Some('-')) || (ch == '/' && at(i + 1) == Some('*'));
                if OPERATOR_CHARS.contains(ch) && !(i > start && comment_starts) {
                    i += 1;
                } else {
                    break;
                }
            }
            push(Kind::Operator, i);
            continue;
        }
        let kind = match c {
            '(' => Kind::LParen,
            ')' => Kind::RParen,
            '[' => Kind::LBracket,
            ']' => Kind::RBracket,
            ',' => Kind::Comma,
            ';' => Kind::Semicolon,
            '.' => Kind::Dot,
            ':' => Kind::Colon,
            _ => Kind::Other,
        };
        i += 1;
        push(kind, i);
    }

    Lexed {
        chars,
        tokens,
        errors,
        utf16,
    }
}

fn unterminated_string(start: usize) -> LexError {
    LexError {
        start,
        end: start + 1,
        message: "This text value is never closed. Add the closing quote (').",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(sql: &str) -> Vec<Kind> {
        lex(sql).tokens.iter().map(|t| t.kind).collect()
    }

    #[test]
    fn splits_common_sql() {
        use Kind::*;
        assert_eq!(
            kinds("SELECT a::int, \"B\" FROM s.t WHERE x >= 1.5;"),
            vec![
                Word,
                Word,
                Cast,
                Word,
                Comma,
                QuotedIdent,
                Word,
                Word,
                Dot,
                Word,
                Word,
                Word,
                Operator,
                Number,
                Semicolon
            ]
        );
    }

    #[test]
    fn reads_variables() {
        let lexed = lex("WHERE d >= {{ Start_Date }} AND r = {{period.to}}");
        let vars: Vec<_> = lexed
            .tokens
            .iter()
            .filter_map(|t| lexed.variable(t))
            .collect();
        assert_eq!(
            vars,
            vec![
                VariableRef {
                    name: "start_date".into(),
                    part: None
                },
                VariableRef {
                    name: "period".into(),
                    part: Some("to".into())
                },
            ]
        );
        assert!(lexed.errors.is_empty());
    }

    #[test]
    fn incomplete_variables_are_errors_not_panics() {
        for sql in ["{{", "{{ x", "{{x}", "{{1}}", "{{}}"] {
            let lexed = lex(sql);
            assert_eq!(lexed.errors.len(), 1, "{sql}");
        }
    }

    #[test]
    fn literals_hide_keywords_and_variables() {
        use Kind::*;
        assert_eq!(kinds("'{{x}} -- no'"), vec![String]);
        assert_eq!(kinds("$$ {{x}} $$"), vec![String]);
        assert_eq!(kinds("E'it\\'s'"), vec![String]);
        assert_eq!(kinds("-- {{x}}\n1"), vec![LineComment, Number]);
        assert_eq!(kinds("/* a /* b */ c */ 1"), vec![BlockComment, Number]);
        assert_eq!(kinds("$1 + $tag$x$tag$"), vec![Param, Operator, String]);
    }

    #[test]
    fn unterminated_tokens_run_to_the_end() {
        for sql in ["SELECT 'abc", "SELECT \"abc", "/* abc", "$$ abc", "E'abc\\"] {
            let lexed = lex(sql);
            assert_eq!(lexed.errors.len(), 1, "{sql}");
            assert_eq!(lexed.tokens.last().map(|t| t.end), Some(lexed.chars.len()));
        }
    }

    #[test]
    fn operators_stop_before_comments() {
        use Kind::*;
        assert_eq!(kinds("a=-- c\nb"), vec![Word, Operator, LineComment, Word]);
        assert_eq!(kinds("1e-3"), vec![Number]);
        assert_eq!(kinds("a-1"), vec![Word, Operator, Number]);
    }

    #[test]
    fn utf16_offsets_count_surrogate_pairs() {
        let lexed = lex("'😀' x");
        let x = lexed.tokens[1];
        assert_eq!(x.start, 4);
        assert_eq!(lexed.to_utf16(x.start), 5);
        assert_eq!(lexed.char_index(5), 4);
    }

    #[test]
    fn identifiers_fold_and_unquote() {
        let lexed = lex("Orders \"My \"\"T\"\"\"");
        assert_eq!(
            lexed.identifier(&lexed.tokens[0]).as_deref(),
            Some("orders")
        );
        assert_eq!(
            lexed.identifier(&lexed.tokens[1]).as_deref(),
            Some("My \"T\"")
        );
    }
}
