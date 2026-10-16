//! Reads typed input such as `3,5 m + 20 cm` or `10' 6"` into a quantity.
//!
//! ```text
//! expr    = term (("+" | "-") term)*
//! term    = unary (("*" | "/") unary)*
//! unary   = ("-" | "+") unary | primary
//! primary = number units? (number units)*     a mixed quantity: 10' 6"
//!         | "(" expr ")" units?
//!         | units
//! units   = unit+                              juxtaposed units multiply: kN m
//! unit    = symbol ("^" "-"? digits | superscripts)?
//! ```
//!
//! `×`, `·` and `⋅` also multiply, `÷` divides and `−` subtracts.

use hito_core::Dimension;

use crate::ParseError;
use crate::catalogue::{UnitCatalogue, checked_mul, checked_pow, superscript_value};
use crate::number::{NumberFormat, is_ignorable};

/// A value in coherent SI units with its dimension.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Quantity {
    pub value: f64,
    pub dimension: Dimension,
}

/// What [`evaluate`] read.
pub(crate) struct Evaluated {
    pub quantity: Quantity,
    /// Whether the input named any unit. Input without units is in the
    /// display unit.
    pub has_units: bool,
}

/// Characters that are a unit symbol on their own.
const SYMBOL_CHARS: &[char] = &['°', '%', '‰', '\'', '"', '′', '″'];

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Number(f64),
    Unit(Quantity),
    /// `+`, `-`, `*` or `/`.
    Operator(char),
    Open,
    Close,
}

struct Token {
    at: usize,
    len: usize,
    kind: Kind,
}

pub(crate) fn evaluate(
    input: &str,
    catalogue: &UnitCatalogue,
    numbers: &NumberFormat,
) -> Result<Evaluated, ParseError> {
    let chars: Vec<char> = input.chars().collect();
    let tokens = Lexer {
        chars: &chars,
        at: 0,
        catalogue,
        numbers,
    }
    .tokens()?;
    if tokens.is_empty() {
        return Err(ParseError::Empty);
    }
    let mut parser = Parser {
        chars: &chars,
        tokens: &tokens,
        next: 0,
        depth: 0,
        catalogue,
    };
    let quantity = parser.expr()?;
    if let Some(token) = tokens.get(parser.next) {
        return Err(parser.unexpected(token));
    }
    let has_units = tokens.iter().any(|t| matches!(t.kind, Kind::Unit(_)));
    Ok(Evaluated {
        quantity,
        has_units,
    })
}

struct Lexer<'a> {
    chars: &'a [char],
    at: usize,
    catalogue: &'a UnitCatalogue,
    numbers: &'a NumberFormat,
}

