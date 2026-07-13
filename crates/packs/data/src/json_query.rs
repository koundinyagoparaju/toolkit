//! JSONPath per RFC 9535, hand-rolled: strict parser, filter
//! expressions with the spec's typing rules, and the five standard
//! functions — match()/search() over I-Regexp (RFC 9485) translated to
//! the already-vetted `regex` crate. Verified against the official
//! compliance test suite (tests/cts.json).

use serde_json::Value;
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct JsonQuery;

impl Tool for JsonQuery {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "json-query".into(),
            label: "JSON Query".into(),
            description: "Query JSON with JSONPath (RFC 9535) and get every match as a JSON \
                          array: `$.users[*].name`, slices, `..` descent, and filters like \
                          `$[?@.price < 10]` with length/count/match/search/value functions."
                .into(),
            keywords: [
                "json", "query", "jsonpath", "path", "extract", "filter", "select", "pick",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Json,
                r#"{"users":[{"name":"Ada","admin":true},{"name":"Alan","admin":false}]}"#,
            ),
            output: DataType::Json,
            streaming: false,
            options: vec![OptionSpec::string(
                "query",
                "Query",
                "JSONPath (RFC 9535). Must start with `$`. Returns a JSON array of every match.",
            )
            .default_value("$..name".into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Json(root) = inputs.sole() else {
            unreachable!()
        };
        let query = options.str_opt("query").unwrap_or("$");
        let parsed = parse(query).map_err(|e| ToolError::new(format!("bad query: {e}")))?;
        let nodes = eval_segments(&parsed.segments, &root, &root);
        Ok(DataValue::Json(Value::Array(
            nodes.into_iter().cloned().collect(),
        )))
    }
}

// ---------------------------------------------------------------- AST

struct Query {
    segments: Vec<Segment>,
}

enum Segment {
    Child(Vec<Selector>),
    Descendant(Vec<Selector>),
}

enum Selector {
    Name(String),
    Wildcard,
    Index(i64),
    Slice {
        start: Option<i64>,
        end: Option<i64>,
        step: Option<i64>,
    },
    Filter(LogicalExpr),
}

enum LogicalExpr {
    Or(Box<LogicalExpr>, Box<LogicalExpr>),
    And(Box<LogicalExpr>, Box<LogicalExpr>),
    Not(Box<LogicalExpr>),
    Comparison(Comparable, CmpOp, Comparable),
    /// Existence test of a sub-query.
    Test {
        relative: bool,
        segments: Vec<Segment>,
    },
    /// A LogicalType function used as a test (match/search).
    FunctionTest(FunctionExpr),
}

#[derive(Clone, Copy, PartialEq)]
enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

enum Comparable {
    Literal(Value),
    /// Singular query: name/index segments only.
    Singular {
        relative: bool,
        path: Vec<SingularSeg>,
    },
    /// A ValueType function (length/count/value).
    Function(FunctionExpr),
}

enum SingularSeg {
    Name(String),
    Index(i64),
}

struct FunctionExpr {
    name: String,
    args: Vec<FunctionArg>,
}

enum FunctionArg {
    Literal(Value),
    Query {
        relative: bool,
        segments: Vec<Segment>,
    },
    Singular {
        relative: bool,
        path: Vec<SingularSeg>,
    },
    Function(FunctionExpr),
}

/// Declared types per RFC 9535 §2.4.1.
#[derive(PartialEq, Clone, Copy, Debug)]
enum FnType {
    ValueT,
    LogicalT,
    NodesT,
}

fn function_signature(name: &str) -> Option<(&'static [FnType], FnType)> {
    use FnType::*;
    match name {
        "length" => Some((&[ValueT], ValueT)),
        "count" => Some((&[NodesT], ValueT)),
        "match" | "search" => Some((&[ValueT, ValueT], LogicalT)),
        "value" => Some((&[NodesT], ValueT)),
        _ => None,
    }
}

// ------------------------------------------------------------- parser

const MAX_SAFE: i64 = 9007199254740991; // 2^53 - 1 (I-JSON exact range)

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

type PResult<T> = Result<T, String>;

fn parse(query: &str) -> PResult<Query> {
    let mut p = Parser {
        src: query.as_bytes(),
        pos: 0,
    };
    p.expect(b'$')?;
    let segments = p.segments()?;
    if p.pos != p.src.len() {
        return Err(format!("unexpected trailing input at {}", p.pos));
    }
    Ok(Query { segments })
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }
    fn bump(&mut self) -> Option<u8> {
        let b = self.peek();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }
    fn expect(&mut self, b: u8) -> PResult<()> {
        if self.peek() == Some(b) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at {}", b as char, self.pos))
        }
    }
    fn eat(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn eat_str(&mut self, s: &str) -> bool {
        if self.src[self.pos..].starts_with(s.as_bytes()) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }
    /// Blank space: space, tab, LF, CR.
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    /// Segments of a query (used for the root query and sub-queries).
    fn segments(&mut self) -> PResult<Vec<Segment>> {
        let mut segments = Vec::new();
        loop {
            let mark = self.pos;
            self.skip_ws();
            match self.peek() {
                Some(b'.') => {
                    self.pos += 1;
                    if self.eat(b'.') {
                        // descendant: ..name | ..* | ..[...]
                        match self.peek() {
                            Some(b'[') => {
                                segments.push(Segment::Descendant(self.bracketed()?));
                            }
                            Some(b'*') => {
                                self.pos += 1;
                                segments.push(Segment::Descendant(vec![Selector::Wildcard]));
                            }
                            _ => {
                                let name = self.member_name()?;
                                segments.push(Segment::Descendant(vec![Selector::Name(name)]));
                            }
                        }
                    } else if self.eat(b'*') {
                        segments.push(Segment::Child(vec![Selector::Wildcard]));
                    } else {
                        let name = self.member_name()?;
                        segments.push(Segment::Child(vec![Selector::Name(name)]));
                    }
                }
                Some(b'[') => segments.push(Segment::Child(self.bracketed()?)),
                _ => {
                    // No further segment: the blank space we skipped
                    // belongs to the caller (e.g. before `&&`).
                    self.pos = mark;
                    return Ok(segments);
                }
            }
        }
    }

    /// `name` after `.` or `..`: shorthand member names.
    fn member_name(&mut self) -> PResult<String> {
        let start = self.pos;
        let rest = &self.src[self.pos..];
        let text = core::str::from_utf8(rest).map_err(|_| "invalid UTF-8".to_string())?;
        let mut chars = text.char_indices();
        match chars.next() {
            Some((_, c)) if c.is_ascii_alphabetic() || c == '_' || (c as u32) >= 0x80 => {}
            _ => return Err(format!("expected member name at {start}")),
        }
        let mut end = text.len();
        for (i, c) in chars {
            if !(c.is_ascii_alphanumeric() || c == '_' || (c as u32) >= 0x80) {
                end = i;
                break;
            }
        }
        self.pos += end;
        Ok(text[..end].to_string())
    }

    /// `[selector, selector, ...]` — at least one.
    fn bracketed(&mut self) -> PResult<Vec<Selector>> {
        self.expect(b'[')?;
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            out.push(self.selector()?);
            self.skip_ws();
            if self.eat(b',') {
                continue;
            }
            self.expect(b']')?;
            return Ok(out);
        }
    }

    fn selector(&mut self) -> PResult<Selector> {
        match self.peek() {
            Some(b'\'' | b'"') => Ok(Selector::Name(self.string_literal()?)),
            Some(b'*') => {
                self.pos += 1;
                Ok(Selector::Wildcard)
            }
            Some(b'?') => {
                self.pos += 1;
                self.skip_ws();
                Ok(Selector::Filter(self.logical_or()?))
            }
            _ => self.index_or_slice(),
        }
    }

    fn index_or_slice(&mut self) -> PResult<Selector> {
        // A slice may start with ':' (empty start).
        let start = if matches!(self.peek(), Some(b':')) {
            None
        } else {
            Some(self.int()?)
        };
        let mark = self.pos;
        self.skip_ws();
        if !self.eat(b':') {
            self.pos = mark;
            return match start {
                Some(i) => Ok(Selector::Index(i)),
                None => Err(format!("expected selector at {}", self.pos)),
            };
        }
        self.skip_ws();
        let end = match self.peek() {
            Some(b'0'..=b'9' | b'-') => Some(self.int()?),
            _ => None,
        };
        let mark = self.pos;
        self.skip_ws();
        if self.eat(b':') {
            self.skip_ws();
            let step = match self.peek() {
                Some(b'0'..=b'9' | b'-') => Some(self.int()?),
                _ => None,
            };
            Ok(Selector::Slice { start, end, step })
        } else {
            self.pos = mark;
            Ok(Selector::Slice {
                start,
                end,
                step: None,
            })
        }
    }

    /// Strict I-JSON integer: no leading zeros, no "-0", 2^53 bounds.
    fn int(&mut self) -> PResult<i64> {
        let start = self.pos;
        let neg = self.eat(b'-');
        let digits_start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let digits = &self.src[digits_start..self.pos];
        if digits.is_empty() {
            return Err(format!("expected integer at {start}"));
        }
        if digits.len() > 1 && digits[0] == b'0' {
            return Err("leading zeros are not allowed".into());
        }
        if neg && digits == b"0" {
            return Err("-0 is not allowed".into());
        }
        let text = core::str::from_utf8(&self.src[start..self.pos]).expect("digits");
        let value: i64 = text.parse().map_err(|_| "integer overflows".to_string())?;
        if !(-MAX_SAFE..=MAX_SAFE).contains(&value) {
            return Err("integer outside the interoperable range".into());
        }
        Ok(value)
    }

    /// Quoted string literal with JSON-style escapes (both quote kinds).
    fn string_literal(&mut self) -> PResult<String> {
        let quote = self.bump().expect("caller saw a quote");
        let mut out = String::new();
        loop {
            let rest = core::str::from_utf8(&self.src[self.pos..])
                .map_err(|_| "invalid UTF-8".to_string())?;
            let mut chars = rest.chars();
            let c = chars.next().ok_or("unterminated string")?;
            self.pos += c.len_utf8();
            match c {
                c if c as u32 == quote as u32 => return Ok(out),
                '\\' => {
                    let e = self.bump().ok_or("unterminated escape")? as char;
                    match e {
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        '/' => out.push('/'),
                        '\\' => out.push('\\'),
                        '\'' if quote == b'\'' => out.push('\''),
                        '"' if quote == b'"' => out.push('"'),
                        'u' => out.push(self.unicode_escape()?),
                        _ => return Err(format!("invalid escape '\\{e}'")),
                    }
                }
                c if (c as u32) < 0x20 => return Err("unescaped control character".into()),
                c => out.push(c),
            }
        }
    }

    fn unicode_escape(&mut self) -> PResult<char> {
        let hex4 = |p: &mut Self| -> PResult<u32> {
            let mut v = 0u32;
            for _ in 0..4 {
                let d = p.bump().ok_or("truncated \\u escape")? as char;
                v = v * 16 + d.to_digit(16).ok_or("bad \\u escape")?;
            }
            Ok(v)
        };
        let hi = hex4(self)?;
        if (0xD800..0xDC00).contains(&hi) {
            if !(self.eat(b'\\') && self.eat(b'u')) {
                return Err("lone high surrogate".into());
            }
            let lo = hex4(self)?;
            if !(0xDC00..0xE000).contains(&lo) {
                return Err("invalid low surrogate".into());
            }
            let c = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
            char::from_u32(c).ok_or_else(|| "invalid surrogate pair".into())
        } else if (0xDC00..0xE000).contains(&hi) {
            Err("lone low surrogate".into())
        } else {
            char::from_u32(hi).ok_or_else(|| "invalid \\u escape".into())
        }
    }

    // ---- filter expressions

    fn logical_or(&mut self) -> PResult<LogicalExpr> {
        let mut left = self.logical_and()?;
        loop {
            let mark = self.pos;
            self.skip_ws();
            if self.eat_str("||") {
                self.skip_ws();
                let right = self.logical_and()?;
                left = LogicalExpr::Or(Box::new(left), Box::new(right));
            } else {
                self.pos = mark;
                return Ok(left);
            }
        }
    }

    fn logical_and(&mut self) -> PResult<LogicalExpr> {
        let mut left = self.basic_expr()?;
        loop {
            let mark = self.pos;
            self.skip_ws();
            if self.eat_str("&&") {
                self.skip_ws();
                let right = self.basic_expr()?;
                left = LogicalExpr::And(Box::new(left), Box::new(right));
            } else {
                self.pos = mark;
                return Ok(left);
            }
        }
    }

    fn basic_expr(&mut self) -> PResult<LogicalExpr> {
        if self.eat(b'!') {
            self.skip_ws();
            // Negation applies to a paren-expr or a test-expr only.
            if self.eat(b'(') {
                self.skip_ws();
                let inner = self.logical_or()?;
                self.skip_ws();
                self.expect(b')')?;
                return Ok(LogicalExpr::Not(Box::new(inner)));
            }
            let test = self.test_only()?;
            return Ok(LogicalExpr::Not(Box::new(test)));
        }
        if self.eat(b'(') {
            self.skip_ws();
            let inner = self.logical_or()?;
            self.skip_ws();
            self.expect(b')')?;
            return Ok(inner);
        }
        // Either a comparison or a test: parse one operand, then decide.
        let mark = self.pos;
        let first = self.first_operand()?;
        self.skip_ws();
        if let Some(op) = self.comparison_op() {
            self.skip_ws();
            let right = self.comparable()?;
            let left = match first {
                Operand::Comparable(c) => c,
                Operand::NonSingularQuery => {
                    return Err(format!("only singular queries may be compared (at {mark})"))
                }
            };
            check_comparable(&left)?;
            check_comparable(&right)?;
            return Ok(LogicalExpr::Comparison(left, op, right));
        }
        // No operator: the operand must be a valid test-expr.
        self.pos = mark;
        self.test_or_function()
    }

    /// A test-expr: query existence or a LogicalType function call.
    fn test_only(&mut self) -> PResult<LogicalExpr> {
        self.test_or_function()
    }

    fn test_or_function(&mut self) -> PResult<LogicalExpr> {
        match self.peek() {
            Some(b'@') | Some(b'$') => {
                let relative = self.bump() == Some(b'@');
                let segments = self.segments()?;
                Ok(LogicalExpr::Test { relative, segments })
            }
            Some(c) if c.is_ascii_lowercase() => {
                let f = self.function()?;
                let (_, ret) = function_signature(&f.name).expect("validated");
                if ret != FnType::LogicalT {
                    return Err(format!(
                        "function {}() used as a test must return LogicalType",
                        f.name
                    ));
                }
                Ok(LogicalExpr::FunctionTest(f))
            }
            _ => Err(format!("expected test expression at {}", self.pos)),
        }
    }

    fn comparison_op(&mut self) -> Option<CmpOp> {
        for (text, op) in [
            ("==", CmpOp::Eq),
            ("!=", CmpOp::Ne),
            ("<=", CmpOp::Le),
            (">=", CmpOp::Ge),
            ("<", CmpOp::Lt),
            (">", CmpOp::Gt),
        ] {
            if self.eat_str(text) {
                return Some(op);
            }
        }
        None
    }

    /// The first operand of a maybe-comparison. Queries here may be
    /// non-singular (still fine if no operator follows — it's a test).
    fn first_operand(&mut self) -> PResult<Operand> {
        match self.peek() {
            Some(b'@') | Some(b'$') => {
                let relative = self.bump() == Some(b'@');
                let segments = self.segments()?;
                Ok(match to_singular_ref(&segments) {
                    Some(path) => Operand::Comparable(Comparable::Singular { relative, path }),
                    None => Operand::NonSingularQuery,
                })
            }
            _ => Ok(Operand::Comparable(self.comparable()?)),
        }
    }

    /// One side of a comparison.
    fn comparable(&mut self) -> PResult<Comparable> {
        match self.peek() {
            Some(b'\'' | b'"') => Ok(Comparable::Literal(Value::String(self.string_literal()?))),
            Some(b'@') | Some(b'$') => {
                let relative = self.bump() == Some(b'@');
                let segments = self.segments()?;
                let path = to_singular(segments)
                    .ok_or("only singular queries may be compared".to_string())?;
                Ok(Comparable::Singular { relative, path })
            }
            Some(b't') if self.eat_str("true") => Ok(Comparable::Literal(Value::Bool(true))),
            Some(b'f') if self.eat_str("false") => Ok(Comparable::Literal(Value::Bool(false))),
            Some(b'n') if self.eat_str("null") => Ok(Comparable::Literal(Value::Null)),
            Some(c) if c.is_ascii_lowercase() => Ok(Comparable::Function(self.function()?)),
            _ => Ok(Comparable::Literal(self.number_literal()?)),
        }
    }

    /// JSON number (int or float with fraction/exponent), I-JSON ints.
    fn number_literal(&mut self) -> PResult<Value> {
        let start = self.pos;
        self.eat(b'-');
        let int_start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let int_digits_len = self.pos - int_start;
        let int_leading_zero = int_digits_len > 1 && self.src[int_start] == b'0';
        if int_digits_len == 0 {
            return Err(format!("expected number at {start}"));
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            let frac_start = self.pos;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
            if self.pos == frac_start {
                return Err("digits required after decimal point".into());
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let exp_start = self.pos;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
            if self.pos == exp_start {
                return Err("digits required in exponent".into());
            }
        }
        if int_leading_zero {
            return Err("leading zeros are not allowed".into());
        }
        let text = core::str::from_utf8(&self.src[start..self.pos]).expect("ascii");
        if !is_float {
            if text == "-0" {
                // Valid as a comparison literal (unlike an index).
                return Ok(Value::Number(
                    serde_json::Number::from_f64(-0.0).expect("finite"),
                ));
            }
            let v: i64 = text.parse().map_err(|_| "integer overflows".to_string())?;
            if !(-MAX_SAFE..=MAX_SAFE).contains(&v) {
                return Err("integer outside the interoperable range".into());
            }
            return Ok(Value::Number(v.into()));
        }
        let v: f64 = text.parse().map_err(|_| "bad number".to_string())?;
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| "number out of range".into())
    }

    fn function(&mut self) -> PResult<FunctionExpr> {
        let start = self.pos;
        while matches!(self.peek(), Some(b'a'..=b'z' | b'0'..=b'9' | b'_')) {
            self.pos += 1;
        }
        let name = core::str::from_utf8(&self.src[start..self.pos])
            .expect("ascii")
            .to_string();
        let (params, _) =
            function_signature(&name).ok_or(format!("unknown function \"{name}\""))?;
        self.expect(b'(')?;
        let mut args = Vec::new();
        self.skip_ws();
        if !self.eat(b')') {
            loop {
                args.push(self.function_arg()?);
                self.skip_ws();
                if self.eat(b',') {
                    self.skip_ws();
                    continue;
                }
                self.expect(b')')?;
                break;
            }
        }
        if args.len() != params.len() {
            return Err(format!(
                "{name}() takes {} argument(s), got {}",
                params.len(),
                args.len()
            ));
        }
        for (arg, want) in args.iter().zip(params) {
            check_arg_type(arg, *want, &name)?;
        }
        Ok(FunctionExpr { name, args })
    }

    fn function_arg(&mut self) -> PResult<FunctionArg> {
        match self.peek() {
            Some(b'\'' | b'"') => Ok(FunctionArg::Literal(Value::String(self.string_literal()?))),
            Some(b'@') | Some(b'$') => {
                let relative = self.bump() == Some(b'@');
                let segments = self.segments()?;
                Ok(match to_singular_ref(&segments) {
                    Some(path) => FunctionArg::Singular { relative, path },
                    None => FunctionArg::Query { relative, segments },
                })
            }
            Some(b't') if self.eat_str("true") => Ok(FunctionArg::Literal(Value::Bool(true))),
            Some(b'f') if self.eat_str("false") => Ok(FunctionArg::Literal(Value::Bool(false))),
            Some(b'n') if self.eat_str("null") => Ok(FunctionArg::Literal(Value::Null)),
            Some(c) if c.is_ascii_lowercase() => Ok(FunctionArg::Function(self.function()?)),
            _ => Ok(FunctionArg::Literal(self.number_literal()?)),
        }
    }
}

