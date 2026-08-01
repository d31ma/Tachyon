use crate::Failure;
use crate::failure::{diagnostic, source_span};
use serde_json::{Number, Value};
use std::collections::BTreeMap;
use std::str::FromStr;

const MAX_EXPRESSION_BYTES: usize = 1_024;
const MAX_EXPRESSION_DEPTH: usize = 32;
/// Largest argument list a call in a template expression may carry.
const MAX_CALL_ARGUMENTS: usize = 8;
const FORBIDDEN_PROPERTIES: &[&str] = &["__proto__", "constructor", "prototype"];

pub(crate) type Scope = BTreeMap<String, Value>;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Expression {
    source: String,
    source_path: String,
    source_start: usize,
    node: Expr,
}

impl Expression {
    pub(crate) fn parse(
        source: &str,
        source_path: &str,
        source_start: usize,
    ) -> Result<Self, Failure> {
        if source.is_empty() || source.len() > MAX_EXPRESSION_BYTES {
            return Err(expression_failure(
                source_path,
                source_start,
                source_start.saturating_add(source.len()),
                "Template expression must contain 1 through 1,024 bytes.",
            ));
        }
        let tokens = Lexer::new(source, source_path, source_start).tokenize()?;
        let mut parser = Parser {
            tokens: &tokens,
            current: 0,
            source_path,
            source_start,
            source_len: source.len(),
        };
        let node = parser.parse_ternary(0)?;
        if !matches!(parser.peek().kind, TokenKind::End) {
            return Err(parser.error("Template expression contains unexpected trailing syntax."));
        }
        Ok(Self {
            source: String::from(source),
            source_path: String::from(source_path),
            source_start,
            node,
        })
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn evaluate(&self, scope: &Scope) -> Result<Value, Failure> {
        evaluate(
            &self.node,
            scope,
            &self.source_path,
            self.source_start,
            self.source.len(),
        )
    }
}

impl Expression {
    /// Serialises the parsed expression for an island to evaluate.
    ///
    /// The client receives the bounded AST the compiler already produced, not
    /// source, so the runtime interprets a fixed shape and never parses or
    /// evaluates JavaScript.
    pub(crate) fn to_client_json(&self) -> String {
        client_json(&self.node)
    }
}

fn client_json(node: &Expr) -> String {
    let list = |nodes: &[Expr]| nodes.iter().map(client_json).collect::<Vec<_>>().join(",");
    match node {
        Expr::Literal(value) => format!(r#"{{"k":"lit","v":{value}}}"#),
        Expr::Identifier(name) => format!(r#"{{"k":"id","n":"{name}"}}"#),
        Expr::Access(parent, Access::Property(property)) => format!(
            r#"{{"k":"get","o":{},"p":"{property}"}}"#,
            client_json(parent)
        ),
        Expr::Access(parent, Access::Index(index)) => {
            format!(r#"{{"k":"idx","o":{},"i":{index}}}"#, client_json(parent))
        }
        Expr::Not(inner) => format!(r#"{{"k":"not","e":{}}}"#, client_json(inner)),
        Expr::Compare {
            left,
            operator,
            right,
        } => format!(
            r#"{{"k":"cmp","op":"{}","l":{},"r":{}}}"#,
            match operator {
                CompareOperator::Equal => "eq",
                CompareOperator::NotEqual => "ne",
                CompareOperator::Less => "lt",
                CompareOperator::LessEqual => "le",
                CompareOperator::Greater => "gt",
                CompareOperator::GreaterEqual => "ge",
            },
            client_json(left),
            client_json(right)
        ),
        Expr::Logical {
            left,
            operator,
            right,
        } => format!(
            r#"{{"k":"log","op":"{}","l":{},"r":{}}}"#,
            match operator {
                LogicalOperator::And => "and",
                LogicalOperator::Or => "or",
            },
            client_json(left),
            client_json(right)
        ),
        Expr::Arithmetic {
            left,
            operator,
            right,
        } => format!(
            r#"{{"k":"num","op":"{}","l":{},"r":{}}}"#,
            match operator {
                ArithmeticOperator::Add => "add",
                ArithmeticOperator::Subtract => "sub",
                ArithmeticOperator::Multiply => "mul",
                ArithmeticOperator::Divide => "div",
            },
            client_json(left),
            client_json(right)
        ),
        Expr::Conditional {
            condition,
            when_true,
            when_false,
        } => format!(
            r#"{{"k":"if","c":{},"t":{},"f":{}}}"#,
            client_json(condition),
            client_json(when_true),
            client_json(when_false)
        ),
        Expr::Await(inner) => format!(r#"{{"k":"await","e":{}}}"#, client_json(inner)),
        Expr::Call { callee, arguments } => format!(
            r#"{{"k":"call","c":{},"a":[{}]}}"#,
            client_json(callee),
            list(arguments)
        ),
    }
}

impl Expression {
    /// Builds `self == other`, used to desugar `<switch>` into a conditional
    /// chain.
    ///
    /// The node is built directly rather than by re-parsing concatenated
    /// source, so a switch value like `a || b` cannot change meaning through
    /// operator precedence.
    pub(crate) fn equals(&self, other: &Self) -> Self {
        Self {
            source: format!("{} == {}", self.source, other.source),
            source_path: self.source_path.clone(),
            source_start: self.source_start,
            node: Expr::Compare {
                left: Box::new(self.node.clone()),
                operator: CompareOperator::Equal,
                right: Box::new(other.node.clone()),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Literal(Value),
    Identifier(String),
    Access(Box<Self>, Access),
    Not(Box<Self>),
    Compare {
        left: Box<Self>,
        operator: CompareOperator,
        right: Box<Self>,
    },
    Logical {
        left: Box<Self>,
        operator: LogicalOperator,
        right: Box<Self>,
    },
    Arithmetic {
        left: Box<Self>,
        operator: ArithmeticOperator,
        right: Box<Self>,
    },
    Conditional {
        condition: Box<Self>,
        when_true: Box<Self>,
        when_false: Box<Self>,
    },
    /// A call, which only an island's companion instance can resolve.
    Call {
        callee: Box<Self>,
        arguments: Vec<Self>,
    },
    /// An awaited value, resolvable only where evaluation can be async.
    Await(Box<Self>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArithmeticOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Clone, Debug, PartialEq)]
enum Access {
    Property(String),
    Index(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompareOperator {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LogicalOperator {
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq)]
struct Token {
    kind: TokenKind,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, PartialEq)]
enum TokenKind {
    Identifier(String),
    Literal(Value),
    Bang,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    Dot,
    LeftBracket,
    RightBracket,
    LeftParen,
    RightParen,
    Question,
    Colon,
    Comma,
    Plus,
    Minus,
    Star,
    Slash,
    End,
}

struct Lexer<'a> {
    source: &'a str,
    source_path: &'a str,
    source_start: usize,
    current: usize,
}

impl<'a> Lexer<'a> {
    const fn new(source: &'a str, source_path: &'a str, source_start: usize) -> Self {
        Self {
            source,
            source_path,
            source_start,
            current: 0,
        }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, Failure> {
        let mut tokens = Vec::new();
        while self.current < self.source.len() {
            let byte = self.source.as_bytes()[self.current];
            if byte.is_ascii_whitespace() {
                self.current += 1;
                continue;
            }
            let start = self.current;
            let kind = match byte {
                b'!' if self.consume(b"!==") => TokenKind::NotEqual,
                b'!' if self.consume(b"!=") => TokenKind::NotEqual,
                b'!' => {
                    self.current += 1;
                    TokenKind::Bang
                }
                b'?' => {
                    self.current += 1;
                    TokenKind::Question
                }
                b',' => {
                    self.current += 1;
                    TokenKind::Comma
                }
                b':' => {
                    self.current += 1;
                    TokenKind::Colon
                }
                b'+' => {
                    self.current += 1;
                    TokenKind::Plus
                }
                b'-' => {
                    self.current += 1;
                    TokenKind::Minus
                }
                b'*' => {
                    self.current += 1;
                    TokenKind::Star
                }
                b'/' => {
                    self.current += 1;
                    TokenKind::Slash
                }
                b'=' if self.consume(b"===") => TokenKind::Equal,
                b'=' if self.consume(b"==") => TokenKind::Equal,
                b'<' if self.consume(b"<=") => TokenKind::LessEqual,
                b'<' => {
                    self.current += 1;
                    TokenKind::Less
                }
                b'>' if self.consume(b">=") => TokenKind::GreaterEqual,
                b'>' => {
                    self.current += 1;
                    TokenKind::Greater
                }
                b'&' if self.consume(b"&&") => TokenKind::And,
                b'|' if self.consume(b"||") => TokenKind::Or,
                b'.' => {
                    self.current += 1;
                    TokenKind::Dot
                }
                b'[' => {
                    self.current += 1;
                    TokenKind::LeftBracket
                }
                b']' => {
                    self.current += 1;
                    TokenKind::RightBracket
                }
                b'(' => {
                    self.current += 1;
                    TokenKind::LeftParen
                }
                b')' => {
                    self.current += 1;
                    TokenKind::RightParen
                }
                b'\'' | b'"' => self.string(byte)?,
                byte if byte.is_ascii_digit() || byte == b'-' => self.number()?,
                byte if identifier_start(byte) => self.identifier(),
                _ => {
                    return Err(self.error(
                        start,
                        start.saturating_add(1),
                        "Template expression contains unsupported syntax.",
                    ));
                }
            };
            tokens.push(Token {
                kind,
                start,
                end: self.current,
            });
        }
        tokens.push(Token {
            kind: TokenKind::End,
            start: self.current,
            end: self.current,
        });
        Ok(tokens)
    }

    fn consume(&mut self, expected: &[u8]) -> bool {
        let end = self.current.saturating_add(expected.len());
        if self.source.as_bytes().get(self.current..end) == Some(expected) {
            self.current = end;
            true
        } else {
            false
        }
    }

    fn string(&mut self, quote: u8) -> Result<TokenKind, Failure> {
        let start = self.current;
        self.current += 1;
        let mut value = String::new();
        while self.current < self.source.len() {
            let byte = self.source.as_bytes()[self.current];
            self.current += 1;
            if byte == quote {
                return Ok(TokenKind::Literal(Value::String(value)));
            }
            if byte == b'\\' {
                let Some(escaped) = self.source.as_bytes().get(self.current).copied() else {
                    break;
                };
                self.current += 1;
                match escaped {
                    b'\\' => value.push('\\'),
                    b'\'' => value.push('\''),
                    b'"' => value.push('"'),
                    b'n' => value.push('\n'),
                    b'r' => value.push('\r'),
                    b't' => value.push('\t'),
                    _ => {
                        return Err(self.error(
                            self.current.saturating_sub(2),
                            self.current,
                            "Template string contains an unsupported escape.",
                        ));
                    }
                }
                continue;
            }
            let Some(character) = self.source[self.current - 1..].chars().next() else {
                break;
            };
            value.push(character);
            self.current = self.current.saturating_sub(1) + character.len_utf8();
        }
        Err(self.error(
            start,
            self.source.len(),
            "Template string is not terminated.",
        ))
    }

    fn number(&mut self) -> Result<TokenKind, Failure> {
        let start = self.current;
        if self.source.as_bytes()[self.current] == b'-' {
            self.current += 1;
        }
        while self
            .source
            .as_bytes()
            .get(self.current)
            .is_some_and(u8::is_ascii_digit)
        {
            self.current += 1;
        }
        if self.source.as_bytes().get(self.current) == Some(&b'.') {
            self.current += 1;
            while self
                .source
                .as_bytes()
                .get(self.current)
                .is_some_and(u8::is_ascii_digit)
            {
                self.current += 1;
            }
        }
        if self
            .source
            .as_bytes()
            .get(self.current)
            .is_some_and(|byte| matches!(byte, b'e' | b'E'))
        {
            self.current += 1;
            if self
                .source
                .as_bytes()
                .get(self.current)
                .is_some_and(|byte| matches!(byte, b'+' | b'-'))
            {
                self.current += 1;
            }
            while self
                .source
                .as_bytes()
                .get(self.current)
                .is_some_and(u8::is_ascii_digit)
            {
                self.current += 1;
            }
        }
        let raw = &self.source[start..self.current];
        let number = Number::from_str(raw).map_err(|_| {
            self.error(
                start,
                self.current,
                "Template expression contains an invalid finite number.",
            )
        })?;
        Ok(TokenKind::Literal(Value::Number(number)))
    }

    fn identifier(&mut self) -> TokenKind {
        let start = self.current;
        self.current += 1;
        while self
            .source
            .as_bytes()
            .get(self.current)
            .is_some_and(|byte| identifier_continue(*byte))
        {
            self.current += 1;
        }
        match &self.source[start..self.current] {
            "true" => TokenKind::Literal(Value::Bool(true)),
            "false" => TokenKind::Literal(Value::Bool(false)),
            "null" => TokenKind::Literal(Value::Null),
            value => TokenKind::Identifier(String::from(value)),
        }
    }

    fn error(&self, start: usize, end: usize, message: &str) -> Failure {
        expression_failure(
            self.source_path,
            self.source_start.saturating_add(start),
            self.source_start.saturating_add(end),
            message,
        )
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    current: usize,
    source_path: &'a str,
    source_start: usize,
    source_len: usize,
}

impl Parser<'_> {
    /// `condition ? when_true : when_false`, the shape every template
    /// language offers for choosing a value inline.
    fn parse_ternary(&mut self, depth: usize) -> Result<Expr, Failure> {
        self.assert_depth(depth)?;
        let condition = self.parse_or(depth + 1)?;
        if !matches!(self.peek().kind, TokenKind::Question) {
            return Ok(condition);
        }
        self.advance();
        let when_true = self.parse_ternary(depth + 1)?;
        if !matches!(self.peek().kind, TokenKind::Colon) {
            return Err(self.error("Conditional expression requires ':' before its alternative."));
        }
        self.advance();
        let when_false = self.parse_ternary(depth + 1)?;
        Ok(Expr::Conditional {
            condition: Box::new(condition),
            when_true: Box::new(when_true),
            when_false: Box::new(when_false),
        })
    }

    fn parse_or(&mut self, depth: usize) -> Result<Expr, Failure> {
        self.assert_depth(depth)?;
        let mut expression = self.parse_and(depth + 1)?;
        while matches!(self.peek().kind, TokenKind::Or) {
            self.advance();
            let right = self.parse_and(depth + 1)?;
            expression = Expr::Logical {
                left: Box::new(expression),
                operator: LogicalOperator::Or,
                right: Box::new(right),
            };
        }
        Ok(expression)
    }

    fn parse_and(&mut self, depth: usize) -> Result<Expr, Failure> {
        self.assert_depth(depth)?;
        let mut expression = self.parse_comparison(depth + 1)?;
        while matches!(self.peek().kind, TokenKind::And) {
            self.advance();
            let right = self.parse_comparison(depth + 1)?;
            expression = Expr::Logical {
                left: Box::new(expression),
                operator: LogicalOperator::And,
                right: Box::new(right),
            };
        }
        Ok(expression)
    }

    fn parse_comparison(&mut self, depth: usize) -> Result<Expr, Failure> {
        self.assert_depth(depth)?;
        let left = self.parse_additive(depth + 1)?;
        let operator = match self.peek().kind {
            TokenKind::Equal => Some(CompareOperator::Equal),
            TokenKind::NotEqual => Some(CompareOperator::NotEqual),
            TokenKind::Less => Some(CompareOperator::Less),
            TokenKind::LessEqual => Some(CompareOperator::LessEqual),
            TokenKind::Greater => Some(CompareOperator::Greater),
            TokenKind::GreaterEqual => Some(CompareOperator::GreaterEqual),
            _ => None,
        };
        let Some(operator) = operator else {
            return Ok(left);
        };
        self.advance();
        let right = self.parse_additive(depth + 1)?;
        Ok(Expr::Compare {
            left: Box::new(left),
            operator,
            right: Box::new(right),
        })
    }

    /// `+`, `-`, `*`, and `/`, left-associative, with `*` and `/` binding
    /// tighter. `+` also concatenates when either side is a string, which is
    /// what a template uses it for.
    fn parse_additive(&mut self, depth: usize) -> Result<Expr, Failure> {
        self.assert_depth(depth)?;
        let mut left = self.parse_multiplicative(depth + 1)?;
        loop {
            let operator = match self.peek().kind {
                TokenKind::Plus => ArithmeticOperator::Add,
                TokenKind::Minus => ArithmeticOperator::Subtract,
                _ => return Ok(left),
            };
            self.advance();
            let right = self.parse_multiplicative(depth + 1)?;
            left = Expr::Arithmetic {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            };
        }
    }

    fn parse_multiplicative(&mut self, depth: usize) -> Result<Expr, Failure> {
        self.assert_depth(depth)?;
        let mut left = self.parse_unary(depth + 1)?;
        loop {
            let operator = match self.peek().kind {
                TokenKind::Star => ArithmeticOperator::Multiply,
                TokenKind::Slash => ArithmeticOperator::Divide,
                _ => return Ok(left),
            };
            self.advance();
            let right = self.parse_unary(depth + 1)?;
            left = Expr::Arithmetic {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            };
        }
    }

    fn parse_unary(&mut self, depth: usize) -> Result<Expr, Failure> {
        self.assert_depth(depth)?;
        // `await` is only meaningful where evaluation can be async, which is
        // inside an island; elsewhere it is refused at render.
        if matches!(&self.peek().kind, TokenKind::Identifier(name) if name == "await") {
            self.advance();
            return Ok(Expr::Await(Box::new(self.parse_unary(depth + 1)?)));
        }
        if matches!(self.peek().kind, TokenKind::Bang) {
            self.advance();
            return Ok(Expr::Not(Box::new(self.parse_unary(depth + 1)?)));
        }
        self.parse_access(depth + 1)
    }

    fn parse_access(&mut self, depth: usize) -> Result<Expr, Failure> {
        self.assert_depth(depth)?;
        let mut expression = self.parse_primary(depth + 1)?;
        loop {
            if matches!(self.peek().kind, TokenKind::Dot) {
                self.advance();
                let TokenKind::Identifier(property) = self.peek().kind.clone() else {
                    return Err(self.error("Property access requires an identifier."));
                };
                if FORBIDDEN_PROPERTIES.contains(&property.as_str()) {
                    return Err(
                        self.error("Template expressions cannot access prototype properties.")
                    );
                }
                self.advance();
                expression = Expr::Access(Box::new(expression), Access::Property(property));
            } else if matches!(self.peek().kind, TokenKind::LeftParen) {
                self.advance();
                let mut arguments = Vec::new();
                while !matches!(self.peek().kind, TokenKind::RightParen) {
                    if arguments.len() >= MAX_CALL_ARGUMENTS {
                        return Err(self.error("A call accepts at most eight arguments."));
                    }
                    arguments.push(self.parse_ternary(depth + 1)?);
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                if !matches!(self.peek().kind, TokenKind::RightParen) {
                    return Err(self.error("A call is missing ')'."));
                }
                self.advance();
                expression = Expr::Call {
                    callee: Box::new(expression),
                    arguments,
                };
            } else if matches!(self.peek().kind, TokenKind::LeftBracket) {
                self.advance();
                let TokenKind::Literal(Value::Number(index)) = self.peek().kind.clone() else {
                    return Err(self.error("Array access requires a non-negative integer literal."));
                };
                let Some(index) = index.as_u64().and_then(|value| usize::try_from(value).ok())
                else {
                    return Err(self.error("Array index is outside the supported range."));
                };
                self.advance();
                if !matches!(self.peek().kind, TokenKind::RightBracket) {
                    return Err(self.error("Array access is missing ']'."));
                }
                self.advance();
                expression = Expr::Access(Box::new(expression), Access::Index(index));
            } else {
                break;
            }
        }
        Ok(expression)
    }

    fn parse_primary(&mut self, depth: usize) -> Result<Expr, Failure> {
        self.assert_depth(depth)?;
        match self.peek().kind.clone() {
            TokenKind::Literal(value) => {
                self.advance();
                Ok(Expr::Literal(value))
            }
            TokenKind::Identifier(identifier) => {
                self.advance();
                Ok(Expr::Identifier(identifier))
            }
            TokenKind::LeftParen => {
                self.advance();
                let expression = self.parse_ternary(depth + 1)?;
                if !matches!(self.peek().kind, TokenKind::RightParen) {
                    return Err(self.error("Template expression is missing ')'."));
                }
                self.advance();
                Ok(expression)
            }
            _ => Err(self.error("Template expression is incomplete.")),
        }
    }

    fn assert_depth(&self, depth: usize) -> Result<(), Failure> {
        if depth > MAX_EXPRESSION_DEPTH {
            return Err(self.error("Template expression exceeds the nesting limit of 32."));
        }
        Ok(())
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn advance(&mut self) {
        if self.current + 1 < self.tokens.len() {
            self.current += 1;
        }
    }

    fn error(&self, message: &str) -> Failure {
        let token = self.peek();
        expression_failure(
            self.source_path,
            self.source_start.saturating_add(token.start),
            self.source_start
                .saturating_add(token.end.max(token.start + 1).min(self.source_len)),
            message,
        )
    }
}

fn evaluate(
    expression: &Expr,
    scope: &Scope,
    source_path: &str,
    source_start: usize,
    source_len: usize,
) -> Result<Value, Failure> {
    let missing = || {
        expression_failure(
            source_path,
            source_start,
            source_start.saturating_add(source_len),
            "Template expression references a missing or incompatible value.",
        )
    };
    match expression {
        Expr::Literal(value) => Ok(value.clone()),
        Expr::Call { callee, arguments } => evaluate_safe_call(
            callee,
            arguments,
            scope,
            source_path,
            source_start,
            source_len,
        )
        .unwrap_or_else(|| {
            Err(island_expression_failure(
                source_path,
                source_start,
                source_len,
                "A call needs a companion instance, which exists only inside an island.",
            ))
        }),
        Expr::Await(_) => Err(island_expression_failure(
            source_path,
            source_start,
            source_len,
            "An awaited value needs async evaluation, which happens only inside an island.",
        )),
        Expr::Conditional { .. } | Expr::Arithmetic { .. } => {
            evaluate_composite(expression, scope, source_path, source_start, source_len)
        }
        Expr::Identifier(identifier) => scope.get(identifier).cloned().ok_or_else(missing),
        Expr::Access(_, _) => {
            evaluate_access(expression, scope, source_path, source_start, source_len)
        }
        Expr::Not(inner) => Ok(Value::Bool(!truthy(&evaluate(
            inner,
            scope,
            source_path,
            source_start,
            source_len,
        )?))),
        Expr::Logical {
            left,
            operator: LogicalOperator::And,
            right,
        } => {
            let left = evaluate(left, scope, source_path, source_start, source_len)?;
            if !truthy(&left) {
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(truthy(&evaluate(
                right,
                scope,
                source_path,
                source_start,
                source_len,
            )?)))
        }
        Expr::Logical {
            left,
            operator: LogicalOperator::Or,
            right,
        } => {
            let left = evaluate(left, scope, source_path, source_start, source_len)?;
            if truthy(&left) {
                return Ok(Value::Bool(true));
            }
            Ok(Value::Bool(truthy(&evaluate(
                right,
                scope,
                source_path,
                source_start,
                source_len,
            )?)))
        }
        Expr::Compare {
            left,
            operator,
            right,
        } => {
            let left = evaluate(left, scope, source_path, source_start, source_len)?;
            let right = evaluate(right, scope, source_path, source_start, source_len)?;
            compare(&left, *operator, &right)
                .map(Value::Bool)
                .ok_or_else(missing)
        }
    }
}

fn evaluate_safe_call(
    callee: &Expr,
    arguments: &[Expr],
    scope: &Scope,
    source_path: &str,
    source_start: usize,
    source_len: usize,
) -> Option<Result<Value, Failure>> {
    let Expr::Access(parent, Access::Property(method)) = callee else {
        return None;
    };
    if method != "join" || arguments.len() > 1 {
        return None;
    }
    let missing = || {
        expression_failure(
            source_path,
            source_start,
            source_start.saturating_add(source_len),
            "Template expression references a missing or incompatible value.",
        )
    };
    let result = (|| {
        let Value::Array(items) = evaluate(parent, scope, source_path, source_start, source_len)?
        else {
            return Err(missing());
        };
        let separator = if let Some(argument) = arguments.first() {
            evaluate(argument, scope, source_path, source_start, source_len)?
                .as_str()
                .map(String::from)
                .ok_or_else(missing)?
        } else {
            String::from(",")
        };
        let mut parts = Vec::with_capacity(items.len());
        for item in items {
            match item {
                Value::Null => parts.push(String::new()),
                Value::Bool(value) => parts.push(value.to_string()),
                Value::Number(value) => parts.push(value.to_string()),
                Value::String(value) => parts.push(value),
                Value::Array(_) | Value::Object(_) => return Err(missing()),
            }
        }
        Ok(Value::String(parts.join(&separator)))
    })();
    Some(result)
}

fn evaluate_access(
    expression: &Expr,
    scope: &Scope,
    source_path: &str,
    source_start: usize,
    source_len: usize,
) -> Result<Value, Failure> {
    let Expr::Access(parent, access) = expression else {
        unreachable!();
    };
    let value = evaluate(parent, scope, source_path, source_start, source_len)?;
    let found = match access {
        Access::Property(property) => value.as_object().and_then(|object| object.get(property)),
        Access::Index(index) => value.as_array().and_then(|array| array.get(*index)),
    };
    found.cloned().ok_or_else(|| {
        island_expression_failure(
            source_path,
            source_start,
            source_len,
            "Template expression references a missing or incompatible value.",
        )
    })
}

fn island_expression_failure(
    source_path: &str,
    source_start: usize,
    source_len: usize,
    message: &str,
) -> Failure {
    expression_failure(
        source_path,
        source_start,
        source_start.saturating_add(source_len),
        message,
    )
}

pub(crate) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn compare(left: &Value, operator: CompareOperator, right: &Value) -> Option<bool> {
    match operator {
        CompareOperator::Equal => Some(left == right),
        CompareOperator::NotEqual => Some(left != right),
        CompareOperator::Less
        | CompareOperator::LessEqual
        | CompareOperator::Greater
        | CompareOperator::GreaterEqual => {
            let ordering = match (left, right) {
                (Value::Number(left), Value::Number(right)) => {
                    left.as_f64()?.partial_cmp(&right.as_f64()?)?
                }
                (Value::String(left), Value::String(right)) => left.cmp(right),
                _ => return None,
            };
            Some(match operator {
                CompareOperator::Less => ordering.is_lt(),
                CompareOperator::LessEqual => ordering.is_le(),
                CompareOperator::Greater => ordering.is_gt(),
                CompareOperator::GreaterEqual => ordering.is_ge(),
                CompareOperator::Equal | CompareOperator::NotEqual => false,
            })
        }
    }
}

fn identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

fn identifier_continue(byte: u8) -> bool {
    identifier_start(byte) || byte.is_ascii_digit()
}

/// Evaluates the two composite node kinds, kept out of `evaluate` so neither
/// function grows past the project's length limit.
fn evaluate_composite(
    expression: &Expr,
    scope: &Scope,
    source_path: &str,
    source_start: usize,
    source_len: usize,
) -> Result<Value, Failure> {
    match expression {
        Expr::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            let condition = evaluate(condition, scope, source_path, source_start, source_len)?;
            let taken = if truthy(&condition) {
                when_true
            } else {
                when_false
            };
            evaluate(taken, scope, source_path, source_start, source_len)
        }
        Expr::Arithmetic {
            left,
            operator,
            right,
        } => {
            let left = evaluate(left, scope, source_path, source_start, source_len)?;
            let right = evaluate(right, scope, source_path, source_start, source_len)?;
            arithmetic(&left, *operator, &right).ok_or_else(|| {
                expression_failure(
                    source_path,
                    source_start,
                    source_start.saturating_add(source_len),
                    "Template expression references a missing or incompatible value.",
                )
            })
        }
        _ => unreachable!("caller matched a composite"),
    }
}

/// Applies one arithmetic operator, concatenating when `+` sees a string.
///
/// A template uses `+` mostly to build a path or a label, so a string on
/// either side concatenates. Anything else needs two numbers, and division by
/// zero is a missing value rather than an infinity.
fn arithmetic(left: &Value, operator: ArithmeticOperator, right: &Value) -> Option<Value> {
    if operator == ArithmeticOperator::Add && (left.is_string() || right.is_string()) {
        let render = |value: &Value| match value {
            Value::String(text) => text.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        };
        return Some(Value::String(format!("{}{}", render(left), render(right))));
    }
    let left = left.as_f64()?;
    let right = right.as_f64()?;
    let result = match operator {
        ArithmeticOperator::Add => left + right,
        ArithmeticOperator::Subtract => left - right,
        ArithmeticOperator::Multiply => left * right,
        ArithmeticOperator::Divide => left / right,
    };
    if !result.is_finite() {
        return None;
    }
    serde_json::Number::from_f64(result).map(Value::Number)
}

fn expression_failure(source_path: &str, start: usize, end: usize, message: &str) -> Failure {
    Failure::one(diagnostic(
        1303,
        message,
        Some(String::from(
            "Use bounded JSON paths, literals, comparisons, and boolean operators.",
        )),
        source_span(source_path, start, end),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{Expression, Scope, truthy};
    use serde_json::{Value, json};

    fn expression(source: &str) -> Expression {
        Expression::parse(source, "server/routes/yon.html", 10).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn conditional_and_arithmetic_expressions_evaluate() {
        let scope = Scope::from([
            (String::from("slug"), Value::String(String::from("store"))),
            (String::from("active"), Value::String(String::from("store"))),
            (String::from("count"), Value::from(7)),
        ]);
        let evaluate = |source: &str| {
            Expression::parse(source, "client/pages/tac.html", 0)
                .expect(source)
                .evaluate(&scope)
                .expect(source)
        };

        // A ternary is how every template language picks a value inline.
        assert_eq!(
            evaluate("slug === active ? 'on' : 'off'"),
            Value::from("on")
        );
        assert_eq!(
            evaluate("slug === 'other' ? 'on' : 'off'"),
            Value::from("off")
        );
        // A null alternative drops the attribute rather than printing "null".
        assert_eq!(evaluate("slug === 'other' ? 'page' : null"), Value::Null);

        // `+` concatenates when either side is a string, which is what a
        // template uses it for, and adds otherwise.
        assert_eq!(evaluate("'/atlas/' + slug"), Value::from("/atlas/store"));
        assert_eq!(evaluate("count + 1"), Value::from(8.0));
        assert_eq!(evaluate("count * 2 - 4"), Value::from(10.0));
        // Multiplication binds tighter than addition.
        assert_eq!(evaluate("1 + count * 2"), Value::from(15.0));

        // Division by zero is a missing value, not an infinity.
        assert!(
            Expression::parse("count / 0", "client/pages/tac.html", 0)
                .expect("parse")
                .evaluate(&scope)
                .is_err()
        );
        // A ternary without its alternative is refused.
        assert!(Expression::parse("slug ? 'on'", "client/pages/tac.html", 0).is_err());
    }

    #[test]
    fn paths_literals_boolean_logic_and_comparisons_evaluate() {
        let mut scope = Scope::new();
        scope.insert(
            String::from("product"),
            json!({"name": "Tachyon", "prices": [3, 7]}),
        );
        scope.insert(String::from("active"), json!(true));
        for (source, expected) in [
            ("product.name", json!("Tachyon")),
            ("product.prices[1]", json!(7)),
            ("active && product.prices[0] < 4", json!(true)),
            ("!false && ('a' === \"a\")", json!(true)),
            ("null != product.name || false", json!(true)),
            ("3.5e1 >= 35", json!(true)),
            ("7 > 3 && 3 <= 3", json!(true)),
            ("'b' > 'a' && 'a' <= 'a'", json!(true)),
            ("false && missing", json!(false)),
            ("true || missing", json!(true)),
            ("\"line\\nquote\\\"slash\\\\\" !== ''", json!(true)),
        ] {
            assert_eq!(
                expression(source)
                    .evaluate(&scope)
                    .unwrap_or_else(|_| unreachable!()),
                expected
            );
        }
    }

    #[test]
    fn unsupported_missing_malformed_and_excessive_expressions_fail() {
        let scope = Scope::new();
        for source in [
            "",
            "missing",
            "call()",
            "value = 1",
            "object.__proto__",
            "items[-1]",
            "items[0",
            "items[value]",
            "object.",
            "(true",
            "'bad\\q'",
            "1e",
            "'unterminated",
            &"(".repeat(34),
            &"x".repeat(1_025),
        ] {
            let result = Expression::parse(source, "client/pages/tac.html", 0)
                .and_then(|parsed| parsed.evaluate(&scope));
            assert!(result.is_err(), "{source}");
        }
    }

    #[test]
    fn json_truthiness_is_explicit() {
        for value in [json!(null), json!(false), json!(0), json!("")] {
            assert!(!truthy(&value));
        }
        for value in [json!(true), json!(1), json!("x"), json!([]), json!({})] {
            assert!(truthy(&value));
        }
    }
}