impl Lexer<'_> {
    fn tokens(mut self) -> Result<Vec<Token>, ParseError> {
        let mut tokens = Vec::new();
        while let Some(&c) = self.chars.get(self.at) {
            let start = self.at;
            if c.is_whitespace() || is_ignorable(c) {
                self.at += 1;
                continue;
            }
            let kind = if self.starts_number(start) {
                Kind::Number(self.number()?)
            } else if c.is_alphabetic() || SYMBOL_CHARS.contains(&c) {
                Kind::Unit(self.unit()?)
            } else {
                self.at += 1;
                match c {
                    '+' => Kind::Operator('+'),
                    '-' | '−' => Kind::Operator('-'),
                    '*' | '×' | '·' | '⋅' => Kind::Operator('*'),
                    '/' | '÷' => Kind::Operator('/'),
                    '(' => Kind::Open,
                    ')' => Kind::Close,
                    _ => {
                        return Err(ParseError::Unexpected {
                            at: start,
                            found: c.to_string(),
                        });
                    }
                }
            };
            tokens.push(Token {
                at: start,
                len: self.at - start,
                kind,
            });
        }
        Ok(tokens)
    }

    fn is_digit(&self, at: usize) -> bool {
        self.chars
            .get(at)
            .is_some_and(|&c| self.numbers.digit_value(c).is_some())
    }

    /// Whether `symbol` followed by a digit is at `at`.
    fn is_before_digit(&self, at: usize, symbol: &[char]) -> bool {
        !symbol.is_empty()
            && self.chars[at..].starts_with(symbol)
            && self.is_digit(at + symbol.len())
    }

    /// The length of the decimal separator before a digit at `at`, if there
    /// is one. A point counts too when the locale doesn't group with it, so
    /// `3.5` reads in fr or ar-EG but not in es-AR.
    fn decimal_at(&self, at: usize) -> Option<usize> {
        let decimal = &self.numbers.decimal;
        if self.is_before_digit(at, decimal) {
            Some(decimal.len())
        } else {
            (self.numbers.group != ['.'] && self.is_before_digit(at, &['.'])).then_some(1)
        }
    }

    fn starts_number(&self, at: usize) -> bool {
        self.is_digit(at) || self.decimal_at(at).is_some()
    }

    /// Reads a number written in the locale. Group separators must be where
    /// the locale puts them, so that `3.5` in es-AR is an error rather than
    /// thirty-five.
    fn number(&mut self) -> Result<f64, ParseError> {
        let start = self.at;
        let group = &self.numbers.group;
        let mut ascii = String::new();
        let mut in_fraction = false;
        // Digits in each group of the integer part, the last one still open.
        let mut groups = vec![0];
        loop {
            if let Some(n) = self
                .chars
                .get(self.at)
                .and_then(|&c| self.numbers.digit_value(c))
            {
                ascii.push(char::from_digit(n, 10).expect("a decimal digit"));
                if !in_fraction {
                    *groups.last_mut().expect("never empty") += 1;
                }
                self.at += 1;
            } else if in_fraction {
                break;
            } else if self.is_before_digit(self.at, group) {
                groups.push(0);
                self.at += group.len();
            } else if let Some(len) = self.decimal_at(self.at) {
                ascii.push('.');
                in_fraction = true;
                self.at += len;
            } else {
                break;
            }
        }
        // The first group has one to three digits and the last one three.
        // Groups between have two (as in India's 12,34,567) or three.
        let well_grouped = match groups.as_slice() {
            [_] => true,
            [first, middle @ .., last] => {
                (1..=3).contains(first) && middle.iter().all(|n| (2..=3).contains(n)) && *last == 3
            }
            [] => unreachable!("never empty"),
        };
        match ascii.parse() {
            Ok(value) if well_grouped => Ok(value),
            _ => Err(ParseError::BadNumber {
                at: start,
                text: self.chars[start..self.at].iter().collect(),
            }),
        }
    }

    /// Reads a unit symbol and its exponent.
    fn unit(&mut self) -> Result<Quantity, ParseError> {
        let start = self.at;
        if SYMBOL_CHARS.contains(&self.chars[start]) {
            self.at += 1;
        } else {
            while self.chars.get(self.at).is_some_and(|c| c.is_alphabetic()) {
                self.at += 1;
            }
        }
        let symbol: String = self.chars[start..self.at].iter().collect();
        let unit = self
            .catalogue
            .get(&symbol)
            .ok_or_else(|| ParseError::UnknownUnit {
                at: start,
                symbol: symbol.clone(),
            })?;
        let (value, dimension) = (unit.factor, unit.dimension);

        let exponent_at = self.at;
        let Some(exponent) = self.exponent()? else {
            return Ok(Quantity { value, dimension });
        };
        let out_of_range = ParseError::ExponentOutOfRange { at: exponent_at };
        let exponent = i8::try_from(exponent).map_err(|_| out_of_range.clone())?;
        Ok(Quantity {
            value: value.powi(exponent.into()),
            dimension: checked_pow(dimension, exponent).ok_or(out_of_range)?,
        })
    }

    /// Reads `^2`, `^-1`, `²` or `⁻¹`, if there.
    fn exponent(&mut self) -> Result<Option<i64>, ParseError> {
        let caret = self.chars.get(self.at) == Some(&'^');
        if caret {
            self.at += 1;
        }
        let negative = match self.chars.get(self.at) {
            Some('-' | '−') if caret => true,
            Some('⁻') if !caret => true,
            _ => false,
        };
        if negative {
            self.at += 1;
        }
        let digit = |c: char| match caret {
            true => c.to_digit(10),
            false => superscript_value(c),
        };
        let mut exponent: i64 = 0;
        let digits_start = self.at;
        while let Some(n) = self.chars.get(self.at).and_then(|&c| digit(c)) {
            exponent = exponent.saturating_mul(10).saturating_add(n.into());
            self.at += 1;
        }
        if self.at == digits_start {
            if !caret && !negative {
                return Ok(None);
            }
            return Err(match self.chars.get(self.at) {
                Some(c) => ParseError::Unexpected {
                    at: self.at,
                    found: c.to_string(),
                },
                None => ParseError::UnexpectedEnd,
            });
        }
        Ok(Some(if negative { -exponent } else { exponent }))
    }
}

struct Parser<'a> {
    chars: &'a [char],
    tokens: &'a [Token],
    next: usize,
    /// Parentheses and signs open around the current token.
    depth: usize,
    catalogue: &'a UnitCatalogue,
}

