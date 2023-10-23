use std::{error::Error, fmt::Display};

use crate::parser::Rule;
use pest::{
    iterators::{Pair, Pairs},
    Span,
};

pub type Node<'a> = Pair<'a, Rule>;
pub type Nodes<'a> = Pairs<'a, Rule>;

pub trait ExpectToken<'i> {
    fn expect_token(self, expected: Rule) -> Result<Node<'i>, ParseError>;
}

impl<'i> ExpectToken<'i> for Node<'i> {
    fn expect_token(self, rule: Rule) -> Result<Node<'i>, ParseError> {
        if self.as_rule() == rule {
            return Ok(self);
        }

        return Err(ParseError::expected(rule, self.as_rule(), self.as_span()));
    }
}

impl<'i> ExpectToken<'i> for Option<Node<'i>> {
    fn expect_token(self, expected: Rule) -> Result<Node<'i>, ParseError> {
        return self
            .map(|node| node.expect_token(expected))
            .ok_or(ParseError::nothing())?;
    }
}

pub trait ExpectTokens<'i> {
    fn expect_next_token(&mut self, rule: Rule) -> Result<Node<'i>, ParseError>;
}

impl<'i> ExpectTokens<'i> for Nodes<'i> {
    fn expect_next_token(&mut self, rule: Rule) -> Result<Node<'i>, ParseError> {
        return self.next().expect_token(rule);
    }
}

#[derive(Debug)]
pub struct ParseError {
    message: String,
}

impl ParseError {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn expected(expected: Rule, found: Rule, at: Span) -> Self {
        Self {
            message: format!("Expected {expected} but found {found} at {at:?}"),
        }
    }

    pub fn nothing() -> ParseError {
        Self {
            message: String::from("Expected token but found none"),
        }
    }

    pub fn mismatch(primitive: Rule, as_str: &str) -> ParseError {
        Self {
            message: format!("Mismatch: {as_str} is not {primitive}"),
        }
    }
}

impl Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for ParseError {}
impl Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
