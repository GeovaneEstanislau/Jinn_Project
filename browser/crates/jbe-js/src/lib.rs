//! Lightweight sandboxed JavaScript interpreter and DOM bridge for Jinn Browser Engine.
//!
//! Provides deterministic statement execution, bounded computation steps,
//! and native DOM manipulation primitives (`document.getElementById`, `innerText`, `style`).

use std::collections::HashMap;
use jbe_core::NodeId;
use jbe_dom::DomTree;

/// JavaScript runtime values.
#[derive(Debug, Clone, PartialEq)]
pub enum JsValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Element(NodeId),
}

impl JsValue {
    pub fn to_string_val(&self) -> String {
        match self {
            JsValue::Undefined => "undefined".to_string(),
            JsValue::Null => "null".to_string(),
            JsValue::Boolean(b) => if *b { "true".to_string() } else { "false".to_string() },
            JsValue::Number(n) => {
                if n.fract() == 0.0 {
                    format!("{:.0}", n)
                } else {
                    format!("{}", n)
                }
            }
            JsValue::String(s) => s.clone(),
            JsValue::Element(id) => format!("[object Element#{}]", id.index()),
        }
    }

    pub fn to_number(&self) -> f64 {
        match self {
            JsValue::Undefined => f64::NAN,
            JsValue::Null => 0.0,
            JsValue::Boolean(b) => if *b { 1.0 } else { 0.0 },
            JsValue::Number(n) => *n,
            JsValue::String(s) => s.trim().parse::<f64>().unwrap_or(f64::NAN),
            JsValue::Element(_) => f64::NAN,
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            JsValue::Undefined | JsValue::Null => false,
            JsValue::Boolean(b) => *b,
            JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
            JsValue::String(s) => !s.is_empty(),
            JsValue::Element(_) => true,
        }
    }
}

/// Errors raised during parsing or execution of JS.
#[derive(Debug, Clone, PartialEq)]
pub enum JsError {
    ParseError(String),
    ExecutionError(String),
    StepLimitExceeded,
    ReferenceError(String),
}

// ── AST Definition ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    NotEq,
    StrictEq,
    StrictNotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(JsValue),
    Ident(String),
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    Assign(String, Box<Expr>),
    Member(Box<Expr>, String),
    MemberAssign(Box<Expr>, String, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Var(String, Option<Expr>),
    Expr(Expr),
    If(Expr, Vec<Stmt>, Option<Vec<Stmt>>),
    While(Expr, Vec<Stmt>),
    FunctionDecl(String, Vec<String>, Vec<Stmt>),
    Return(Option<Expr>),
}

// ── Tokenizer ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Var,
    Let,
    If,
    Else,
    While,
    Function,
    Return,
    Ident(String),
    Number(f64),
    StringLit(String),
    True,
    False,
    Null,
    Undefined,
    Plus,
    Minus,
    Star,
    Slash,
    Eq,
    EqEq,
    NotEq,
    StrictEq,
    StrictNotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Semicolon,
    Comma,
    Dot,
    Eof,
}