/// How deeply parentheses and signs may nest. Far more than anyone types.
const MAX_DEPTH: usize = 64;

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.next)
    }

    fn take(&mut self) -> Result<&'a Token, ParseError> {
        let token = self
            .tokens
            .get(self.next)
            .ok_or(ParseError::UnexpectedEnd)?;
        self.next += 1;
        Ok(token)
    }

    fn unexpected(&self, token: &Token) -> ParseError {
        ParseError::Unexpected {
            at: token.at,
            found: self.chars[token.at..token.at + token.len].iter().collect(),
        }
    }

    fn peek_operator(&self, operators: &[char]) -> Option<(usize, char)> {
        match self.peek()?.kind {
            Kind::Operator(op) if operators.contains(&op) => Some((self.peek()?.at, op)),
            _ => None,
        }
    }

    fn expr(&mut self) -> Result<Quantity, ParseError> {
        let mut left = self.term()?;
        while let Some((at, operator)) = self.peek_operator(&['+', '-']) {
            self.next += 1;
            let right = self.term()?;
            left = self.add(at, operator, left, right)?;
        }
        Ok(left)
    }

    fn add(
        &self,
        at: usize,
        operator: char,
        left: Quantity,
        right: Quantity,
    ) -> Result<Quantity, ParseError> {
        if left.dimension != right.dimension {
            return Err(ParseError::Incompatible {
                at,
                operator,
                left: left.dimension,
                right: right.dimension,
                left_unit: self.catalogue.si_symbol(left.dimension),
                right_unit: self.catalogue.si_symbol(right.dimension),
            });
        }
        let value = match operator {
            '-' => left.value - right.value,
            _ => left.value + right.value,
        };
        Ok(Quantity { value, ..left })
    }

    fn term(&mut self) -> Result<Quantity, ParseError> {
        let mut left = self.unary()?;
        while let Some((at, operator)) = self.peek_operator(&['*', '/']) {
            self.next += 1;
            let right = self.unary()?;
            left = multiply(at, left, right, operator == '/')?;
        }
        Ok(left)
    }

    /// Runs `nested` one level deeper, failing past [`MAX_DEPTH`] so that
    /// input like `((((…` can't overflow the stack.
    fn nest<T>(
        &mut self,
        at: usize,
        nested: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        if self.depth == MAX_DEPTH {
            return Err(ParseError::TooDeep { at });
        }
        self.depth += 1;
        let result = nested(self);
        self.depth -= 1;
        result
    }

    fn unary(&mut self) -> Result<Quantity, ParseError> {
        match self.peek_operator(&['+', '-']) {
            Some((at, operator)) => {
                self.next += 1;
                let q = self.nest(at, Self::unary)?;
                let sign = if operator == '-' { -1.0 } else { 1.0 };
                Ok(Quantity {
                    value: sign * q.value,
                    ..q
                })
            }
            None => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<Quantity, ParseError> {
        match self.take()?.kind {
            Kind::Number(value) => {
                let number = Quantity {
                    value,
                    dimension: Dimension::NONE,
                };
                if !self.peek_unit() {
                    return Ok(number);
                }
                let mut sum = self.with_units(number)?;
                // A mixed quantity, such as 10' 6" or 1 m 20 cm.
                while let Some(&Token {
                    at,
                    kind: Kind::Number(value),
                    ..
                }) = self.peek()
                {
                    self.next += 1;
                    if !self.peek_unit() {
                        return Err(ParseError::MissingUnit { at });
                    }
                    let part = self.with_units(Quantity {
                        value,
                        dimension: Dimension::NONE,
                    })?;
                    sum = self.add(at, '+', sum, part)?;
                }
                Ok(sum)
            }
            Kind::Unit(unit) => self.with_units(unit),
            Kind::Open => {
                let at = self.tokens[self.next - 1].at;
                let inner = self.nest(at, Self::expr)?;
                let close = self.take()?;
                if close.kind != Kind::Close {
                    return Err(self.unexpected(close));
                }
                self.with_units(inner)
            }
            Kind::Operator(_) | Kind::Close => Err(self.unexpected(&self.tokens[self.next - 1])),
        }
    }

    fn peek_unit(&self) -> bool {
        self.peek().is_some_and(|t| matches!(t.kind, Kind::Unit(_)))
    }

    /// Multiplies `q` by the units that follow it, if any.
    fn with_units(&mut self, mut q: Quantity) -> Result<Quantity, ParseError> {
        while let Some(&Token {
            at,
            kind: Kind::Unit(unit),
            ..
        }) = self.peek()
        {
            self.next += 1;
            q = multiply(at, q, unit, false)?;
        }
        Ok(q)
    }
}

fn multiply(at: usize, a: Quantity, b: Quantity, divide: bool) -> Result<Quantity, ParseError> {
    let (value, b_dimension) = match divide {
        true => (a.value / b.value, checked_pow(b.dimension, -1)),
        false => (a.value * b.value, Some(b.dimension)),
    };
    let dimension = b_dimension
        .and_then(|b| checked_mul(a.dimension, b))
        .ok_or(ParseError::ExponentOutOfRange { at })?;
    Ok(Quantity { value, dimension })
}
