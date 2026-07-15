use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

/// Evaluate an arithmetic expression exactly — the calculator an agent
/// should reach for instead of doing arithmetic "in its head". Hand-rolled
/// Pratt parser; no dependencies.
pub struct Calc;

impl Tool for Calc {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "calc".into(),
            label: "Calculator".into(),
            description: "Evaluate an arithmetic expression: + - * / % ^ (power), parentheses, \
                          functions (sqrt, ln, log10, log2, exp, abs, floor, ceil, round, \
                          sin/cos/tan and inverses, min, max) and constants pi, e, tau."
                .into(),
            keywords: [
                "calc",
                "calculator",
                "math",
                "arithmetic",
                "evaluate",
                "expression",
                "compute",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "2^10 + sqrt(144) * 3"),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let result = eval(text.trim())?;
        if !result.is_finite() {
            return Err(ToolError::new("the result is not a finite number"));
        }
        Ok(DataValue::Json(serde_json::json!({ "result": result })))
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Ident(String),
    Op(char),
    LParen,
    RParen,
    Comma,
}

fn tokenize(text: &str) -> Result<Vec<Token>, ToolError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            '+' | '-' | '*' | '/' | '%' | '^' => {
                tokens.push(Token::Op(c));
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '0'..='9' | '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                // Scientific notation: 1e9, 2.5e-3.
                if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                    let mut j = i + 1;
                    if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                        j += 1;
                    }
                    if j < chars.len() && chars[j].is_ascii_digit() {
                        i = j;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                }
                let s: String = chars[start..i].iter().collect();
                let n = s
                    .parse::<f64>()
                    .map_err(|_| ToolError::new(format!("\"{s}\" is not a number")))?;
                tokens.push(Token::Number(n));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                tokens.push(Token::Ident(chars[start..i].iter().collect()));
            }
            other => return Err(ToolError::new(format!("unexpected character \"{other}\""))),
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