enum Operand {
    Comparable(Comparable),
    NonSingularQuery,
}

/// Comparison operands must be ValueType (RFC 9535 well-typedness).
fn check_comparable(c: &Comparable) -> PResult<()> {
    if let Comparable::Function(f) = c {
        let (_, ret) = function_signature(&f.name).expect("validated");
        if ret != FnType::ValueT {
            return Err(format!("function {}() cannot be compared", f.name));
        }
    }
    Ok(())
}

fn check_arg_type(arg: &FunctionArg, want: FnType, fname: &str) -> PResult<()> {
    let ok = match want {
        FnType::ValueT => match arg {
            FunctionArg::Literal(_) | FunctionArg::Singular { .. } => true,
            FunctionArg::Function(f) => {
                function_signature(&f.name).expect("validated").1 == FnType::ValueT
            }
            FunctionArg::Query { .. } => false, // non-singular query is NodesType
        },
        FnType::NodesT => match arg {
            FunctionArg::Query { .. } | FunctionArg::Singular { .. } => true,
            FunctionArg::Function(f) => {
                function_signature(&f.name).expect("validated").1 == FnType::NodesT
            }
            FunctionArg::Literal(_) => false,
        },
        FnType::LogicalT => false, // none of the standard functions take one
    };
    if ok {
        Ok(())
    } else {
        Err(format!("argument of {fname}() has the wrong type"))
    }
}

