//! A bounded parser for OpenUI Lang programs: one `name = expression` statement per line,
//! expressions are component calls, `@Action(...)` calls, references, `$state` bindings,
//! strings, numbers, booleans, `null`, arrays and objects. Parsing is data only: nothing is
//! evaluated, called or fetched.

use std::collections::BTreeMap;

use thiserror::Error;

/// Why a body cannot be rendered natively.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LangError {
    /// The body is over the byte limit.
    #[error("body larger than {0} bytes")]
    TooLarge(usize),
    /// A syntax error, with the line.
    #[error("syntax error on line {line}: {what}")]
    Syntax {
        /// 1-based line.
        line: usize,
        /// What was wrong.
        what: String,
    },
    /// No `root` statement.
    #[error("no root statement")]
    NoRoot,
    /// A reference to a name that is not defined.
    #[error("unknown reference {0}")]
    UnknownRef(String),
    /// A reference cycle.
    #[error("reference cycle through {0}")]
    Cycle(String),
    /// Deeper than the nesting bound.
    #[error("nesting deeper than {0}")]
    TooDeep(usize),
    /// More nodes than the bound.
    #[error("more than {0} nodes")]
    TooManyNodes(usize),
}

/// Bounds; violations fail closed to the fallback (ADR-0074 decision 6).
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    /// Body bytes.
    pub max_bytes: usize,
    /// Nesting depth after resolving references.
    pub max_depth: usize,
    /// Nodes after resolving references.
    pub max_nodes: usize,
}

impl Default for Bounds {
    fn default() -> Self {
        Self {
            max_bytes: oqto_session_view::parts::MAX_BODY_BYTES,
            max_depth: 24,
            max_nodes: 2000,
        }
    }
}

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A string.
    Str(String),
    /// A number.
    Num(f64),
    /// `true` or `false`.
    Bool(bool),
    /// `null`.
    Null,
    /// `[a, b]`.
    Array(Vec<Expr>),
    /// `{key: value}`.
    Object(Vec<(String, Expr)>),
    /// `Name(args)`.
    Call(String, Vec<Expr>),
    /// `@Name(args)`.
    Action(String, Vec<Expr>),
    /// A reference to another statement.
    Ref(String),
    /// `$name`: a binding to form state.
    State(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Num(f64),
    Punct(char),
    At,
    Dollar(String),
}

fn syntax(line: usize, what: impl Into<String>) -> LangError {
    LangError::Syntax {
        line,
        what: what.into(),
    }
}

fn lex(line: &str, number: usize) -> Result<Vec<Token>, LangError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\r' => i += 1,
            '(' | ')' | '[' | ']' | '{' | '}' | ',' | ':' | '=' => {
                tokens.push(Token::Punct(c));
                i += 1;
            }
            '@' => {
                tokens.push(Token::At);
                i += 1;
            }
            '"' => {
                let (text, next) = string(&chars, i + 1, number)?;
                tokens.push(Token::Str(text));
                i = next;
            }
            '$' => {
                let start = i + 1;
                i = start;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                if i == start {
                    return Err(syntax(number, "empty $binding"));
                }
                tokens.push(Token::Dollar(chars[start..i].iter().collect()));
            }
            c if c == '-' || c.is_ascii_digit() => {
                let start = i;
                i += 1;
                while i < chars.len()
                    && (chars[i].is_ascii_digit()
                        || matches!(chars[i], '.' | 'e' | 'E' | '+' | '-'))
                {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let value = text
                    .parse::<f64>()
                    .map_err(|_| syntax(number, format!("bad number {text}")))?;
                tokens.push(Token::Num(value));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                tokens.push(Token::Ident(chars[start..i].iter().collect()));
            }
            other => return Err(syntax(number, format!("unexpected {other:?}"))),
        }
    }
    Ok(tokens)
}

