/// All variables available to condition expressions.
#[derive(Debug, Clone, Default)]
pub struct ConditionVars {
    pub cursor_dist: f32,
    pub state_time_ms: u32,
    pub on_surface: bool,
    pub pet_x: f32,
    pub pet_y: f32,
    pub pet_vx: f32,
    pub pet_vy: f32,
    pub pet_v: f32, // pre-computed: sqrt(vx²+vy²)
    pub pet_w: f32,
    pub pet_h: f32,
    pub screen_w: f32,
    pub screen_h: f32,
    pub hour: u32,
    pub focused_app: String,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Literal(Literal),
    Var(Var),
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    UnaryNot(Box<Expr>),
    Call { name: String, args: Vec<Expr> },
}

#[derive(Debug, Clone)]
pub enum Literal {
    Number(f32),
    DurationMs(u32),
    Bool(bool),
    Str(String),
}

#[derive(Debug, Clone)]
pub enum Var {
    CursorDist,
    StateTime,
    OnSurface,
    PetX,
    PetY,
    PetVx,
    PetVy,
    PetV,
    ScreenW,
    ScreenH,
    Hour,
    FocusedApp,
    NearEdge {
        axis: Option<Axis>,
        threshold_px: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Axis {
    X,
    Y,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

// ─── Tokens ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f32),
    DurationMs(u32),
    Bool(bool),
    Str(String),
    Ident(String),
    NearEdge {
        axis: Option<Axis>,
        threshold_px: f32,
    },
    Op(BinOp),
    Not,
    LParen,
    RParen,
    Comma,
}

// ─── Lexer ───────────────────────────────────────────────────────────────────

fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut pos = 0;
    let mut tokens = Vec::new();

    while pos < chars.len() {
        // Skip whitespace
        if chars[pos].is_whitespace() {
            pos += 1;
            continue;
        }

        // String literal "..."
        if chars[pos] == '"' {
            pos += 1;
            let start = pos;
            while pos < chars.len() && chars[pos] != '"' {
                pos += 1;
            }
            if pos >= chars.len() {
                return Err("unterminated string literal".to_string());
            }
            let s: String = chars[start..pos].iter().collect();
            pos += 1; // consume closing "
            tokens.push(Token::Str(s));
            continue;
        }

        // Operators: <=, >=, ==, !=, <, >
        if chars[pos] == '<' {
            if pos + 1 < chars.len() && chars[pos + 1] == '=' {
                tokens.push(Token::Op(BinOp::Le));
                pos += 2;
            } else {
                tokens.push(Token::Op(BinOp::Lt));
                pos += 1;
            }
            continue;
        }
        if chars[pos] == '>' {
            if pos + 1 < chars.len() && chars[pos + 1] == '=' {
                tokens.push(Token::Op(BinOp::Ge));
                pos += 2;
            } else {
                tokens.push(Token::Op(BinOp::Gt));
                pos += 1;
            }
            continue;
        }
        if chars[pos] == '=' && pos + 1 < chars.len() && chars[pos + 1] == '=' {
            tokens.push(Token::Op(BinOp::Eq));
            pos += 2;
            continue;
        }
        if chars[pos] == '!' && pos + 1 < chars.len() && chars[pos + 1] == '=' {
            tokens.push(Token::Op(BinOp::Ne));
            pos += 2;
            continue;
        }

        // Punctuation
        if chars[pos] == '(' {
            tokens.push(Token::LParen);
            pos += 1;
            continue;
        }
        if chars[pos] == ')' {
            tokens.push(Token::RParen);
            pos += 1;
            continue;
        }
        if chars[pos] == ',' {
            tokens.push(Token::Comma);
            pos += 1;
            continue;
        }

        // Number or duration: digits, possibly with decimal point
        if chars[pos].is_ascii_digit() {
            let start = pos;
            while pos < chars.len() && chars[pos].is_ascii_digit() {
                pos += 1;
            }
            let has_decimal = pos < chars.len() && chars[pos] == '.';
            if has_decimal {
                pos += 1;
                while pos < chars.len() && chars[pos].is_ascii_digit() {
                    pos += 1;
                }
            }
            // Check suffix
            // "ms" suffix
            if pos + 1 < chars.len() && chars[pos] == 'm' && chars[pos + 1] == 's' {
                let num_str: String = chars[start..pos].iter().collect();
                let ms: u32 = num_str
                    .parse::<u32>()
                    .map_err(|_| format!("invalid duration: {}", num_str))?;
                tokens.push(Token::DurationMs(ms));
                pos += 2;
            // "s" suffix (seconds → ms)
            } else if pos < chars.len() && chars[pos] == 's' {
                let num_str: String = chars[start..pos].iter().collect();
                let secs: f32 = num_str
                    .parse::<f32>()
                    .map_err(|_| format!("invalid duration: {}", num_str))?;
                let ms = (secs * 1000.0).round() as u32;
                tokens.push(Token::DurationMs(ms));
                pos += 1;
            } else {
                let num_str: String = chars[start..pos].iter().collect();
                let n: f32 = num_str
                    .parse::<f32>()
                    .map_err(|_| format!("invalid number: {}", num_str))?;
                tokens.push(Token::Number(n));
            }
            continue;
        }

        // Identifiers and keywords
        if chars[pos].is_alphabetic() || chars[pos] == '_' {
            let start = pos;
            while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                pos += 1;
            }
            let name: String = chars[start..pos].iter().collect();

            // near_edge special handling
            if name == "near_edge" {
                let mut axis: Option<Axis> = None;
                let mut threshold_px: f32 = 100.0;

                // Scan ahead for optional .x / .y and/or .\d+px
                // We may have: .x, .y, .80px, .x.70px, .y.70px
                // Parse as many ".something" segments as match
                let mut tmp_pos = pos;
                loop {
                    if tmp_pos < chars.len() && chars[tmp_pos] == '.' {
                        let seg_start = tmp_pos + 1;
                        let mut seg_pos = seg_start;
                        while seg_pos < chars.len()
                            && (chars[seg_pos].is_alphanumeric() || chars[seg_pos] == '_')
                        {
                            seg_pos += 1;
                        }
                        let seg: String = chars[seg_start..seg_pos].iter().collect();

                        if seg == "x" {
                            axis = Some(Axis::X);
                            tmp_pos = seg_pos;
                        } else if seg == "y" {
                            axis = Some(Axis::Y);
                            tmp_pos = seg_pos;
                        } else if seg.ends_with("px") {
                            let num_part = &seg[..seg.len() - 2];
                            if let Ok(v) = num_part.parse::<f32>() {
                                threshold_px = v;
                                tmp_pos = seg_pos;
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                pos = tmp_pos;
                tokens.push(Token::NearEdge { axis, threshold_px });
                continue;
            }

            // Check for compound identifiers with a dot (input.focused_app, pet.vx, etc.)
            if pos < chars.len() && chars[pos] == '.' {
                // Peek at what follows
                let mut tmp_pos = pos + 1;
                let sub_start = tmp_pos;
                while tmp_pos < chars.len()
                    && (chars[tmp_pos].is_alphanumeric() || chars[tmp_pos] == '_')
                {
                    tmp_pos += 1;
                }
                let sub: String = chars[sub_start..tmp_pos].iter().collect();
                let compound = format!("{}.{}", name, sub);

                match compound.as_str() {
                    "input.focused_app" | "pet.vx" | "pet.vy" | "pet.v" => {
                        pos = tmp_pos;
                        tokens.push(Token::Ident(compound));
                        continue;
                    }
                    _ => {
                        // not a known compound ident; fall through to handle `name` alone
                    }
                }
            }

            // Keywords and known identifiers
            match name.as_str() {
                "and" => tokens.push(Token::Op(BinOp::And)),
                "or" => tokens.push(Token::Op(BinOp::Or)),
                "not" => tokens.push(Token::Not),
                "true" => tokens.push(Token::Bool(true)),
                "false" => tokens.push(Token::Bool(false)),
                "cursor_dist" | "state_time" | "on_surface" | "pet_x" | "pet_y" | "screen_w"
                | "screen_h" | "hour" | "abs" | "min" | "max" => {
                    tokens.push(Token::Ident(name))
                }
                _ => return Err(format!("unknown identifier: {}", name)),
            }
            continue;
        }

        return Err(format!("unexpected character: '{}'", chars[pos]));
    }

    Ok(tokens)
}

// ─── Parser ──────────────────────────────────────────────────────────────────

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn expect_rparen(&mut self) -> Result<(), String> {
        match self.advance() {
            Some(Token::RParen) => Ok(()),
            other => Err(format!("expected ')', got {:?}", other)),
        }
    }

    // parse_or → parse_and ( "or" parse_and )*
    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while let Some(Token::Op(BinOp::Or)) = self.peek() {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinOp {
                op: BinOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // parse_and → parse_cmp ( "and" parse_cmp )*
    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_cmp()?;
        while let Some(Token::Op(BinOp::And)) = self.peek() {
            self.advance();
            let right = self.parse_cmp()?;
            left = Expr::BinOp {
                op: BinOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // parse_cmp → parse_unary ( ("<"|">"|"<="|">="|"=="|"!=") parse_unary )?
    fn parse_cmp(&mut self) -> Result<Expr, String> {
        let left = self.parse_unary()?;
        let op = match self.peek() {
            Some(Token::Op(op)) => {
                match op {
                    BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge | BinOp::Eq | BinOp::Ne => {
                        op.clone()
                    }
                    _ => return Ok(left),
                }
            }
            _ => return Ok(left),
        };
        self.advance();
        let right = self.parse_unary()?;
        Ok(Expr::BinOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    // parse_unary → "not" parse_primary | parse_primary
    fn parse_unary(&mut self) -> Result<Expr, String> {
        if let Some(Token::Not) = self.peek() {
            self.advance();
            let inner = self.parse_primary()?;
            return Ok(Expr::UnaryNot(Box::new(inner)));
        }
        self.parse_primary()
    }

    // parse_primary → NUMBER | DURATION | BOOL | STR | NEAR_EDGE | IDENT | IDENT "(" args ")" | "(" parse_or ")"
    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek().cloned() {
            Some(Token::Number(n)) => {
                self.advance();
                Ok(Expr::Literal(Literal::Number(n)))
            }
            Some(Token::DurationMs(ms)) => {
                self.advance();
                Ok(Expr::Literal(Literal::DurationMs(ms)))
            }
            Some(Token::Bool(b)) => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(b)))
            }
            Some(Token::Str(s)) => {
                self.advance();
                Ok(Expr::Literal(Literal::Str(s)))
            }
            Some(Token::NearEdge { axis, threshold_px }) => {
                self.advance();
                Ok(Expr::Var(Var::NearEdge { axis, threshold_px }))
            }
            Some(Token::Ident(name)) => {
                self.advance();
                // Check if it's a function call
                if let Some(Token::LParen) = self.peek() {
                    // Validate function name
                    if !["abs", "min", "max"].contains(&name.as_str()) {
                        return Err(format!("unknown function: {}", name));
                    }
                    self.advance(); // consume '('
                    let mut args = Vec::new();
                    if let Some(Token::RParen) = self.peek() {
                        // empty args
                    } else {
                        args.push(self.parse_or()?);
                        while let Some(Token::Comma) = self.peek() {
                            self.advance();
                            args.push(self.parse_or()?);
                        }
                    }
                    self.expect_rparen()?;
                    return Ok(Expr::Call { name, args });
                }
                // Variable
                let var = match name.as_str() {
                    "cursor_dist" => Var::CursorDist,
                    "state_time" => Var::StateTime,
                    "on_surface" => Var::OnSurface,
                    "pet_x" => Var::PetX,
                    "pet_y" => Var::PetY,
                    "pet.vx" => Var::PetVx,
                    "pet.vy" => Var::PetVy,
                    "pet.v" => Var::PetV,
                    "screen_w" => Var::ScreenW,
                    "screen_h" => Var::ScreenH,
                    "hour" => Var::Hour,
                    "input.focused_app" => Var::FocusedApp,
                    _ => return Err(format!("unknown variable: {}", name)),
                };
                Ok(Expr::Var(var))
            }
            Some(Token::LParen) => {
                self.advance();
                let inner = self.parse_or()?;
                self.expect_rparen()?;
                Ok(inner)
            }
            other => Err(format!("unexpected token in primary: {:?}", other)),
        }
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Parse an expression string into an AST. Returns Err with a message on failure.
pub fn parse(src: &str) -> Result<Expr, String> {
    let tokens = tokenize(src)?;
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_or()?;
    if parser.pos != parser.tokens.len() {
        return Err(format!(
            "unexpected token after expression: {:?}",
            parser.tokens.get(parser.pos)
        ));
    }
    Ok(expr)
}

/// Evaluate a compiled expression against a context snapshot.
/// NOTE: stub — full implementation is in Task 4.
pub fn eval(_expr: &Expr, _vars: &ConditionVars) -> Result<bool, String> {
    Ok(false)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_comparison() {
        assert!(parse("cursor_dist < 150").is_ok());
    }

    #[test]
    fn parse_duration_comparison() {
        assert!(parse("state_time > 3s").is_ok());
    }

    #[test]
    fn parse_and_expression() {
        assert!(parse("on_surface and cursor_dist < 50").is_ok());
    }

    #[test]
    fn parse_not() {
        assert!(parse("not on_surface").is_ok());
    }

    #[test]
    fn parse_near_edge_plain() {
        assert!(parse("near_edge").is_ok());
    }

    #[test]
    fn parse_near_edge_parameterized() {
        assert!(parse("near_edge.x.70px").is_ok());
        assert!(parse("near_edge.80px").is_ok());
        assert!(parse("near_edge.y").is_ok());
    }

    #[test]
    fn parse_function_call() {
        assert!(parse("abs(pet.vx) > 50").is_ok());
    }

    #[test]
    fn parse_unknown_variable_fails() {
        assert!(parse("typo_var < 5").is_err());
    }

    #[test]
    fn parse_unknown_function_fails() {
        assert!(parse("eval(1)").is_err());
    }

    #[test]
    fn parse_string_literal() {
        assert!(parse(r#"input.focused_app == "code.exe""#).is_ok());
    }
}