fn to_singular(segments: Vec<Segment>) -> Option<Vec<SingularSeg>> {
    to_singular_ref(&segments)
}

fn to_singular_ref(segments: &[Segment]) -> Option<Vec<SingularSeg>> {
    let mut path = Vec::new();
    for seg in segments {
        let Segment::Child(selectors) = seg else {
            return None;
        };
        let [one] = selectors.as_slice() else {
            return None;
        };
        match one {
            Selector::Name(n) => path.push(SingularSeg::Name(n.clone())),
            Selector::Index(i) => path.push(SingularSeg::Index(*i)),
            _ => return None,
        }
    }
    Some(path)
}

// ---------------------------------------------------------- evaluator

fn eval_segments<'a>(segments: &[Segment], root: &'a Value, current: &'a Value) -> Vec<&'a Value> {
    let mut nodes = vec![current];
    for segment in segments {
        let mut next = Vec::new();
        match segment {
            Segment::Child(selectors) => {
                for node in &nodes {
                    for sel in selectors {
                        apply_selector(sel, node, root, &mut next);
                    }
                }
            }
            Segment::Descendant(selectors) => {
                for node in &nodes {
                    let mut all = Vec::new();
                    collect_descendants(node, &mut all);
                    for d in all {
                        for sel in selectors {
                            apply_selector(sel, d, root, &mut next);
                        }
                    }
                }
            }
        }
        nodes = next;
    }
    nodes
}