struct Tokenizer<'a> {
    chars: &'a [u8],
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            chars: input.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        if self.pos < self.chars.len() {
            Some(self.chars[self.pos])
        } else {
            None
        }
    }

    fn next(&mut self) -> Option<u8> {
        if self.pos < self.chars.len() {
            let ch = self.chars[self.pos];
            self.pos += 1;
            Some(ch)
        } else {
            None
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == b' ' || ch == b'\t' || ch == b'\r' || ch == b'\n' {
                self.pos += 1;
            } else if ch == b'/' && self.pos + 1 < self.chars.len() && self.chars[self.pos + 1] == b'/' {
                // Line comment
                self.pos += 2;
                while self.pos < self.chars.len() && self.chars[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Result<Token, JsError> {
        self.skip_whitespace_and_comments();
        let ch = match self.next() {
            Some(c) => c,
            None => return Ok(Token::Eof),
        };

        match ch {
            b'(' => Ok(Token::LParen),
            b')' => Ok(Token::RParen),
            b'{' => Ok(Token::LBrace),
            b'}' => Ok(Token::RBrace),
            b';' => Ok(Token::Semicolon),
            b',' => Ok(Token::Comma),
            b'.' => Ok(Token::Dot),
            b'+' => Ok(Token::Plus),
            b'-' => Ok(Token::Minus),
            b'*' => Ok(Token::Star),
            b'/' => Ok(Token::Slash),
            b'=' => {
                if self.peek() == Some(b'=') {
                    self.next();
                    if self.peek() == Some(b'=') {
                        self.next();
                        Ok(Token::StrictEq)
                    } else {
                        Ok(Token::EqEq)
                    }
                } else {
                    Ok(Token::Eq)
                }
            }
            b'!' => {
                if self.peek() == Some(b'=') {
                    self.next();
                    if self.peek() == Some(b'=') {
                        self.next();
                        Ok(Token::StrictNotEq)
                    } else {
                        Ok(Token::NotEq)
                    }
                } else {
                    Err(JsError::ParseError("Unexpected '!' without '='".into()))
                }
            }
            b'<' => {
                if self.peek() == Some(b'=') {
                    self.next();
                    Ok(Token::LtEq)
                } else {
                    Ok(Token::Lt)
                }
            }
            b'>' => {
                if self.peek() == Some(b'=') {
                    self.next();
                    Ok(Token::GtEq)
                } else {
                    Ok(Token::Gt)
                }
            }
            b'"' | b'\'' => {
                let quote = ch;
                let start = self.pos;
                while let Some(c) = self.next() {
                    if c == quote {
                        let text = std::str::from_utf8(&self.chars[start..self.pos - 1])
                            .map_err(|_| JsError::ParseError("Invalid UTF-8 in string".into()))?;
                        return Ok(Token::StringLit(text.to_string()));
                    }
                }
                Err(JsError::ParseError("Unterminated string literal".into()))
            }
            b'0'..=b'9' => {
                let start = self.pos - 1;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() || c == b'.' {
                        self.next();
                    } else {
                        break;
                    }
                }
                let num_str = std::str::from_utf8(&self.chars[start..self.pos])
                    .map_err(|_| JsError::ParseError("Invalid number format".into()))?;
                let n = num_str.parse::<f64>().map_err(|_| JsError::ParseError("Failed to parse number".into()))?;
                Ok(Token::Number(n))
            }
            c if c.is_ascii_alphabetic() || c == b'_' || c == b'$' => {
                let start = self.pos - 1;
                while let Some(nxt) = self.peek() {
                    if nxt.is_ascii_alphanumeric() || nxt == b'_' || nxt == b'$' {
                        self.next();
                    } else {
                        break;
                    }
                }
                let word = std::str::from_utf8(&self.chars[start..self.pos])
                    .map_err(|_| JsError::ParseError("Invalid UTF-8 identifier".into()))?;
                match word {
                    "var" => Ok(Token::Var),
                    "let" => Ok(Token::Let),
                    "if" => Ok(Token::If),
                    "else" => Ok(Token::Else),
                    "while" => Ok(Token::While),
                    "function" => Ok(Token::Function),
                    "return" => Ok(Token::Return),
                    "true" => Ok(Token::True),
                    "false" => Ok(Token::False),
                    "null" => Ok(Token::Null),
                    "undefined" => Ok(Token::Undefined),
                    other => Ok(Token::Ident(other.to_string())),
                }
            }
            other => Err(JsError::ParseError(format!("Unexpected character: '{}'", other as char))),
        }
    }
}

// ── Parser ────────────────────────────────────────────────────────────────────

pub struct JsParser {
    tokens: Vec<Token>,
    idx: usize,
}

impl JsParser {
    pub fn parse(input: &str) -> Result<Vec<Stmt>, JsError> {
        let mut tokenizer = Tokenizer::new(input);
        let mut tokens = Vec::new();
        loop {
            let tok = tokenizer.next_token()?;
            if tok == Token::Eof {
                tokens.push(Token::Eof);
                break;
            }
            tokens.push(tok);
        }

        let mut parser = Self { tokens, idx: 0 };
        let mut statements = Vec::new();
        while parser.peek() != &Token::Eof {
            statements.push(parser.parse_statement()?);
        }
        Ok(statements)
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.idx).unwrap_or(&Token::Eof)
    }

    fn next(&mut self) -> Token {
        if self.idx < self.tokens.len() {
            let t = self.tokens[self.idx].clone();
            self.idx += 1;
            t
        } else {
            Token::Eof
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), JsError> {
        let tok = self.next();
        if tok == expected {
            Ok(())
        } else {
            Err(JsError::ParseError(format!("Expected {:?}, got {:?}", expected, tok)))
        }
    }

    fn parse_statement(&mut self) -> Result<Stmt, JsError> {
        match self.peek() {
            Token::Var | Token::Let => {
                self.next(); // var / let
                let name = match self.next() {
                    Token::Ident(s) => s,
                    other => return Err(JsError::ParseError(format!("Expected variable name, got {:?}", other))),
                };
                let init = if self.peek() == &Token::Eq {
                    self.next();
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                if self.peek() == &Token::Semicolon {
                    self.next();
                }
                Ok(Stmt::Var(name, init))
            }
            Token::If => {
                self.next();
                self.expect(Token::LParen)?;
                let cond = self.parse_expression()?;
                self.expect(Token::RParen)?;
                let then_body = self.parse_block_or_stmt()?;
                let else_body = if self.peek() == &Token::Else {
                    self.next();
                    Some(self.parse_block_or_stmt()?)
                } else {
                    None
                };
                Ok(Stmt::If(cond, then_body, else_body))
            }
            Token::While => {
                self.next();
                self.expect(Token::LParen)?;
                let cond = self.parse_expression()?;
                self.expect(Token::RParen)?;
                let body = self.parse_block_or_stmt()?;
                Ok(Stmt::While(cond, body))
            }
            Token::Function => {
                self.next();
                let name = match self.next() {
                    Token::Ident(s) => s,
                    other => return Err(JsError::ParseError(format!("Expected function name, got {:?}", other))),
                };
                self.expect(Token::LParen)?;
                let mut params = Vec::new();
                if self.peek() != &Token::RParen {
                    loop {
                        match self.next() {
                            Token::Ident(p) => params.push(p),
                            other => return Err(JsError::ParseError(format!("Expected parameter, got {:?}", other))),
                        }
                        if self.peek() == &Token::Comma {
                            self.next();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(Token::RParen)?;
                let body = self.parse_block()?;
                Ok(Stmt::FunctionDecl(name, params, body))
            }
            Token::Return => {
                self.next();
                let expr = if self.peek() != &Token::Semicolon && self.peek() != &Token::RBrace && self.peek() != &Token::Eof {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                if self.peek() == &Token::Semicolon {
                    self.next();
                }
                Ok(Stmt::Return(expr))
            }
            _ => {
                let expr = self.parse_expression()?;
                if self.peek() == &Token::Semicolon {
                    self.next();
                }
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn parse_block_or_stmt(&mut self) -> Result<Vec<Stmt>, JsError> {
        if self.peek() == &Token::LBrace {
            self.parse_block()
        } else {
            Ok(vec![self.parse_statement()?])
        }
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, JsError> {
        self.expect(Token::LBrace)?;
        let mut stmts = Vec::new();
        while self.peek() != &Token::RBrace && self.peek() != &Token::Eof {
            stmts.push(self.parse_statement()?);
        }
        self.expect(Token::RBrace)?;
        Ok(stmts)
    }

    fn parse_expression(&mut self) -> Result<Expr, JsError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, JsError> {
        let expr = self.parse_equality()?;
        if self.peek() == &Token::Eq {
            self.next();
            let value = self.parse_assignment()?;
            match expr {
                Expr::Ident(name) => Ok(Expr::Assign(name, Box::new(value))),
                Expr::Member(obj, prop) => Ok(Expr::MemberAssign(obj, prop, Box::new(value))),
                other => Err(JsError::ParseError(format!("Invalid left-hand side in assignment: {:?}", other))),
            }
        } else {
            Ok(expr)
        }
    }

    fn parse_equality(&mut self) -> Result<Expr, JsError> {
        let mut expr = self.parse_relational()?;
        while matches!(self.peek(), Token::EqEq | Token::NotEq | Token::StrictEq | Token::StrictNotEq) {
            let op = match self.next() {
                Token::EqEq | Token::StrictEq => BinaryOp::Eq,
                Token::NotEq | Token::StrictNotEq => BinaryOp::NotEq,
                _ => unreachable!(),
            };
            let right = self.parse_relational()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_relational(&mut self) -> Result<Expr, JsError> {
        let mut expr = self.parse_additive()?;
        while matches!(self.peek(), Token::Lt | Token::LtEq | Token::Gt | Token::GtEq) {
            let op = match self.next() {
                Token::Lt => BinaryOp::Lt,
                Token::LtEq => BinaryOp::LtEq,
                Token::Gt => BinaryOp::Gt,
                Token::GtEq => BinaryOp::GtEq,
                _ => unreachable!(),
            };
            let right = self.parse_additive()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_additive(&mut self) -> Result<Expr, JsError> {
        let mut expr = self.parse_multiplicative()?;
        while matches!(self.peek(), Token::Plus | Token::Minus) {
            let op = match self.next() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => unreachable!(),
            };
            let right = self.parse_multiplicative()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, JsError> {
        let mut expr = self.parse_postfix()?;
        while matches!(self.peek(), Token::Star | Token::Slash) {
            let op = match self.next() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                _ => unreachable!(),
            };
            let right = self.parse_postfix()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_postfix(&mut self) -> Result<Expr, JsError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.peek() == &Token::Dot {
                self.next();
                let prop = match self.next() {
                    Token::Ident(s) => s,
                    other => return Err(JsError::ParseError(format!("Expected property name after '.', got {:?}", other))),
                };
                expr = Expr::Member(Box::new(expr), prop);
            } else if self.peek() == &Token::LParen {
                self.next();
                let mut args = Vec::new();
                if self.peek() != &Token::RParen {
                    loop {
                        args.push(self.parse_expression()?);
                        if self.peek() == &Token::Comma {
                            self.next();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(Token::RParen)?;
                expr = Expr::Call(Box::new(expr), args);
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, JsError> {
        match self.next() {
            Token::Number(n) => Ok(Expr::Literal(JsValue::Number(n))),
            Token::StringLit(s) => Ok(Expr::Literal(JsValue::String(s))),
            Token::True => Ok(Expr::Literal(JsValue::Boolean(true))),
            Token::False => Ok(Expr::Literal(JsValue::Boolean(false))),
            Token::Null => Ok(Expr::Literal(JsValue::Null)),
            Token::Undefined => Ok(Expr::Literal(JsValue::Undefined)),
            Token::Ident(s) => Ok(Expr::Ident(s)),
            Token::LParen => {
                let expr = self.parse_expression()?;
                self.expect(Token::RParen)?;
                Ok(expr)
            }
            other => Err(JsError::ParseError(format!("Unexpected token in primary expression: {:?}", other))),
        }
    }
}

// ── Interpreter & DOM Bridge ──────────────────────────────────────────────────

/// Sandboxed JavaScript Runtime for Jinn Browser.
pub struct JsRuntime {
    globals: HashMap<String, JsValue>,
    locals: Vec<HashMap<String, JsValue>>,
    functions: HashMap<String, (Vec<String>, Vec<Stmt>)>,
    event_listeners: HashMap<(NodeId, String), String>,
    logs: Vec<String>,
    step_count: usize,
    max_steps: usize,
}

impl Default for JsRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl JsRuntime {
    pub fn new() -> Self {
        let mut globals = HashMap::new();
        globals.insert("document".to_string(), JsValue::String("[object Document]".into()));
        globals.insert("console".to_string(), JsValue::String("[object Console]".into()));

        Self {
            globals,
            locals: Vec::new(),
            functions: HashMap::new(),
            event_listeners: HashMap::new(),
            logs: Vec::new(),
            step_count: 0,
            max_steps: 10_000,
        }
    }

    pub fn set_step_limit(&mut self, limit: usize) {
        self.max_steps = limit;
    }

    pub fn get_logs(&self) -> &[String] {
        &self.logs
    }

    pub fn get_var(&self, name: &str) -> Option<&JsValue> {
        for scope in self.locals.iter().rev() {
            if let Some(val) = scope.get(name) {
                return Some(val);
            }
        }
        self.globals.get(name)
    }

    pub fn set_var(&mut self, name: &str, val: JsValue) {
        for scope in self.locals.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), val);
                return;
            }
        }
        self.globals.insert(name.to_string(), val);
    }

    fn check_step(&mut self) -> Result<(), JsError> {
        self.step_count += 1;
        if self.step_count > self.max_steps {
            Err(JsError::StepLimitExceeded)
        } else {
            Ok(())
        }
    }

    /// Evaluates JS code within the sandboxed runtime, updating the DOM tree where requested.
    pub fn eval(&mut self, code: &str, dom: &mut DomTree) -> Result<JsValue, JsError> {
        self.step_count = 0;
        let stmts = JsParser::parse(code)?;
        let mut last_val = JsValue::Undefined;
        for stmt in &stmts {
            if let Some(val) = self.execute_stmt(stmt, dom)? {
                last_val = val;
            }
        }
        Ok(last_val)
    }

    /// Dispatches an event (e.g. "click") to an element, executing registered listener or inline handler.
    pub fn dispatch_event(&mut self, node: NodeId, event_name: &str, dom: &mut DomTree) -> Result<bool, JsError> {
        self.step_count = 0;
        // 1. Check registered listeners
        let handler_name = self.event_listeners.get(&(node, event_name.to_string())).cloned();
        if let Some(fn_name) = handler_name {
            if let Some((params, body)) = self.functions.get(&fn_name).cloned() {
                self.locals.push(HashMap::new());
                for param in params {
                    self.set_var(&param, JsValue::Element(node));
                }
                for stmt in &body {
                    self.execute_stmt(stmt, dom)?;
                }
                self.locals.pop();
                return Ok(true);
            }
        }

        // 2. Check inline onclick attribute on the DOM node
        if let Some(dom_node) = dom.get(node) {
            let attr_name = format!("on{}", event_name);
            if let Some(inline_code) = dom_node.get_attribute(&attr_name) {
                let code = inline_code.to_string();
                self.eval(&code, dom)?;
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn execute_stmt(&mut self, stmt: &Stmt, dom: &mut DomTree) -> Result<Option<JsValue>, JsError> {
        self.check_step()?;
        match stmt {
            Stmt::Var(name, init_expr) => {
                let val = if let Some(expr) = init_expr {
                    self.eval_expr(expr, dom)?
                } else {
                    JsValue::Undefined
                };
                if let Some(current_scope) = self.locals.last_mut() {
                    current_scope.insert(name.clone(), val);
                } else {
                    self.globals.insert(name.clone(), val);
                }
                Ok(None)
            }
            Stmt::Expr(expr) => {
                let val = self.eval_expr(expr, dom)?;
                Ok(Some(val))
            }
            Stmt::If(cond, then_branch, else_branch) => {
                let cond_val = self.eval_expr(cond, dom)?;
                if cond_val.is_truthy() {
                    for s in then_branch {
                        if let Some(ret) = self.execute_stmt(s, dom)? {
                            return Ok(Some(ret));
                        }
                    }
                } else if let Some(else_stmts) = else_branch {
                    for s in else_stmts {
                        if let Some(ret) = self.execute_stmt(s, dom)? {
                            return Ok(Some(ret));
                        }
                    }
                }
                Ok(None)
            }
            Stmt::While(cond, body) => {
                while self.eval_expr(cond, dom)?.is_truthy() {
                    self.check_step()?;
                    for s in body {
                        if let Some(ret) = self.execute_stmt(s, dom)? {
                            return Ok(Some(ret));
                        }
                    }
                }
                Ok(None)
            }
            Stmt::FunctionDecl(name, params, body) => {
                self.functions.insert(name.clone(), (params.clone(), body.clone()));
                self.globals.insert(name.clone(), JsValue::String(format!("[Function: {}]", name)));
                Ok(None)
            }
            Stmt::Return(expr_opt) => {
                let val = if let Some(e) = expr_opt {
                    self.eval_expr(e, dom)?
                } else {
                    JsValue::Undefined
                };
                Ok(Some(val))
            }
        }
    }

    fn eval_expr(&mut self, expr: &Expr, dom: &mut DomTree) -> Result<JsValue, JsError> {
        self.check_step()?;
        match expr {
            Expr::Literal(val) => Ok(val.clone()),
            Expr::Ident(name) => {
                if let Some(val) = self.get_var(name) {
                    Ok(val.clone())
                } else {
                    Ok(JsValue::Undefined)
                }
            }
            Expr::Assign(name, rhs_expr) => {
                let val = self.eval_expr(rhs_expr, dom)?;
                self.set_var(name, val.clone());
                Ok(val)
            }
            Expr::Binary(left, op, right) => {
                let l_val = self.eval_expr(left, dom)?;
                let r_val = self.eval_expr(right, dom)?;
                match op {
                    BinaryOp::Add => {
                        // If either is string -> string concatenation
                        if matches!(l_val, JsValue::String(_)) || matches!(r_val, JsValue::String(_)) {
                            let mut res = l_val.to_string_val();
                            res.push_str(&r_val.to_string_val());
                            Ok(JsValue::String(res))
                        } else {
                            let res = l_val.to_number() + r_val.to_number();
                            Ok(JsValue::Number(res))
                        }
                    }
                    BinaryOp::Sub => Ok(JsValue::Number(l_val.to_number() - r_val.to_number())),
                    BinaryOp::Mul => Ok(JsValue::Number(l_val.to_number() * r_val.to_number())),
                    BinaryOp::Div => Ok(JsValue::Number(l_val.to_number() / r_val.to_number())),
                    BinaryOp::Eq | BinaryOp::StrictEq => Ok(JsValue::Boolean(l_val == r_val)),
                    BinaryOp::NotEq | BinaryOp::StrictNotEq => Ok(JsValue::Boolean(l_val != r_val)),
                    BinaryOp::Lt => Ok(JsValue::Boolean(l_val.to_number() < r_val.to_number())),
                    BinaryOp::LtEq => Ok(JsValue::Boolean(l_val.to_number() <= r_val.to_number())),
                    BinaryOp::Gt => Ok(JsValue::Boolean(l_val.to_number() > r_val.to_number())),
                    BinaryOp::GtEq => Ok(JsValue::Boolean(l_val.to_number() >= r_val.to_number())),
                }
            }
            Expr::Member(obj_expr, prop) => {
                let obj = self.eval_expr(obj_expr, dom)?;
                match obj {
                    JsValue::Element(id) => {
                        if prop == "innerText" || prop == "textContent" {
                            let text = dom.text_content(id);
                            Ok(JsValue::String(text))
                        } else if prop == "id" {
                            let id_str = dom.get(id).and_then(|n| n.id_attr()).unwrap_or("").to_string();
                            Ok(JsValue::String(id_str))
                        } else if prop == "className" {
                            let class_str = dom.get(id).and_then(|n| n.class_attr()).unwrap_or("").to_string();
                            Ok(JsValue::String(class_str))
                        } else if prop == "style" {
                            Ok(JsValue::String(format!("[Style of Element#{}]", id.index())))
                        } else {
                            Ok(JsValue::Undefined)
                        }
                    }
                    _ => Ok(JsValue::Undefined),
                }
            }
            Expr::MemberAssign(obj_expr, prop, val_expr) => {
                let val = self.eval_expr(val_expr, dom)?;
                // Check if obj_expr is member access like elem.style
                if let Expr::Member(inner_obj_expr, inner_prop) = obj_expr.as_ref() {
                    if inner_prop == "style" {
                        let inner_val = self.eval_expr(inner_obj_expr, dom)?;
                        if let JsValue::Element(elem_id) = inner_val {
                            let css_val = val.to_string_val();
                            // Update style attribute in DOM: e.g. color: #ff0000;
                            let current_style = dom.get(elem_id).and_then(|n| n.get_attribute("style")).unwrap_or("").to_string();
                            let css_prop = if prop == "color" {
                                "color"
                            } else if prop == "backgroundColor" {
                                "background-color"
                            } else if prop == "fontSize" {
                                "font-size"
                            } else {
                                prop.as_str()
                            };
                            let new_style = format!("{}; {}: {};", current_style, css_prop, css_val);
                            let _ = dom.set_attribute(elem_id, "style", &new_style);
                            return Ok(val);
                        }
                    }
                }

                let obj = self.eval_expr(obj_expr, dom)?;
                if let JsValue::Element(elem_id) = obj {
                    if prop == "innerText" || prop == "textContent" {
                        let text = val.to_string_val();
                        let _ = dom.set_text_content(elem_id, &text);
                    } else if prop == "className" {
                        let _ = dom.set_attribute(elem_id, "class", &val.to_string_val());
                    } else if prop == "onclick" {
                        let handler = val.to_string_val();
                        let _ = dom.set_attribute(elem_id, "onclick", &handler);
                    }
                }
                Ok(val)
            }
            Expr::Call(callee, args) => {
                // Check for document.getElementById("id")
                if let Expr::Member(obj_expr, method_name) = callee.as_ref() {
                    let obj = self.eval_expr(obj_expr, dom)?;
                    if let JsValue::String(s) = &obj {
                        if s == "[object Document]" && method_name == "getElementById" {
                            if let Some(first_arg) = args.first() {
                                let id_val = self.eval_expr(first_arg, dom)?.to_string_val();
                                if let Some(node_id) = dom.find_by_id(&id_val) {
                                    return Ok(JsValue::Element(node_id));
                                } else {
                                    return Ok(JsValue::Null);
                                }
                            }
                        } else if s == "[object Console]" && method_name == "log" {
                            let mut line = String::new();
                            for (i, a) in args.iter().enumerate() {
                                if i > 0 { line.push(' '); }
                                line.push_str(&self.eval_expr(a, dom)?.to_string_val());
                            }
                            self.logs.push(line);
                            return Ok(JsValue::Undefined);
                        }
                    }

                    // Check for element.addEventListener("click", handler), setAttribute, getAttribute
                    if let JsValue::Element(elem_id) = obj {
                        if method_name == "addEventListener" && args.len() >= 2 {
                            let ev = self.eval_expr(&args[0], dom)?.to_string_val();
                            let handler = match &args[1] {
                                Expr::Ident(id_name) => id_name.clone(),
                                other => self.eval_expr(other, dom)?.to_string_val(),
                            };
                            self.event_listeners.insert((elem_id, ev), handler);
                            return Ok(JsValue::Undefined);
                        }
                        if method_name == "setAttribute" && args.len() >= 2 {
                            let attr_name = self.eval_expr(&args[0], dom)?.to_string_val();
                            let attr_val = self.eval_expr(&args[1], dom)?.to_string_val();
                            let _ = dom.set_attribute(elem_id, &attr_name, &attr_val);
                            return Ok(JsValue::Undefined);
                        }
                        if method_name == "getAttribute" && !args.is_empty() {
                            let attr_name = self.eval_expr(&args[0], dom)?.to_string_val();
                            let val = dom.get(elem_id).and_then(|n| n.get_attribute(&attr_name)).unwrap_or("").to_string();
                            return Ok(JsValue::String(val));
                        }
                    }
                }

                // Check function call by name
                if let Expr::Ident(fn_name) = callee.as_ref() {
                    if let Some((params, body)) = self.functions.get(fn_name).cloned() {
                        let mut evaled_args = Vec::new();
                        for a in args {
                            evaled_args.push(self.eval_expr(a, dom)?);
                        }
                        self.locals.push(HashMap::new());
                        for (i, param) in params.iter().enumerate() {
                            let arg_val = evaled_args.get(i).cloned().unwrap_or(JsValue::Undefined);
                            self.set_var(param, arg_val);
                        }
                        let mut ret_val = JsValue::Undefined;
                        for stmt in &body {
                            if let Some(ret) = self.execute_stmt(stmt, dom)? {
                                ret_val = ret;
                                break;
                            }
                        }
                        self.locals.pop();
                        return Ok(ret_val);
                    }
                }

                Ok(JsValue::Undefined)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jbe_core::EngineLimits;

    #[test]
    fn test_js_arithmetic_and_variables() {
        let mut dom = DomTree::new(EngineLimits::default());
        let mut rt = JsRuntime::new();
        let code = r#"
            var x = 10;
            let y = 20;
            var z = x + y * 2;
            z;
        "#;
        let res = rt.eval(code, &mut dom).unwrap();
        assert_eq!(res, JsValue::Number(50.0));
        assert_eq!(rt.get_var("z"), Some(&JsValue::Number(50.0)));
    }

    #[test]
    fn test_js_string_concatenation_and_console() {
        let mut dom = DomTree::new(EngineLimits::default());
        let mut rt = JsRuntime::new();
        let code = r#"
            var msg = "Count: " + 42;
            console.log(msg);
        "#;
        rt.eval(code, &mut dom).unwrap();
        assert_eq!(rt.get_logs(), &["Count: 42"]);
    }

    #[test]
    fn test_js_dom_manipulation() {
        let mut dom = DomTree::new(EngineLimits::default());
        let div = dom
            .create_element(
                "div",
                vec![
                    ("id".into(), "counter".into()),
                    ("class".into(), "badge".into()),
                ],
            )
            .unwrap();
        dom.append_child(dom.root(), div).unwrap();
        dom.set_text_content(div, "0").unwrap();

        let mut rt = JsRuntime::new();
        let code = r##"
            var elem = document.getElementById("counter");
            elem.innerText = "Value: 99";
            elem.style.color = "#00ffaa";
        "##;
        rt.eval(code, &mut dom).unwrap();

        assert_eq!(dom.text_content(div), "Value: 99");
        let div_node = dom.get(div).unwrap();
        assert!(div_node.get_attribute("style").unwrap().contains("color: #00ffaa"));
    }

    #[test]
    fn test_js_event_dispatch() {
        let mut dom = DomTree::new(EngineLimits::default());
        let btn = dom
            .create_element(
                "button",
                vec![
                    ("id".into(), "click-me".into()),
                    ("onclick".into(), "count = count + 1;".into()),
                ],
            )
            .unwrap();
        dom.append_child(dom.root(), btn).unwrap();

        let mut rt = JsRuntime::new();
        rt.eval("var count = 0;", &mut dom).unwrap();

        // Dispatch click event
        let handled = rt.dispatch_event(btn, "click", &mut dom).unwrap();
        assert!(handled);
        assert_eq!(rt.get_var("count"), Some(&JsValue::Number(1.0)));

        // Dispatch again
        rt.dispatch_event(btn, "click", &mut dom).unwrap();
        assert_eq!(rt.get_var("count"), Some(&JsValue::Number(2.0)));
    }

    #[test]
    fn test_js_step_limit_prevents_infinite_loop() {
        let mut dom = DomTree::new(EngineLimits::default());
        let mut rt = JsRuntime::new();
        rt.set_step_limit(100);
        let code = "while (true) {}";
        let err = rt.eval(code, &mut dom);
        assert_eq!(err, Err(JsError::StepLimitExceeded));
    }
}
