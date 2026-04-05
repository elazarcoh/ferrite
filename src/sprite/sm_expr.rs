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
    // Multi-pet vars — populated by App::update() each frame
    pub pet_count: u32,
    pub other_pet_dist: f32,
    // Surface vars — populated by PetInstance::tick() each frame
    pub surface_w: f32,
    pub surface_label: String,  // "taskbar", "window", or "" (virtual ground)
    // Collision vars — populated only during on_collide(); "" / 0.0 otherwise
    pub collide_type: String,
    pub collide_vx: f32,
    pub collide_vy: f32,
    pub collide_v: f32,
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
    PetCount,
    OtherPetDist,
    SurfaceW,
    SurfaceLabel,
    NearEdge {
        axis: Option<Axis>,
        threshold_px: f32,
    },
    CollideType,
    CollideVx,
    CollideVy,
    CollideV,
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
                | "screen_h" | "hour" | "abs" | "min" | "max"
                | "collide_type" | "collide_vx" | "collide_vy" | "collide_v"
                | "pet_count" | "other_pet_dist" | "surface_w" | "surface_label" => {
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
                    "pet_count"      => Var::PetCount,
                    "other_pet_dist" => Var::OtherPetDist,
                    "surface_w"      => Var::SurfaceW,
                    "surface_label"  => Var::SurfaceLabel,
                    "collide_type" => Var::CollideType,
                    "collide_vx" => Var::CollideVx,
                    "collide_vy" => Var::CollideVy,
                    "collide_v" => Var::CollideV,
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

// ─── Evaluator ───────────────────────────────────────────────────────────────

#[derive(Debug)]
enum Value {
    Number(f32),
    Bool(bool),
    Str(String),
}

fn eval_value(expr: &Expr, vars: &ConditionVars) -> Result<Value, String> {
    match expr {
        Expr::Literal(Literal::Number(n)) => Ok(Value::Number(*n)),
        Expr::Literal(Literal::DurationMs(ms)) => Ok(Value::Number(*ms as f32)),
        Expr::Literal(Literal::Bool(b)) => Ok(Value::Bool(*b)),
        Expr::Literal(Literal::Str(s)) => Ok(Value::Str(s.clone())),

        Expr::Var(v) => match v {
            Var::CursorDist => Ok(Value::Number(vars.cursor_dist)),
            Var::StateTime => Ok(Value::Number(vars.state_time_ms as f32)),
            Var::OnSurface => Ok(Value::Bool(vars.on_surface)),
            Var::PetX => Ok(Value::Number(vars.pet_x)),
            Var::PetY => Ok(Value::Number(vars.pet_y)),
            Var::PetVx => Ok(Value::Number(vars.pet_vx)),
            Var::PetVy => Ok(Value::Number(vars.pet_vy)),
            Var::PetV => Ok(Value::Number(vars.pet_v)),
            Var::ScreenW => Ok(Value::Number(vars.screen_w)),
            Var::ScreenH => Ok(Value::Number(vars.screen_h)),
            Var::Hour => Ok(Value::Number(vars.hour as f32)),
            Var::FocusedApp => Ok(Value::Str(vars.focused_app.clone())),
            Var::PetCount      => Ok(Value::Number(vars.pet_count as f32)),
            Var::OtherPetDist  => Ok(Value::Number(vars.other_pet_dist)),
            Var::SurfaceW      => Ok(Value::Number(vars.surface_w)),
            Var::SurfaceLabel  => Ok(Value::Str(vars.surface_label.clone())),
            Var::CollideType => Ok(Value::Str(vars.collide_type.clone())),
            Var::CollideVx => Ok(Value::Number(vars.collide_vx)),
            Var::CollideVy => Ok(Value::Number(vars.collide_vy)),
            Var::CollideV => Ok(Value::Number(vars.collide_v)),
            Var::NearEdge { axis, threshold_px } => {
                let t = *threshold_px;
                let result = match axis {
                    None => {
                        vars.pet_x < t
                            || vars.pet_x + vars.pet_w > vars.screen_w - t
                            || vars.pet_y < t
                            || vars.pet_y + vars.pet_h > vars.screen_h - t
                    }
                    Some(Axis::X) => {
                        vars.pet_x < t || vars.pet_x + vars.pet_w > vars.screen_w - t
                    }
                    Some(Axis::Y) => {
                        vars.pet_y < t || vars.pet_y + vars.pet_h > vars.screen_h - t
                    }
                };
                Ok(Value::Bool(result))
            }
        },

        Expr::UnaryNot(inner) => {
            let b = eval(inner, vars)?;
            Ok(Value::Bool(!b))
        }

        Expr::BinOp { op: BinOp::And, left, right } => {
            if !eval(left, vars)? {
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(eval(right, vars)?))
        }

        Expr::BinOp { op: BinOp::Or, left, right } => {
            if eval(left, vars)? {
                return Ok(Value::Bool(true));
            }
            Ok(Value::Bool(eval(right, vars)?))
        }

        Expr::BinOp { op, left, right } => {
            let lv = eval_value(left, vars)?;
            let rv = eval_value(right, vars)?;
            let result = match (lv, rv) {
                (Value::Number(l), Value::Number(r)) => match op {
                    BinOp::Lt => l < r,
                    BinOp::Gt => l > r,
                    BinOp::Le => l <= r,
                    BinOp::Ge => l >= r,
                    BinOp::Eq => l == r,
                    BinOp::Ne => l != r,
                    _ => unreachable!("And/Or handled above"),
                },
                (Value::Str(l), Value::Str(r)) => match op {
                    BinOp::Eq => l == r,
                    BinOp::Ne => l != r,
                    _ => return Err("only == and != are valid for string comparison".to_string()),
                },
                (Value::Bool(l), Value::Bool(r)) => match op {
                    BinOp::Eq => l == r,
                    BinOp::Ne => l != r,
                    _ => return Err("only == and != are valid for bool comparison".to_string()),
                },
                _ => return Err("type mismatch in comparison".to_string()),
            };
            Ok(Value::Bool(result))
        }

        Expr::Call { name, args } => match name.as_str() {
            "abs" => {
                if args.len() != 1 {
                    return Err(format!("abs expects 1 argument, got {}", args.len()));
                }
                let v = eval_as_number(&args[0], vars)?;
                Ok(Value::Number(v.abs()))
            }
            "min" => {
                if args.len() != 2 {
                    return Err(format!("min expects 2 arguments, got {}", args.len()));
                }
                let a = eval_as_number(&args[0], vars)?;
                let b = eval_as_number(&args[1], vars)?;
                Ok(Value::Number(a.min(b)))
            }
            "max" => {
                if args.len() != 2 {
                    return Err(format!("max expects 2 arguments, got {}", args.len()));
                }
                let a = eval_as_number(&args[0], vars)?;
                let b = eval_as_number(&args[1], vars)?;
                Ok(Value::Number(a.max(b)))
            }
            _ => Err(format!("unknown function: {}", name)),
        },
    }
}

fn eval_as_number(expr: &Expr, vars: &ConditionVars) -> Result<f32, String> {
    match eval_value(expr, vars)? {
        Value::Number(n) => Ok(n),
        other => Err(format!("expected number, got {:?}", other)),
    }
}

/// Evaluate a compiled expression against a context snapshot.
pub fn eval(expr: &Expr, vars: &ConditionVars) -> Result<bool, String> {
    match eval_value(expr, vars)? {
        Value::Bool(b) => Ok(b),
        Value::Number(n) => Ok(n != 0.0),
        Value::Str(_) => Err("expression must evaluate to bool".to_string()),
    }
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

    #[test]
    fn eval_cursor_near() {
        let expr = parse("cursor_dist < 150").unwrap();
        let mut v = ConditionVars::default();
        v.cursor_dist = 100.0;
        assert!(eval(&expr, &v).unwrap());
        v.cursor_dist = 200.0;
        assert!(!eval(&expr, &v).unwrap());
    }

    #[test]
    fn eval_state_time_duration() {
        let expr = parse("state_time > 3s").unwrap();
        let mut v = ConditionVars::default();
        v.state_time_ms = 5000;
        assert!(eval(&expr, &v).unwrap());
        v.state_time_ms = 2000;
        assert!(!eval(&expr, &v).unwrap());
    }

    #[test]
    fn eval_near_edge_x() {
        let expr = parse("near_edge.x.70px").unwrap();
        let mut v = ConditionVars { pet_x: 30.0, screen_w: 1920.0, pet_w: 32.0, ..Default::default() };
        assert!(eval(&expr, &v).unwrap()); // 30 < 70
        v.pet_x = 500.0;
        assert!(!eval(&expr, &v).unwrap());
    }

    #[test]
    fn eval_pet_velocity() {
        let expr = parse("pet.v > 100").unwrap();
        let mut v = ConditionVars::default();
        v.pet_v = 150.0;
        assert!(eval(&expr, &v).unwrap());
    }

    #[test]
    fn eval_abs_function() {
        let expr = parse("abs(pet.vx) > 50").unwrap();
        let mut v = ConditionVars::default();
        v.pet_vx = -80.0;
        assert!(eval(&expr, &v).unwrap());
    }

    #[test]
    fn eval_focused_app() {
        let expr = parse(r#"input.focused_app == "code.exe""#).unwrap();
        let mut v = ConditionVars::default();
        v.focused_app = "code.exe".to_string();
        assert!(eval(&expr, &v).unwrap());
        v.focused_app = "other.exe".to_string();
        assert!(!eval(&expr, &v).unwrap());
    }

    #[test]
    fn parse_collide_type_variable() {
        assert!(parse(r#"collide_type == "head_on""#).is_ok());
    }

    #[test]
    fn parse_collide_v_variable() {
        assert!(parse("collide_v > 80").is_ok());
    }

    #[test]
    fn parse_collide_vx_vy_variables() {
        assert!(parse("collide_vx > 0 and collide_vy < 0").is_ok());
    }

    #[test]
    fn eval_collide_type_matches() {
        let expr = parse(r#"collide_type == "head_on""#).unwrap();
        let mut v = ConditionVars::default();
        v.collide_type = "head_on".to_string();
        assert!(eval(&expr, &v).unwrap());
        v.collide_type = String::new();
        assert!(!eval(&expr, &v).unwrap());
    }

    #[test]
    fn eval_collide_v_threshold() {
        let expr = parse("collide_v > 50").unwrap();
        let mut v = ConditionVars::default();
        v.collide_v = 80.0;
        assert!(eval(&expr, &v).unwrap());
        v.collide_v = 30.0;
        assert!(!eval(&expr, &v).unwrap());
    }

    #[test]
    fn parse_pet_count_variable() {
        assert!(parse("pet_count > 1").is_ok());
    }

    #[test]
    fn parse_other_pet_dist_variable() {
        assert!(parse("other_pet_dist < 200").is_ok());
    }

    #[test]
    fn parse_surface_vars() {
        assert!(parse("surface_w > 100").is_ok());
        assert!(parse(r#"surface_label == "taskbar""#).is_ok());
    }

    #[test]
    fn eval_pet_count_threshold() {
        let expr = parse("pet_count > 1").unwrap();
        let mut v = ConditionVars::default();
        v.pet_count = 2;
        assert!(eval(&expr, &v).unwrap());
        v.pet_count = 1;
        assert!(!eval(&expr, &v).unwrap());
    }

    #[test]
    fn eval_other_pet_dist_threshold() {
        let expr = parse("other_pet_dist < 100").unwrap();
        let mut v = ConditionVars::default();
        v.other_pet_dist = 50.0;
        assert!(eval(&expr, &v).unwrap());
        v.other_pet_dist = 200.0;
        assert!(!eval(&expr, &v).unwrap());
    }

    #[test]
    fn eval_surface_label_string_match() {
        let expr = parse(r#"surface_label == "taskbar""#).unwrap();
        let mut v = ConditionVars::default();
        v.surface_label = "taskbar".to_string();
        assert!(eval(&expr, &v).unwrap());
        v.surface_label = "window".to_string();
        assert!(!eval(&expr, &v).unwrap());
    }

    #[test]
    fn eval_surface_w_threshold() {
        let expr = parse("surface_w > 50").unwrap();
        let mut v = ConditionVars::default();
        v.surface_w = 100.0;
        assert!(eval(&expr, &v).unwrap());
        v.surface_w = 20.0;
        assert!(!eval(&expr, &v).unwrap());
    }
}