/// The node itself plus every descendant, depth-first document order.
fn collect_descendants<'a>(node: &'a Value, out: &mut Vec<&'a Value>) {
    out.push(node);
    match node {
        Value::Object(map) => {
            for v in map.values() {
                collect_descendants(v, out);
            }
        }
        Value::Array(items) => {
            for v in items {
                collect_descendants(v, out);
            }
        }
        _ => {}
    }
}

fn apply_selector<'a>(sel: &Selector, node: &'a Value, root: &'a Value, out: &mut Vec<&'a Value>) {
    match sel {
        Selector::Name(name) => {
            if let Value::Object(map) = node {
                if let Some(v) = map.get(name) {
                    out.push(v);
                }
            }
        }
        Selector::Wildcard => match node {
            Value::Object(map) => out.extend(map.values()),
            Value::Array(items) => out.extend(items.iter()),
            _ => {}
        },
        Selector::Index(i) => {
            if let Value::Array(items) = node {
                let idx = if *i < 0 { items.len() as i64 + i } else { *i };
                if let Ok(idx) = usize::try_from(idx) {
                    if let Some(v) = items.get(idx) {
                        out.push(v);
                    }
                }
            }
        }
        Selector::Slice { start, end, step } => {
            if let Value::Array(items) = node {
                slice_indices(items.len(), *start, *end, *step, |i| out.push(&items[i]));
            }
        }
        Selector::Filter(expr) => {
            let children: Vec<&Value> = match node {
                Value::Object(map) => map.values().collect(),
                Value::Array(items) => items.iter().collect(),
                _ => return,
            };
            for child in children {
                if eval_logical(expr, root, child) {
                    out.push(child);
                }
            }
        }
    }
}