fn string(chars: &[char], mut i: usize, line: usize) -> Result<(String, usize), LangError> {
    let mut out = String::new();
    while i < chars.len() {
        match chars[i] {
            '"' => return Ok((out, i + 1)),
            '\\' => {
                let escaped = chars
                    .get(i + 1)
                    .ok_or_else(|| syntax(line, "unfinished escape"))?;
                match escaped {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '"' | '\\' | '/' => out.push(*escaped),
                    'u' => {
                        let hex: String = chars
                            .get(i + 2..i + 6)
                            .ok_or_else(|| syntax(line, "short \\u"))?
                            .iter()
                            .collect();
                        let code =
                            u32::from_str_radix(&hex, 16).map_err(|_| syntax(line, "bad \\u"))?;
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        i += 4;
                    }
                    other => return Err(syntax(line, format!("unknown escape \\{other}"))),
                }
                i += 2;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Err(syntax(line, "unterminated string"))
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
    line: usize,
}

impl Parser {
    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn eat(&mut self, c: char) -> Result<(), LangError> {
        match self.next() {
            Some(Token::Punct(p)) if p == c => Ok(()),
            other => Err(syntax(
                self.line,
                format!("expected {c:?}, found {other:?}"),
            )),
        }
    }

    fn list(&mut self, close: char) -> Result<Vec<Expr>, LangError> {
        let mut items = Vec::new();
        if self.peek() == Some(&Token::Punct(close)) {
            self.at += 1;
            return Ok(items);
        }
        loop {
            items.push(self.expr()?);
            match self.next() {
                Some(Token::Punct(',')) => {
                    if self.peek() == Some(&Token::Punct(close)) {
                        self.at += 1;
                        return Ok(items);
                    }
                }
                Some(Token::Punct(p)) if p == close => return Ok(items),
                other => {
                    return Err(syntax(
                        self.line,
                        format!("expected , or {close:?}, found {other:?}"),
                    ));
                }
            }
        }
    }

    fn object(&mut self) -> Result<Vec<(String, Expr)>, LangError> {
        let mut pairs = Vec::new();
        if self.peek() == Some(&Token::Punct('}')) {
            self.at += 1;
            return Ok(pairs);
        }
        loop {
            let key = match self.next() {
                Some(Token::Ident(k) | Token::Str(k)) => k,
                other => {
                    return Err(syntax(
                        self.line,
                        format!("expected a key, found {other:?}"),
                    ));
                }
            };
            self.eat(':')?;
            pairs.push((key, self.expr()?));
            match self.next() {
                Some(Token::Punct(',')) => {
                    if self.peek() == Some(&Token::Punct('}')) {
                        self.at += 1;
                        return Ok(pairs);
                    }
                }
                Some(Token::Punct('}')) => return Ok(pairs),
                other => {
                    return Err(syntax(
                        self.line,
                        format!("expected , or }}, found {other:?}"),
                    ));
                }
            }
        }
    }

    fn expr(&mut self) -> Result<Expr, LangError> {
        match self.next() {
            Some(Token::Str(s)) => Ok(Expr::Str(s)),
            Some(Token::Num(n)) => Ok(Expr::Num(n)),
            Some(Token::Dollar(name)) => Ok(Expr::State(name)),
            Some(Token::Punct('[')) => Ok(Expr::Array(self.list(']')?)),
            Some(Token::Punct('{')) => Ok(Expr::Object(self.object()?)),
            Some(Token::At) => match self.next() {
                Some(Token::Ident(name)) => {
                    self.eat('(')?;
                    Ok(Expr::Action(name, self.list(')')?))
                }
                other => Err(syntax(
                    self.line,
                    format!("expected an action name, found {other:?}"),
                )),
            },
            Some(Token::Ident(name)) => match name.as_str() {
                "true" => Ok(Expr::Bool(true)),
                "false" => Ok(Expr::Bool(false)),
                "null" => Ok(Expr::Null),
                _ if self.peek() == Some(&Token::Punct('(')) => {
                    self.at += 1;
                    Ok(Expr::Call(name, self.list(')')?))
                }
                _ => Ok(Expr::Ref(name)),
            },
            other => Err(syntax(self.line, format!("unexpected {other:?}"))),
        }
    }
}

/// A parsed program: statements by name.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// `name = expression`.
    pub statements: BTreeMap<String, Expr>,
}

/// Parse a body within `bounds.max_bytes`.
pub fn parse(body: &str, bounds: &Bounds) -> Result<Program, LangError> {
    if body.len() > bounds.max_bytes {
        return Err(LangError::TooLarge(bounds.max_bytes));
    }
    let mut statements = BTreeMap::new();
    for (index, line) in body.lines().enumerate() {
        let number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        let tokens = lex(trimmed, number)?;
        let mut parser = Parser {
            tokens,
            at: 0,
            line: number,
        };
        let name = match parser.next() {
            Some(Token::Ident(name)) => name,
            other => return Err(syntax(number, format!("expected a name, found {other:?}"))),
        };
        parser.eat('=')?;
        let expr = parser.expr()?;
        if parser.at != parser.tokens.len() {
            return Err(syntax(number, "trailing tokens"));
        }
        statements.insert(name, expr);
    }
    Ok(Program { statements })
}

impl Program {
    /// Resolve references from `root` into one tree, within the depth and node bounds.
    pub fn resolve(&self, bounds: &Bounds) -> Result<Expr, LangError> {
        let root = self.statements.get("root").ok_or(LangError::NoRoot)?;
        let mut nodes = 0;
        let mut path = vec!["root".to_owned()];
        self.inline(root, 0, &mut nodes, &mut path, bounds)
    }

    fn inline(
        &self,
        expr: &Expr,
        depth: usize,
        nodes: &mut usize,
        path: &mut Vec<String>,
        bounds: &Bounds,
    ) -> Result<Expr, LangError> {
        if depth > bounds.max_depth {
            return Err(LangError::TooDeep(bounds.max_depth));
        }
        *nodes += 1;
        if *nodes > bounds.max_nodes {
            return Err(LangError::TooManyNodes(bounds.max_nodes));
        }
        let each = |items: &[Expr],
                    path: &mut Vec<String>,
                    nodes: &mut usize|
         -> Result<Vec<Expr>, LangError> {
            items
                .iter()
                .map(|item| self.inline(item, depth + 1, nodes, path, bounds))
                .collect()
        };
        Ok(match expr {
            Expr::Ref(name) => {
                if path.contains(name) {
                    return Err(LangError::Cycle(name.clone()));
                }
                let target = self
                    .statements
                    .get(name)
                    .ok_or_else(|| LangError::UnknownRef(name.clone()))?;
                path.push(name.clone());
                let resolved = self.inline(target, depth + 1, nodes, path, bounds)?;
                path.pop();
                resolved
            }
            Expr::Array(items) => Expr::Array(each(items, path, nodes)?),
            Expr::Call(name, args) => Expr::Call(name.clone(), each(args, path, nodes)?),
            Expr::Action(name, args) => Expr::Action(name.clone(), each(args, path, nodes)?),
            Expr::Object(pairs) => {
                let values: Vec<Expr> = pairs.iter().map(|(_, v)| v.clone()).collect();
                let resolved = each(&values, path, nodes)?;
                Expr::Object(pairs.iter().map(|(k, _)| k.clone()).zip(resolved).collect())
            }
            other => other.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_calls_actions_objects_and_bindings() -> Result<(), LangError> {
        let body = "root = Stack([a, b])\na = Button(\"Go \\\"now\\\"\", @Answer({x: $x, n: -1.5, ok: true}), \"primary\")\nb = TextContent(\"hi\", null,)";
        let tree = parse(body, &Bounds::default())?.resolve(&Bounds::default())?;
        let Expr::Call(name, args) = tree else {
            return Err(LangError::NoRoot);
        };
        assert_eq!(name, "Stack");
        let Expr::Array(children) = &args[0] else {
            return Err(LangError::NoRoot);
        };
        assert_eq!(
            children[0],
            Expr::Call(
                "Button".into(),
                vec![
                    Expr::Str("Go \"now\"".into()),
                    Expr::Action(
                        "Answer".into(),
                        vec![Expr::Object(vec![
                            ("x".into(), Expr::State("x".into())),
                            ("n".into(), Expr::Num(-1.5)),
                            ("ok".into(), Expr::Bool(true))
                        ])]
                    ),
                    Expr::Str("primary".into()),
                ]
            )
        );
        Ok(())
    }

    #[test]
    fn bounds_and_errors_fail_closed() {
        let b = Bounds::default();
        assert!(matches!(
            parse("root = Stack([x])", &b).and_then(|p| p.resolve(&b)),
            Err(LangError::UnknownRef(_))
        ));
        assert!(matches!(
            parse("root = a\na = b\nb = a", &b).and_then(|p| p.resolve(&b)),
            Err(LangError::Cycle(_))
        ));
        assert!(matches!(
            parse("x = Stack([])", &b).and_then(|p| p.resolve(&b)),
            Err(LangError::NoRoot)
        ));
        assert!(matches!(
            parse("root = Stack([\"open", &b),
            Err(LangError::Syntax { .. })
        ));
        let deep = format!("root = {}{}", "Stack([".repeat(40), "])".repeat(40));
        assert!(matches!(
            parse(&deep, &b).and_then(|p| p.resolve(&b)),
            Err(LangError::TooDeep(_))
        ));
        let wide =
            format!("root = Stack([{}])", vec!["a"; 3000].join(", ")) + "\na = TextContent(\"x\")";
        assert!(matches!(
            parse(&wide, &b).and_then(|p| p.resolve(&b)),
            Err(LangError::TooManyNodes(_))
        ));
        let big = "x".repeat(b.max_bytes + 1);
        assert!(matches!(parse(&big, &b), Err(LangError::TooLarge(_))));
    }
}