fn eval(text: &str) -> Result<f64, ToolError> {
    if text.is_empty() {
        return Err(ToolError::new("empty expression"));
    }
    let mut parser = Parser {
        tokens: tokenize(text)?,
        pos: 0,
    };
    let value = parser.expr(0)?;
    if parser.pos < parser.tokens.len() {
        return Err(ToolError::new(format!(
            "unexpected {:?} after the expression",
            parser.tokens[parser.pos]
        )));
    }
    Ok(value)
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Result<Token, ToolError> {
        let t = self
            .tokens
            .get(self.pos)
            .cloned()
            .ok_or_else(|| ToolError::new("the expression ends unexpectedly"))?;
        self.pos += 1;
        Ok(t)
    }

    /// Pratt: parse until an operator binds less tightly than `min_bp`.
    fn expr(&mut self, min_bp: u8) -> Result<f64, ToolError> {
        let mut lhs = self.atom()?;
        while let Some(Token::Op(op)) = self.peek() {
            let op = *op;
            let (lbp, rbp) = match op {
                '+' | '-' => (1, 2),
                '*' | '/' | '%' => (3, 4),
                '^' => (6, 5), // right-associative
                _ => unreachable!("tokenizer only emits the ops above"),
            };
            if lbp < min_bp {
                break;
            }
            self.pos += 1;
            let rhs = self.expr(rbp)?;
            lhs = match op {
                '+' => lhs + rhs,
                '-' => lhs - rhs,
                '*' => lhs * rhs,
                '/' => {
                    if rhs == 0.0 {
                        return Err(ToolError::new("division by zero"));
                    }
                    lhs / rhs
                }
                '%' => {
                    if rhs == 0.0 {
                        return Err(ToolError::new("modulo by zero"));
                    }
                    lhs % rhs
                }
                '^' => lhs.powf(rhs),
                _ => unreachable!(),
            };
        }
        Ok(lhs)
    }

    fn atom(&mut self) -> Result<f64, ToolError> {
        match self.next()? {
            Token::Number(n) => Ok(n),
            Token::Op('-') => Ok(-self.expr(5)?), // binds tighter than * but looser than ^
            Token::Op('+') => self.expr(5),
            Token::LParen => {
                let v = self.expr(0)?;
                self.expect_rparen()?;
                Ok(v)
            }
            Token::Ident(name) => match name.as_str() {
                "pi" => Ok(std::f64::consts::PI),
                "e" => Ok(std::f64::consts::E),
                "tau" => Ok(std::f64::consts::TAU),
                _ => self.call(&name),
            },
            other => Err(ToolError::new(format!("unexpected {other:?}"))),
        }
    }

    fn call(&mut self, name: &str) -> Result<f64, ToolError> {
        match self.next() {
            Ok(Token::LParen) => {}
            _ => {
                return Err(ToolError::new(format!(
                    "unknown name \"{name}\" (functions need parentheses; constants are pi, e, tau)"
                )))
            }
        }
        let mut args = vec![self.expr(0)?];
        while self.peek() == Some(&Token::Comma) {
            self.pos += 1;
            args.push(self.expr(0)?);
        }
        self.expect_rparen()?;

        let one = |args: &[f64]| -> Result<f64, ToolError> {
            match args {
                [x] => Ok(*x),
                _ => Err(ToolError::new(format!("{name} takes one argument"))),
            }
        };
        match name {
            "sqrt" => Ok(one(&args)?.sqrt()),
            "cbrt" => Ok(one(&args)?.cbrt()),
            "abs" => Ok(one(&args)?.abs()),
            "ln" => Ok(one(&args)?.ln()),
            "log10" | "log" => Ok(one(&args)?.log10()),
            "log2" => Ok(one(&args)?.log2()),
            "exp" => Ok(one(&args)?.exp()),
            "floor" => Ok(one(&args)?.floor()),
            "ceil" => Ok(one(&args)?.ceil()),
            "round" => Ok(one(&args)?.round()),
            "sin" => Ok(one(&args)?.sin()),
            "cos" => Ok(one(&args)?.cos()),
            "tan" => Ok(one(&args)?.tan()),
            "asin" => Ok(one(&args)?.asin()),
            "acos" => Ok(one(&args)?.acos()),
            "atan" => Ok(one(&args)?.atan()),
            "min" => args
                .iter()
                .copied()
                .reduce(f64::min)
                .ok_or_else(|| ToolError::new("min takes at least one argument")),
            "max" => args
                .iter()
                .copied()
                .reduce(f64::max)
                .ok_or_else(|| ToolError::new("max takes at least one argument")),
            _ => Err(ToolError::new(format!("unknown function \"{name}\""))),
        }
    }

    fn expect_rparen(&mut self) -> Result<(), ToolError> {
        match self.next()? {
            Token::RParen => Ok(()),
            other => Err(ToolError::new(format!("expected \")\", found {other:?}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calc(expr: &str) -> f64 {
        eval(expr).unwrap()
    }

    #[test]
    fn precedence_and_associativity() {
        assert_eq!(calc("2+3*4"), 14.0);
        assert_eq!(calc("(2+3)*4"), 20.0);
        assert_eq!(calc("2^10"), 1024.0);
        assert_eq!(calc("2^3^2"), 512.0); // right-assoc: 2^(3^2)
        assert_eq!(calc("10%3"), 1.0);
        assert_eq!(calc("100/4/5"), 5.0); // left-assoc
        assert_eq!(calc("-2^2"), -4.0); // -(2^2), the math convention
        assert_eq!(calc("(-2)^2"), 4.0);
        assert_eq!(calc("2*-3"), -6.0);
    }

    #[test]
    fn functions_and_constants() {
        assert_eq!(calc("sqrt(144)"), 12.0);
        assert_eq!(calc("min(3, 1, 2)"), 1.0);
        assert_eq!(calc("max(3, 1, 2)"), 3.0);
        assert_eq!(calc("abs(-5) + floor(2.9)"), 7.0);
        assert_eq!(calc("log10(1000)"), 3.0);
        assert!((calc("sin(pi)")).abs() < 1e-12);
        assert!((calc("ln(e)") - 1.0).abs() < 1e-12);
        assert_eq!(calc("2.5e3 + 1"), 2501.0);
        assert_eq!(calc("2^10 + sqrt(144) * 3"), 1060.0);
    }

    #[test]
    fn errors() {
        for bad in [
            "",
            "2 +",
            "2 + * 3",
            "(2",
            "sqrt",
            "sqrt(1, 2)",
            "nope(1)",
            "1/0",
            "2 @ 3",
            "pie",
            "sqrt(-1)",
        ] {
            assert!(
                eval(bad).is_err() || !eval(bad).unwrap().is_finite(),
                "{bad}"
            );
        }
        // Non-finite results are rejected at the tool level.
        assert!(eval("sqrt(-1)").unwrap().is_nan());
    }
}