/// RFC 9535 slice semantics (Python-style, step may be negative).
fn slice_indices(
    len: usize,
    start: Option<i64>,
    end: Option<i64>,
    step: Option<i64>,
    mut push: impl FnMut(usize),
) {
    let len = len as i64;
    let step = step.unwrap_or(1);
    if step == 0 || len == 0 {
        return;
    }
    let normalize = |i: i64| if i >= 0 { i } else { len + i };
    if step > 0 {
        let lower = normalize(start.unwrap_or(0)).clamp(0, len);
        let upper = normalize(end.unwrap_or(len)).clamp(0, len);
        let mut i = lower;
        while i < upper {
            push(i as usize);
            i += step;
        }
    } else {
        let upper = normalize(start.unwrap_or(len - 1)).clamp(-1, len - 1);
        let lower = normalize(end.unwrap_or(-len - 1)).clamp(-1, len - 1);
        let mut i = upper;
        while i > lower {
            push(i as usize);
            i += step;
        }
    }
}

/// The result of evaluating a ValueType expression: a value or Nothing.
type MaybeValue = Option<Value>;

fn eval_logical(expr: &LogicalExpr, root: &Value, current: &Value) -> bool {
    match expr {
        LogicalExpr::Or(a, b) => eval_logical(a, root, current) || eval_logical(b, root, current),
        LogicalExpr::And(a, b) => eval_logical(a, root, current) && eval_logical(b, root, current),
        LogicalExpr::Not(inner) => !eval_logical(inner, root, current),
        LogicalExpr::Test { relative, segments } => {
            let base = if *relative { current } else { root };
            !eval_segments(segments, root, base).is_empty()
        }
        LogicalExpr::FunctionTest(f) => eval_function_logical(f, root, current),
        LogicalExpr::Comparison(left, op, right) => {
            let l = eval_comparable(left, root, current);
            let r = eval_comparable(right, root, current);
            compare(&l, *op, &r)
        }
    }
}

