use std::{error::Error, fmt::Display, rc::Rc};

use crate::parser::Rule;
use pest::{
    iterators::{Pair, Pairs},
    Span,
};

pub type Node<'a> = Pair<'a, Rule>;
pub type Nodes<'a> = Pairs<'a, Rule>;

pub trait ExpectToken<'tok> {
    fn expect_token(self, expected: Rule) -> Result<Node<'tok>, ParseError>;
}

impl<'tok> ExpectToken<'tok> for Node<'tok> {
    fn expect_token(self, rule: Rule) -> Result<Node<'tok>, ParseError> {
        if self.as_rule() == rule {
            return Ok(self);
        }
        Err(ParseError::expected(
            rule,
            self.as_rule(),
            self.as_span().to_owned(),
        ))
    }
}

impl<'tok> ExpectToken<'tok> for Option<Node<'tok>> {
    fn expect_token(self, expected: Rule) -> Result<Node<'tok>, ParseError> {
        return self
            .map(|node| node.expect_token(expected))
            .ok_or(ParseError::none(expected))?;
    }
}

pub trait ExpectTokens<'tok> {
    fn expect_next_token(self, rule: Rule) -> Result<Node<'tok>, ParseError>;
}

impl<'tok> ExpectTokens<'tok> for &mut Nodes<'tok> {
    fn expect_next_token(self, rule: Rule) -> Result<Node<'tok>, ParseError> {
        return self.next().expect_token(rule);
    }
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub span: Option<(usize, usize)>,
}

impl ParseError {
    pub fn expected(expected: Rule, found: Rule, at: Span) -> Self {
        Self {
            message: format!("Expected {expected} but found {found}"),
            span: Some((at.start(), at.end())),
        }
    }

    pub fn none(expected: Rule) -> Self {
        Self {
            message: format!("Expected {expected} but found none"),
            span: None,
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

#[derive(Debug, Clone)]
pub enum ReferenceOr<Referenced, Actual> {
    Reference(Rc<Referenced>),
    Actual(Actual),
}

impl<Referenced, Actual> ReferenceOr<Referenced, Actual> {
    pub fn reference_to(reference: Referenced) -> Self {
        return Self::Reference(Rc::new(reference));
    }
}