fn eval_comparable(c: &Comparable, root: &Value, current: &Value) -> MaybeValue {
    match c {
        Comparable::Literal(v) => Some(v.clone()),
        Comparable::Singular { relative, path } => {
            eval_singular(path, if *relative { current } else { root }).cloned()
        }
        Comparable::Function(f) => eval_function_value(f, root, current),
    }
}

fn eval_singular<'a>(path: &[SingularSeg], mut node: &'a Value) -> Option<&'a Value> {
    for seg in path {
        node = match (seg, node) {
            (SingularSeg::Name(n), Value::Object(map)) => map.get(n)?,
            (SingularSeg::Index(i), Value::Array(items)) => {
                let idx = if *i < 0 { items.len() as i64 + i } else { *i };
                items.get(usize::try_from(idx).ok()?)?
            }
            _ => return None,
        };
    }
    Some(node)
}

fn nodelist<'a>(arg: &FunctionArg, root: &'a Value, current: &'a Value) -> Vec<&'a Value> {
    match arg {
        FunctionArg::Query { relative, segments } => {
            eval_segments(segments, root, if *relative { current } else { root })
        }
        FunctionArg::Singular { relative, path } => {
            eval_singular(path, if *relative { current } else { root })
                .into_iter()
                .collect()
        }
        _ => Vec::new(),
    }
}

fn arg_value(arg: &FunctionArg, root: &Value, current: &Value) -> MaybeValue {
    match arg {
        FunctionArg::Literal(v) => Some(v.clone()),
        FunctionArg::Singular { relative, path } => {
            eval_singular(path, if *relative { current } else { root }).cloned()
        }
        FunctionArg::Function(f) => eval_function_value(f, root, current),
        FunctionArg::Query { .. } => None,
    }
}

fn eval_function_value(f: &FunctionExpr, root: &Value, current: &Value) -> MaybeValue {
    match f.name.as_str() {
        "length" => match arg_value(&f.args[0], root, current)? {
            Value::String(s) => Some(Value::Number((s.chars().count() as u64).into())),
            Value::Array(a) => Some(Value::Number((a.len() as u64).into())),
            Value::Object(o) => Some(Value::Number((o.len() as u64).into())),
            _ => None,
        },
        "count" => {
            let n = nodelist(&f.args[0], root, current).len();
            Some(Value::Number((n as u64).into()))
        }
        "value" => {
            let nodes = nodelist(&f.args[0], root, current);
            match nodes.as_slice() {
                [one] => Some((*one).clone()),
                _ => None,
            }
        }
        _ => None,
    }
}

fn eval_function_logical(f: &FunctionExpr, root: &Value, current: &Value) -> bool {
    let (Some(Value::String(text)), Some(Value::String(pattern))) = (
        arg_value(&f.args[0], root, current),
        arg_value(&f.args[1], root, current),
    ) else {
        return false;
    };
    let Some(regex) = iregexp_to_regex(&pattern) else {
        return false;
    };
    let full = if f.name == "match" {
        format!("^(?:{regex})$")
    } else {
        regex
    };
    match regex::Regex::new(&full) {
        Ok(re) => re.is_match(&text),
        Err(_) => false,
    }
}

// ------------------------------------------------------- comparisons

fn compare(l: &MaybeValue, op: CmpOp, r: &MaybeValue) -> bool {
    match op {
        CmpOp::Eq => values_equal(l, r),
        CmpOp::Ne => !values_equal(l, r),
        CmpOp::Lt => less_than(l, r),
        CmpOp::Gt => less_than(r, l),
        CmpOp::Le => less_than(l, r) || values_equal(l, r),
        CmpOp::Ge => less_than(r, l) || values_equal(l, r),
    }
}

fn values_equal(l: &MaybeValue, r: &MaybeValue) -> bool {
    match (l, r) {
        (None, None) => true,
        (Some(a), Some(b)) => json_equal(a, b),
        _ => false,
    }
}

fn json_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => number(x) == number(y),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| json_equal(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| json_equal(v, w)))
        }
        _ => a == b,
    }
}

fn less_than(l: &MaybeValue, r: &MaybeValue) -> bool {
    match (l, r) {
        (Some(Value::Number(a)), Some(Value::Number(b))) => number(a) < number(b),
        (Some(Value::String(a)), Some(Value::String(b))) => a < b,
        _ => false,
    }
}

fn number(n: &serde_json::Number) -> f64 {
    n.as_f64().unwrap_or(f64::NAN)
}

// ---------------------------------------------------------- I-Regexp

/// Validate an I-Regexp (RFC 9485) and translate it for the `regex`
/// crate: `.` means [^\n\r], and `^`/`$` are literal characters. Returns
/// None when the pattern isn't valid I-Regexp.
fn iregexp_to_regex(pattern: &str) -> Option<String> {
    let mut out = String::with_capacity(pattern.len() + 8);
    let mut chars = pattern.chars().peekable();
    let mut depth = 0usize;
    while let Some(c) = chars.next() {
        match c {
            '.' => out.push_str("[^\\n\\r]"),
            '^' | '$' => out.push(c),
            '(' => {
                depth += 1;
                out.push_str("(?:");
            }
            ')' => {
                depth = depth.checked_sub(1)?;
                out.push(')');
            }
            '[' => {
                out.push('[');
                translate_class(&mut chars, &mut out)?;
            }
            ']' | '}' => return None, // stray closers
            '{' => {
                // quantifier: {n} or {n,} or {n,m}
                out.push('{');
                let mut any_digit = false;
                let mut closed = false;
                for d in chars.by_ref() {
                    out.push(d);
                    match d {
                        '0'..='9' => any_digit = true,
                        ',' if any_digit => {}
                        '}' if any_digit => {
                            closed = true;
                            break;
                        }
                        _ => return None,
                    }
                }
                if !closed {
                    return None;
                }
            }
            '\\' => {
                let e = chars.next()?;
                match e {
                    '(' | ')' | '*' | '+' | '-' | '.' | '?' | '[' | '\\' | ']' | '^' | 'n'
                    | 'r' | 't' | '{' | '|' | '}' | '$' => {
                        out.push('\\');
                        out.push(e);
                    }
                    'p' | 'P' => {
                        out.push('\\');
                        out.push(e);
                        copy_category(&mut chars, &mut out)?;
                    }
                    _ => return None, // \d \w \s \b etc. are not I-Regexp
                }
            }
            '*' | '+' | '?' | '|' => out.push(c),
            c => out.push(c),
        }
    }
    if depth != 0 {
        return None;
    }
    Some(out)
}

/// `\p{...}` category names: letters, digits, and dashes.
fn copy_category(chars: &mut std::iter::Peekable<std::str::Chars>, out: &mut String) -> Option<()> {
    if chars.next()? != '{' {
        return None;
    }
    out.push('{');
    loop {
        let c = chars.next()?;
        out.push(c);
        if c == '}' {
            return Some(());
        }
        if !(c.is_ascii_alphanumeric() || c == '-') {
            return None;
        }
    }
}

/// Translate the inside of a character class, cursor just after '['.
fn translate_class(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    out: &mut String,
) -> Option<()> {
    if chars.peek() == Some(&'^') {
        out.push('^');
        chars.next();
    }
    let mut any = false;
    loop {
        let c = chars.next()?;
        match c {
            ']' if any => {
                out.push(']');
                return Some(());
            }
            ']' => return None, // empty class
            '\\' => {
                let e = chars.next()?;
                match e {
                    '(' | ')' | '*' | '+' | '-' | '.' | '?' | '[' | '\\' | ']' | '^' | 'n'
                    | 'r' | 't' | '{' | '|' | '}' | '$' => {
                        out.push('\\');
                        out.push(e);
                    }
                    'p' | 'P' => {
                        out.push('\\');
                        out.push(e);
                        copy_category(chars, out)?;
                    }
                    _ => return None,
                }
            }
            '[' => return None, // '[' must be escaped inside a class
            '&' if chars.peek() == Some(&'&') => return None, // no intersections
            c => {
                // The regex crate treats these as literals inside classes
                // too, except '~' style operators handled above.
                out.push(c);
            }
        }
        any = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use toolkit_core::run_single;

    fn query(doc: Value, q: &str) -> Result<Value, ToolError> {
        let mut opts = Options::new();
        opts.insert("query".into(), q.into());
        run_single(&JsonQuery, DataValue::Json(doc), &opts).map(|v| {
            let DataValue::Json(v) = v else {
                unreachable!()
            };
            v
        })
    }

    fn doc() -> Value {
        json!({"users":[{"name":"Ada","admin":true},{"name":"Alan","admin":false}],
               "meta":{"name":"directory","a.b":7}})
    }

    #[test]
    fn paths_wildcards_indexes_and_slices() {
        assert_eq!(
            query(doc(), "$.users[*].name").unwrap(),
            json!(["Ada", "Alan"])
        );
        assert_eq!(query(doc(), "$.users[-1].name").unwrap(), json!(["Alan"]));
        assert_eq!(
            query(json!([0, 1, 2, 3, 4, 5]), "$[1:5:2]").unwrap(),
            json!([1, 3])
        );
        assert_eq!(
            query(json!([0, 1, 2, 3]), "$[::-1]").unwrap(),
            json!([3, 2, 1, 0])
        );
        assert_eq!(
            query(json!({"a": 1, "b": 2}), "$['a', 'b']").unwrap(),
            json!([1, 2])
        );
    }

    #[test]
    fn recursive_descent_and_quoted_keys() {
        assert_eq!(
            query(doc(), "$..name").unwrap(),
            json!(["Ada", "Alan", "directory"])
        );
        assert_eq!(query(doc(), r#"$.meta["a.b"]"#).unwrap(), json!([7]));
    }

    #[test]
    fn filters() {
        // `?@.admin` is an existence test (both users have the key);
        // truthiness needs an explicit comparison.
        assert_eq!(
            query(doc(), "$.users[?@.admin].name").unwrap(),
            json!(["Ada", "Alan"])
        );
        assert_eq!(
            query(doc(), "$.users[?@.admin == true].name").unwrap(),
            json!(["Ada"])
        );
        assert_eq!(
            query(doc(), "$.users[?@.name == 'Alan'].name").unwrap(),
            json!(["Alan"])
        );
        assert_eq!(
            query(json!([1, 5, 9, 12]), "$[?@ > 4 && @ < 10]").unwrap(),
            json!([5, 9])
        );
        assert_eq!(
            query(doc(), "$.users[?length(@.name) == 4].name").unwrap(),
            json!(["Alan"])
        );
        assert_eq!(
            query(doc(), "$.users[?match(@.name, 'A.a')].name").unwrap(),
            json!(["Ada"])
        );
        assert_eq!(
            query(doc(), "$[?count(@..name) > 1]").unwrap(),
            json!([doc()["users"]])
        );
    }

    #[test]
    fn strictness() {
        assert!(query(doc(), " $").is_err());
        assert!(query(doc(), "$ ").is_err());
        assert!(query(doc(), "users").is_err());
        assert!(query(doc(), "$[01]").is_err());
        assert!(query(doc(), "$[?@.a == match(@.b, 'x')]").is_err());
        assert!(query(doc(), "$[?length(@.a)]").is_err());
    }
}
