//! [SPARQL 1.1 Query Algebra](https://www.w3.org/TR/sparql11-query/#sparqlQuery) representation.

use oxrdf::vocab::xsd;
use rand::random;
pub use spargebra::algebra::{
    AdditiveOperator, Function, MultiplicativeOperator, ParsedSourceLiteral,
    PropertyPathExpression, SourceContextAnnotation, SourceGroupCondition,
    SourceGroupConditionContext, SourceGroupConditionSyntax, SourceGrouping, SourceJoinMode,
    SourceValueTerm, SourceValuesForm, SourceVariableScope,
};
use spargebra::algebra::{
    AggregateExpression as AlAggregateExpression, AggregateFunction, Expression as AlExpression,
    GraphPattern as AlGraphPattern, OrderExpression as AlOrderExpression,
};
use spargebra::term::{BlankNode, TermPattern, TriplePattern};
pub use spargebra::term::{
    GroundTerm, GroundTermPattern, Literal, NamedNode, NamedNodePattern, Variable,
};
#[cfg(feature = "sparql-12")]
use spargebra::term::{GroundTriple, GroundTriplePattern};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::{Add, BitAnd, BitOr, Div, Mul, Neg, Not, Sub};

/// An [expression](https://www.w3.org/TR/sparql11-query/#expressions).
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum Expression {
    NamedNode(NamedNode),
    Literal(Literal),
    Variable(Variable),
    /// [Logical-or](https://www.w3.org/TR/sparql11-query/#func-logical-or).
    Or(Vec<Self>),
    /// [Logical-and](https://www.w3.org/TR/sparql11-query/#func-logical-and).
    And(Vec<Self>),
    /// [RDFterm-equal](https://www.w3.org/TR/sparql11-query/#func-RDFterm-equal) and all the XSD equalities.
    Equal(Box<Self>, Box<Self>),
    /// [sameTerm](https://www.w3.org/TR/sparql11-query/#func-sameTerm).
    SameTerm(Box<Self>, Box<Self>),
    /// [op:numeric-greater-than](https://www.w3.org/TR/xpath-functions-31/#func-numeric-greater-than) and other XSD greater than operators.
    Greater(Box<Self>, Box<Self>),
    GreaterOrEqual(Box<Self>, Box<Self>),
    /// [op:numeric-less-than](https://www.w3.org/TR/xpath-functions-31/#func-numeric-less-than) and other XSD greater than operators.
    Less(Box<Self>, Box<Self>),
    LessOrEqual(Box<Self>, Box<Self>),
    /// [op:numeric-add](https://www.w3.org/TR/xpath-functions-31/#func-numeric-add) and other XSD additions.
    Add(Box<Self>, Box<Self>),
    /// [op:numeric-subtract](https://www.w3.org/TR/xpath-functions-31/#func-numeric-subtract) and other XSD subtractions.
    Subtract(Box<Self>, Box<Self>),
    /// [op:numeric-multiply](https://www.w3.org/TR/xpath-functions-31/#func-numeric-multiply) and other XSD multiplications.
    Multiply(Box<Self>, Box<Self>),
    /// [op:numeric-divide](https://www.w3.org/TR/xpath-functions-31/#func-numeric-divide) and other XSD divides.
    Divide(Box<Self>, Box<Self>),
    /// [op:numeric-unary-plus](https://www.w3.org/TR/xpath-functions-31/#func-numeric-unary-plus) and other XSD unary plus.
    UnaryPlus(Box<Self>),
    /// [op:numeric-unary-minus](https://www.w3.org/TR/xpath-functions-31/#func-numeric-unary-minus) and other XSD unary minus.
    UnaryMinus(Box<Self>),
    /// [fn:not](https://www.w3.org/TR/xpath-functions-31/#func-not).
    Not(Box<Self>),
    /// [EXISTS](https://www.w3.org/TR/sparql11-query/#func-filter-exists).
    Exists(Box<GraphPattern>),
    /// [BOUND](https://www.w3.org/TR/sparql11-query/#func-bound).
    Bound(Variable),
    /// [IF](https://www.w3.org/TR/sparql11-query/#func-if).
    If(Box<Self>, Box<Self>, Box<Self>),
    /// [COALESCE](https://www.w3.org/TR/sparql11-query/#func-coalesce).
    Coalesce(Vec<Self>),
    /// A regular function call.
    FunctionCall(Function, Vec<Self>),
    /// Source order and one additive Expr boundary for the private nullable evaluator.
    /// Appended to preserve the discriminants and hashes of existing expressions.
    #[doc(hidden)]
    AdditiveChain {
        first: Box<Self>,
        tail: Vec<(AdditiveOperator, Self)>,
    },
    /// Source order and one multiplicative Expr boundary for the private nullable evaluator.
    /// A parser-produced tail is nonempty; the evaluator rejects manually empty chains.
    #[doc(hidden)]
    MultiplicativeChain {
        first: Box<Self>,
        tail: Vec<(MultiplicativeOperator, Self)>,
    },
    /// Literal occurrence for the source identity mode; appended for old hashes.
    #[doc(hidden)]
    SourceLiteral(ParsedSourceLiteral),
    #[doc(hidden)]
    SourceExists(spargebra::SourceExistsExpression<GraphPattern>),
    /// An unfinalized grammar capture is transported to the explicit compiler rejection.
    #[doc(hidden)]
    SourceSyntax(spargebra::SourceSyntaxSnapshot),
    /// Original Function metadata; appended without changing old enum hashes.
    #[doc(hidden)]
    SourceFunction {
        function: Function,
        arguments: Vec<Self>,
        syntax: spargebra::SourceSyntaxSnapshot,
    },
    /// Original !=, distinct from logical NOT of equality on errors.
    #[doc(hidden)]
    SourceNotEqual(Box<Self>, Box<Self>),
}

impl Expression {
    pub fn or_all(args: impl IntoIterator<Item = Self>) -> Self {
        let args = args.into_iter();
        let mut all = Vec::with_capacity(args.size_hint().0);
        for arg in args {
            if let Some(ebv) = arg.effective_boolean_value() {
                if ebv {
                    return true.into();
                }
                // We ignore false values
            } else if let Self::Or(args) = arg {
                all.extend(args);
            } else {
                all.push(arg);
            }
        }
        match all.len() {
            0 => false.into(),
            1 => {
                let result = all.pop().unwrap();
                if result.returns_boolean() {
                    result // It's already casted to boolean
                } else {
                    Self::And(vec![result])
                }
            }
            _ => Self::Or(order_vec(all)),
        }
    }

    pub fn and_all(args: impl IntoIterator<Item = Self>) -> Self {
        let args = args.into_iter();
        let mut all = Vec::with_capacity(args.size_hint().0);
        for arg in args {
            if let Some(ebv) = arg.effective_boolean_value() {
                if !ebv {
                    return false.into();
                }
                // We ignore true values
            } else if let Self::And(args) = arg {
                all.extend(args);
            } else {
                all.push(arg);
            }
        }
        match all.len() {
            0 => true.into(),
            1 => {
                let result = all.pop().unwrap();
                if result.returns_boolean() {
                    result
                } else {
                    Self::And(vec![result])
                }
            }
            _ => Self::And(order_vec(all)),
        }
    }

    pub fn equal(left: Self, right: Self) -> Self {
        // TODO: simplify equality when both operands are literal taking care of fun stuff like NaN != NaN
        match (left, right) {
            (Self::NamedNode(left), Self::NamedNode(right)) => (left == right).into(),
            (left, right) => {
                let (left, right) = order_pair(left, right);
                Self::Equal(Box::new(left), Box::new(right))
            }
        }
    }

    pub fn same_term(left: Self, right: Self) -> Self {
        match (left, right) {
            (Self::NamedNode(left), Self::NamedNode(right)) => (left == right).into(),
            (Self::Literal(left), Self::Literal(right)) if left == right => true.into(),
            (left, right) => {
                let (left, right) = order_pair(left, right);
                Self::SameTerm(Box::new(left), Box::new(right))
            }
        }
    }

    pub fn greater(left: Self, right: Self) -> Self {
        Self::Greater(Box::new(left), Box::new(right))
    }

    pub fn greater_or_equal(left: Self, right: Self) -> Self {
        Self::GreaterOrEqual(Box::new(left), Box::new(right))
    }

    pub fn less(left: Self, right: Self) -> Self {
        Self::Less(Box::new(left), Box::new(right))
    }

    pub fn less_or_equal(left: Self, right: Self) -> Self {
        Self::LessOrEqual(Box::new(left), Box::new(right))
    }

    pub fn unary_plus(inner: Self) -> Self {
        Self::UnaryPlus(Box::new(inner))
    }

    pub fn exists(inner: GraphPattern) -> Self {
        if inner.is_empty() {
            return false.into();
        }
        if inner.is_empty_singleton() {
            return true.into();
        }
        Self::Exists(Box::new(inner))
    }

    pub fn if_cond(cond: Self, then: Self, els: Self) -> Self {
        match cond.effective_boolean_value() {
            Some(true) => then,
            Some(false) => els,
            None => Self::If(Box::new(cond), Box::new(then), Box::new(els)),
        }
    }

    pub fn coalesce(args: Vec<Self>) -> Self {
        Self::Coalesce(args)
    }

    pub fn call(name: Function, args: Vec<Self>) -> Self {
        Self::FunctionCall(name, args)
    }

    pub fn effective_boolean_value(&self) -> Option<bool> {
        if let Self::Literal(literal) = self {
            match literal.datatype() {
                xsd::BOOLEAN => match literal.value() {
                    "true" | "1" => Some(true),
                    "false" | "0" => Some(false),
                    _ => None, // TODO
                },
                xsd::STRING => Some(!literal.value().is_empty()),
                _ => None, // TODO
            }
        } else {
            None
        }
    }

    pub fn used_variables(&self) -> HashSet<&Variable> {
        let mut variables = HashSet::new();
        self.lookup_used_variables(&mut |v| {
            variables.insert(v);
        });
        variables
    }

    pub fn lookup_used_variables<'a>(&'a self, callback: &mut impl FnMut(&'a Variable)) {
        match self {
            Self::NamedNode(_) | Self::Literal(_) | Self::SourceLiteral(_) => {}
            Self::Variable(v) | Self::Bound(v) => callback(v),
            Self::Or(inner)
            | Self::And(inner)
            | Self::Coalesce(inner)
            | Self::FunctionCall(_, inner) => {
                for i in inner {
                    i.lookup_used_variables(callback);
                }
            }
            Self::Equal(a, b)
            | Self::SourceNotEqual(a, b)
            | Self::SameTerm(a, b)
            | Self::Greater(a, b)
            | Self::GreaterOrEqual(a, b)
            | Self::Less(a, b)
            | Self::LessOrEqual(a, b)
            | Self::Add(a, b)
            | Self::Subtract(a, b)
            | Self::Multiply(a, b)
            | Self::Divide(a, b) => {
                a.lookup_used_variables(callback);
                b.lookup_used_variables(callback);
            }
            Self::UnaryPlus(i) | Self::UnaryMinus(i) | Self::Not(i) => {
                i.lookup_used_variables(callback)
            }
            Self::Exists(e) => e.lookup_used_variables(callback),
            Self::SourceExists(value) => value.syntax.lookup_used_variables(callback),
            Self::SourceSyntax(value) => value.lookup_used_variables(callback),
            Self::SourceFunction { arguments, .. } => {
                for value in arguments {
                    value.lookup_used_variables(callback);
                }
            }
            Self::If(a, b, c) => {
                a.lookup_used_variables(callback);
                b.lookup_used_variables(callback);
                c.lookup_used_variables(callback);
            }
            Self::AdditiveChain { first, tail } => {
                first.lookup_used_variables(callback);
                for (_, expression) in tail {
                    expression.lookup_used_variables(callback);
                }
            }
            Self::MultiplicativeChain { first, tail } => {
                first.lookup_used_variables(callback);
                for (_, expression) in tail {
                    expression.lookup_used_variables(callback);
                }
            }
        }
    }

    fn from_sparql_algebra(
        expression: &AlExpression,
        graph_name: Option<&NamedNodePattern>,
    ) -> Self {
        match expression {
            AlExpression::NamedNode(node) => Self::NamedNode(node.clone()),
            AlExpression::Literal(literal) => Self::Literal(literal.clone()),
            AlExpression::SourceLiteral(literal) => Self::SourceLiteral(literal.clone()),
            AlExpression::SourceSyntax(value) => Self::SourceSyntax(value.clone()),
            AlExpression::SourceFunction {
                function,
                arguments,
                syntax,
            } => Self::SourceFunction {
                function: function.clone(),
                arguments: arguments
                    .iter()
                    .map(|v| Self::from_sparql_algebra(v, graph_name))
                    .collect(),
                syntax: syntax.clone(),
            },
            AlExpression::SourceExists(value) => {
                Self::SourceExists(value.map_graph(|graph| GraphPattern::from(graph)))
            }
            AlExpression::Variable(variable) => Self::Variable(variable.clone()),
            AlExpression::Or(left, right) => Self::Or(vec![
                Self::from_sparql_algebra(left, graph_name),
                Self::from_sparql_algebra(right, graph_name),
            ]),
            AlExpression::And(left, right) => Self::And(vec![
                Self::from_sparql_algebra(left, graph_name),
                Self::from_sparql_algebra(right, graph_name),
            ]),
            AlExpression::SourceNotEqual(left, right) => Self::SourceNotEqual(
                Box::new(Self::from_sparql_algebra(left, graph_name)),
                Box::new(Self::from_sparql_algebra(right, graph_name)),
            ),
            AlExpression::Equal(left, right) => Self::Equal(
                Box::new(Self::from_sparql_algebra(left, graph_name)),
                Box::new(Self::from_sparql_algebra(right, graph_name)),
            ),
            AlExpression::SameTerm(left, right) => Self::SameTerm(
                Box::new(Self::from_sparql_algebra(left, graph_name)),
                Box::new(Self::from_sparql_algebra(right, graph_name)),
            ),
            AlExpression::Greater(left, right) => Self::Greater(
                Box::new(Self::from_sparql_algebra(left, graph_name)),
                Box::new(Self::from_sparql_algebra(right, graph_name)),
            ),
            AlExpression::GreaterOrEqual(left, right) => Self::GreaterOrEqual(
                Box::new(Self::from_sparql_algebra(left, graph_name)),
                Box::new(Self::from_sparql_algebra(right, graph_name)),
            ),
            AlExpression::Less(left, right) => Self::Less(
                Box::new(Self::from_sparql_algebra(left, graph_name)),
                Box::new(Self::from_sparql_algebra(right, graph_name)),
            ),
            AlExpression::LessOrEqual(left, right) => Self::LessOrEqual(
                Box::new(Self::from_sparql_algebra(left, graph_name)),
                Box::new(Self::from_sparql_algebra(right, graph_name)),
            ),
            AlExpression::In(left, right) => {
                let left = Self::from_sparql_algebra(left, graph_name);
                match right.len() {
                    0 => Self::if_cond(left, false.into(), false.into()),
                    1 => Self::Equal(
                        Box::new(left),
                        Box::new(Self::from_sparql_algebra(&right[0], graph_name)),
                    ),
                    _ => Self::Or(
                        right
                            .iter()
                            .map(|e| {
                                Self::Equal(
                                    Box::new(left.clone()),
                                    Box::new(Self::from_sparql_algebra(e, graph_name)),
                                )
                            })
                            .collect(),
                    ),
                }
            }
            AlExpression::Add(left, right) => Self::Add(
                Box::new(Self::from_sparql_algebra(left, graph_name)),
                Box::new(Self::from_sparql_algebra(right, graph_name)),
            ),
            AlExpression::Subtract(left, right) => Self::Subtract(
                Box::new(Self::from_sparql_algebra(left, graph_name)),
                Box::new(Self::from_sparql_algebra(right, graph_name)),
            ),
            AlExpression::Multiply(left, right) => Self::Multiply(
                Box::new(Self::from_sparql_algebra(left, graph_name)),
                Box::new(Self::from_sparql_algebra(right, graph_name)),
            ),
            AlExpression::Divide(left, right) => Self::Divide(
                Box::new(Self::from_sparql_algebra(left, graph_name)),
                Box::new(Self::from_sparql_algebra(right, graph_name)),
            ),
            AlExpression::UnaryPlus(inner) => {
                Self::UnaryPlus(Box::new(Self::from_sparql_algebra(inner, graph_name)))
            }
            AlExpression::UnaryMinus(inner) => {
                Self::UnaryMinus(Box::new(Self::from_sparql_algebra(inner, graph_name)))
            }
            AlExpression::Not(inner) => {
                Self::Not(Box::new(Self::from_sparql_algebra(inner, graph_name)))
            }
            AlExpression::Exists(inner) => Self::Exists(Box::new(
                GraphPattern::from_sparql_algebra(inner, graph_name, &mut HashMap::new()),
            )),
            AlExpression::Bound(variable) => Self::Bound(variable.clone()),
            AlExpression::If(cond, yes, no) => Self::If(
                Box::new(Self::from_sparql_algebra(cond, graph_name)),
                Box::new(Self::from_sparql_algebra(yes, graph_name)),
                Box::new(Self::from_sparql_algebra(no, graph_name)),
            ),
            AlExpression::Coalesce(inner) => Self::Coalesce(
                inner
                    .iter()
                    .map(|e| Self::from_sparql_algebra(e, graph_name))
                    .collect(),
            ),
            AlExpression::FunctionCall(name, args) => Self::FunctionCall(
                name.clone(),
                args.iter()
                    .map(|e| Self::from_sparql_algebra(e, graph_name))
                    .collect(),
            ),
            AlExpression::AdditiveChain { first, tail } => Self::AdditiveChain {
                first: Box::new(Self::from_sparql_algebra(first, graph_name)),
                tail: tail
                    .iter()
                    .map(|(operator, expression)| {
                        (*operator, Self::from_sparql_algebra(expression, graph_name))
                    })
                    .collect(),
            },
            AlExpression::MultiplicativeChain { first, tail } => Self::MultiplicativeChain {
                first: Box::new(Self::from_sparql_algebra(first, graph_name)),
                tail: tail
                    .iter()
                    .map(|(operator, expression)| {
                        (*operator, Self::from_sparql_algebra(expression, graph_name))
                    })
                    .collect(),
            },
        }
    }

    fn returns_boolean(&self) -> bool {
        match self {
            Self::Or(_)
            | Self::And(_)
            | Self::Equal(_, _)
            | Self::SourceNotEqual(_, _)
            | Self::SameTerm(_, _)
            | Self::Greater(_, _)
            | Self::GreaterOrEqual(_, _)
            | Self::Less(_, _)
            | Self::LessOrEqual(_, _)
            | Self::Not(_)
            | Self::Exists(_)
            | Self::Bound(_)
            | Self::FunctionCall(
                Function::IsBlank | Function::IsIri | Function::IsLiteral | Function::IsNumeric,
                _,
            ) => true,
            #[cfg(feature = "sparql-12")]
            Self::FunctionCall(Function::IsTriple, _) => true,
            Self::Literal(literal) => literal.datatype() == xsd::BOOLEAN,
            Self::If(_, a, b) => a.returns_boolean() && b.returns_boolean(),
            Self::AdditiveChain { .. } | Self::MultiplicativeChain { .. } => false,
            _ => false,
        }
    }
}

impl From<NamedNode> for Expression {
    fn from(value: NamedNode) -> Self {
        Self::NamedNode(value)
    }
}

impl From<Literal> for Expression {
    fn from(value: Literal) -> Self {
        Self::Literal(value)
    }
}

impl From<GroundTerm> for Expression {
    fn from(value: GroundTerm) -> Self {
        match value {
            GroundTerm::NamedNode(value) => value.into(),
            GroundTerm::Literal(value) => value.into(),
            #[cfg(feature = "sparql-12")]
            GroundTerm::Triple(value) => (*value).into(),
        }
    }
}

impl From<NamedNodePattern> for Expression {
    fn from(value: NamedNodePattern) -> Self {
        match value {
            NamedNodePattern::NamedNode(value) => value.into(),
            NamedNodePattern::Variable(variable) => variable.into(),
        }
    }
}

impl From<GroundTermPattern> for Expression {
    fn from(value: GroundTermPattern) -> Self {
        match value {
            GroundTermPattern::NamedNode(value) => value.into(),
            GroundTermPattern::Literal(value) => value.into(),
            #[cfg(feature = "sparql-12")]
            GroundTermPattern::Triple(value) => (*value).into(),
            GroundTermPattern::Variable(variable) => variable.into(),
            GroundTermPattern::SourceLiteral(literal) => Self::SourceLiteral(literal),
        }
    }
}

#[cfg(feature = "sparql-12")]
impl From<GroundTriple> for Expression {
    fn from(value: GroundTriple) -> Self {
        Self::FunctionCall(
            Function::Triple,
            vec![
                value.subject.into(),
                value.predicate.into(),
                value.object.into(),
            ],
        )
    }
}

#[cfg(feature = "sparql-12")]
impl From<GroundTriplePattern> for Expression {
    fn from(value: GroundTriplePattern) -> Self {
        Self::FunctionCall(
            Function::Triple,
            vec![
                value.subject.into(),
                value.predicate.into(),
                value.object.into(),
            ],
        )
    }
}

impl From<Variable> for Expression {
    fn from(value: Variable) -> Self {
        Self::Variable(value)
    }
}

impl From<bool> for Expression {
    fn from(value: bool) -> Self {
        Literal::from(value).into()
    }
}

impl From<&Expression> for AlExpression {
    fn from(expression: &Expression) -> Self {
        match expression {
            Expression::NamedNode(node) => Self::NamedNode(node.clone()),
            Expression::Literal(literal) => Self::Literal(literal.clone()),
            Expression::SourceLiteral(literal) => Self::SourceLiteral(literal.clone()),
            Expression::SourceSyntax(value) => Self::SourceSyntax(value.clone()),
            Expression::SourceFunction {
                function,
                arguments,
                syntax,
            } => Self::SourceFunction {
                function: function.clone(),
                arguments: arguments.iter().map(Into::into).collect(),
                syntax: syntax.clone(),
            },
            Expression::SourceExists(value) => {
                Self::SourceExists(value.map_graph(|graph| AlGraphPattern::from(graph)))
            }
            Expression::Variable(variable) => Self::Variable(variable.clone()),
            Expression::Or(inner) => inner
                .iter()
                .map(Into::into)
                .reduce(|a, b| Self::Or(Box::new(a), Box::new(b)))
                .unwrap_or_else(|| Literal::from(false).into()),
            Expression::And(inner) => inner
                .iter()
                .map(Into::into)
                .reduce(|a, b| Self::And(Box::new(a), Box::new(b)))
                .unwrap_or_else(|| Literal::from(true).into()),
            Expression::SourceNotEqual(left, right) => Self::SourceNotEqual(
                Box::new(left.as_ref().into()),
                Box::new(right.as_ref().into()),
            ),
            Expression::Equal(left, right) => Self::Equal(
                Box::new(left.as_ref().into()),
                Box::new(right.as_ref().into()),
            ),
            Expression::SameTerm(left, right) => Self::SameTerm(
                Box::new(left.as_ref().into()),
                Box::new(right.as_ref().into()),
            ),
            Expression::Greater(left, right) => Self::Greater(
                Box::new(left.as_ref().into()),
                Box::new(right.as_ref().into()),
            ),
            Expression::GreaterOrEqual(left, right) => Self::GreaterOrEqual(
                Box::new(left.as_ref().into()),
                Box::new(right.as_ref().into()),
            ),
            Expression::Less(left, right) => Self::Less(
                Box::new(left.as_ref().into()),
                Box::new(right.as_ref().into()),
            ),
            Expression::LessOrEqual(left, right) => Self::LessOrEqual(
                Box::new(left.as_ref().into()),
                Box::new(right.as_ref().into()),
            ),
            Expression::Add(left, right) => Self::Add(
                Box::new(left.as_ref().into()),
                Box::new(right.as_ref().into()),
            ),
            Expression::Subtract(left, right) => Self::Subtract(
                Box::new(left.as_ref().into()),
                Box::new(right.as_ref().into()),
            ),
            Expression::Multiply(left, right) => Self::Multiply(
                Box::new(left.as_ref().into()),
                Box::new(right.as_ref().into()),
            ),
            Expression::Divide(left, right) => Self::Divide(
                Box::new(left.as_ref().into()),
                Box::new(right.as_ref().into()),
            ),
            Expression::UnaryPlus(inner) => Self::UnaryPlus(Box::new(inner.as_ref().into())),
            Expression::UnaryMinus(inner) => Self::UnaryMinus(Box::new(inner.as_ref().into())),
            Expression::Not(inner) => Self::Not(Box::new(inner.as_ref().into())),
            Expression::Exists(inner) => Self::Exists(Box::new(inner.as_ref().into())),
            Expression::Bound(variable) => Self::Bound(variable.clone()),
            Expression::If(cond, yes, no) => Self::If(
                Box::new(cond.as_ref().into()),
                Box::new(yes.as_ref().into()),
                Box::new(no.as_ref().into()),
            ),
            Expression::Coalesce(inner) => Self::Coalesce(inner.iter().map(Into::into).collect()),
            Expression::FunctionCall(name, args) => {
                Self::FunctionCall(name.clone(), args.iter().map(Into::into).collect())
            }
            Expression::AdditiveChain { first, tail } => Self::AdditiveChain {
                first: Box::new(first.as_ref().into()),
                tail: tail
                    .iter()
                    .map(|(operator, expression)| (*operator, expression.into()))
                    .collect(),
            },
            Expression::MultiplicativeChain { first, tail } => Self::MultiplicativeChain {
                first: Box::new(first.as_ref().into()),
                tail: tail
                    .iter()
                    .map(|(operator, expression)| (*operator, expression.into()))
                    .collect(),
            },
        }
    }
}

impl BitAnd for Expression {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self::and_all([self, rhs])
    }
}

impl BitOr for Expression {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self::or_all([self, rhs])
    }
}

impl Not for Expression {
    type Output = Self;

    fn not(self) -> Self {
        if let Some(v) = self.effective_boolean_value() {
            (!v).into()
        } else if let Self::Not(v) = self {
            if v.returns_boolean() {
                *v
            } else {
                Self::Not(Box::new(Self::Not(v)))
            }
        } else {
            Self::Not(Box::new(self))
        }
    }
}

impl Add for Expression {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        let (left, right) = order_pair(self, rhs);
        Self::Add(Box::new(left), Box::new(right))
    }
}

impl Sub for Expression {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::Subtract(Box::new(self), Box::new(rhs))
    }
}

impl Mul for Expression {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        let (left, right) = order_pair(self, rhs);
        Self::Multiply(Box::new(left), Box::new(right))
    }
}

impl Div for Expression {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        Self::Divide(Box::new(self), Box::new(rhs))
    }
}

impl Neg for Expression {
    type Output = Self;

    fn neg(self) -> Self {
        Self::UnaryMinus(Box::new(self))
    }
}

/// A SPARQL query [graph pattern](https://www.w3.org/TR/sparql11-query/#sparqlQuery).
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum GraphPattern {
    /// A [basic graph pattern](https://www.w3.org/TR/sparql11-query/#defn_BasicGraphPattern).
    QuadPattern {
        subject: GroundTermPattern,
        predicate: NamedNodePattern,
        object: GroundTermPattern,
        graph_name: Option<NamedNodePattern>, // None for the default graph
    },
    /// A [property path pattern](https://www.w3.org/TR/sparql11-query/#defn_evalPP_predicate).
    Path {
        subject: GroundTermPattern,
        path: PropertyPathExpression,
        object: GroundTermPattern,
        graph_name: Option<NamedNodePattern>, // None for the default graph
    },
    /// Graph check
    ///
    /// Can yield all named graph like in `GRAPH ?g {}`
    /// or only check if a graph exist like in `GRAPH ex:g {}`
    Graph { graph_name: NamedNodePattern },
    /// [Join](https://www.w3.org/TR/sparql11-query/#defn_algJoin).
    Join {
        left: Box<Self>,
        right: Box<Self>,
        algorithm: JoinAlgorithm,
    },
    /// [LeftJoin](https://www.w3.org/TR/sparql11-query/#defn_algLeftJoin).
    LeftJoin {
        left: Box<Self>,
        right: Box<Self>,
        expression: Expression,
        algorithm: LeftJoinAlgorithm,
    },
    /// Lateral join i.e. evaluate right for all result row of left
    #[cfg(feature = "sep-0006")]
    Lateral { left: Box<Self>, right: Box<Self> },
    /// [Filter](https://www.w3.org/TR/sparql11-query/#defn_algFilter).
    Filter {
        expression: Expression,
        inner: Box<Self>,
    },
    /// [Union](https://www.w3.org/TR/sparql11-query/#defn_algUnion).
    Union { inner: Vec<Self> },
    /// [Extend](https://www.w3.org/TR/sparql11-query/#defn_extend).
    Extend {
        inner: Box<Self>,
        variable: Variable,
        expression: Expression,
    },
    /// [Minus](https://www.w3.org/TR/sparql11-query/#defn_algMinus).
    Minus {
        left: Box<Self>,
        right: Box<Self>,
        algorithm: MinusAlgorithm,
    },
    /// A table used to provide inline values
    Values {
        variables: Vec<Variable>,
        bindings: Vec<Vec<Option<GroundTerm>>>,
    },
    /// [OrderBy](https://www.w3.org/TR/sparql11-query/#defn_algOrdered).
    OrderBy {
        inner: Box<Self>,
        expression: Vec<OrderExpression>,
    },
    /// [Project](https://www.w3.org/TR/sparql11-query/#defn_algProjection).
    Project {
        inner: Box<Self>,
        variables: Vec<Variable>,
    },
    /// [Distinct](https://www.w3.org/TR/sparql11-query/#defn_algDistinct).
    Distinct { inner: Box<Self> },
    /// [Reduced](https://www.w3.org/TR/sparql11-query/#defn_algReduced).
    Reduced { inner: Box<Self> },
    /// [Slice](https://www.w3.org/TR/sparql11-query/#defn_algSlice).
    Slice {
        inner: Box<Self>,
        start: usize,
        length: Option<usize>,
    },
    /// [Group](https://www.w3.org/TR/sparql11-query/#aggregateAlgebra).
    Group {
        inner: Box<Self>,
        variables: Vec<Variable>,
        aggregates: Vec<(Variable, AggregateExpression)>,
    },
    /// [Service](https://www.w3.org/TR/sparql11-federated-query/#defn_evalService).
    Service {
        name: NamedNodePattern,
        inner: Box<Self>,
        silent: bool,
    },
    /// Original aggregate grouping; only the bound private source evaluator
    /// may execute it. Append-only to preserve existing variant hash tags.
    #[doc(hidden)]
    SourceGroup {
        inner: Box<Self>,
        grouping: SourceGrouping<Expression>,
        aggregates: Vec<(Variable, AggregateExpression)>,
    },
    /// A table of source occurrences, never simplified using physical RDF Eq.
    #[doc(hidden)]
    SourceValues {
        variables: Vec<Variable>,
        bindings: Vec<Vec<Option<SourceValueTerm>>>,
    },
    /// Raw source ORDER conditions for the sealed source-literal evaluator.
    /// Appended so all earlier graph-pattern hash discriminants stay unchanged.
    #[doc(hidden)]
    SourceOrderBy {
        inner: Box<Self>,
        expression: Vec<OrderExpression>,
    },
    /// Source binding-context annotations. The wrapped node remains the only
    /// owner of its children, expressions and data. Append-only for old hashes.
    #[doc(hidden)]
    SourceContext {
        inner: Box<Self>,
        annotation: SourceContextAnnotation,
    },
}

impl GraphPattern {
    pub fn empty() -> Self {
        Self::Values {
            variables: Vec::new(),
            bindings: Vec::new(),
        }
    }

    /// Check if the pattern is the empty table
    fn is_empty(&self) -> bool {
        if let Self::Values { bindings, .. } = self {
            bindings.is_empty()
        } else {
            false
        }
    }

    pub fn empty_singleton() -> Self {
        Self::Values {
            variables: Vec::new(),
            bindings: vec![Vec::new()],
        }
    }

    pub fn is_empty_singleton(&self) -> bool {
        if let Self::Values { bindings, .. } = self {
            bindings.len() == 1 && bindings.iter().all(|b| b.iter().all(Option::is_none))
        } else {
            false
        }
    }

    pub fn join(left: Self, right: Self, algorithm: JoinAlgorithm) -> Self {
        if left.is_empty() || right.is_empty() {
            return Self::empty();
        }
        if left.is_empty_singleton() {
            return right;
        }
        if right.is_empty_singleton() {
            return left;
        }
        Self::Join {
            left: Box::new(left),
            right: Box::new(right),
            algorithm,
        }
    }

    #[cfg(feature = "sep-0006")]
    pub fn lateral(left: Self, right: Self) -> Self {
        if left.is_empty() || right.is_empty() {
            return Self::empty();
        }
        if left.is_empty_singleton() {
            return right;
        }
        if right.is_empty_singleton() {
            return left;
        }
        Self::Lateral {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    pub fn left_join(
        left: Self,
        right: Self,
        expression: Expression,
        algorithm: LeftJoinAlgorithm,
    ) -> Self {
        let expression_ebv = expression.effective_boolean_value();
        if left.is_empty()
            || right.is_empty()
            || right.is_empty_singleton()
            || expression_ebv == Some(false)
        {
            return left;
        }
        Self::LeftJoin {
            left: Box::new(left),
            right: Box::new(right),
            expression: if expression_ebv == Some(true) {
                true.into()
            } else {
                expression
            },
            algorithm,
        }
    }

    pub fn minus(left: Self, right: Self, algorithm: MinusAlgorithm) -> Self {
        if left.is_empty() {
            return Self::empty();
        }
        if right.is_empty() {
            return left;
        }
        Self::Minus {
            left: Box::new(left),
            right: Box::new(right),
            algorithm,
        }
    }

    pub fn union(left: Self, right: Self) -> Self {
        Self::union_all([left, right])
    }

    pub fn union_all(args: impl IntoIterator<Item = Self>) -> Self {
        let args = args.into_iter();
        let mut all = Vec::with_capacity(args.size_hint().0);
        for arg in args {
            if arg.is_empty() {
                continue;
            }
            if let Self::Union { inner } = arg {
                all.extend(inner);
            } else {
                all.push(arg);
            }
        }
        if all.is_empty() {
            Self::empty()
        } else {
            Self::Union {
                inner: order_vec(all),
            }
        }
    }

    pub fn filter(inner: Self, expression: Expression) -> Self {
        if inner.is_empty() {
            return Self::empty();
        }
        // We unwrap singleton And
        let expression = match expression {
            Expression::And(mut l) if l.len() == 1 => l.pop().unwrap(),
            e => e,
        };
        match expression.effective_boolean_value() {
            Some(true) => inner,
            Some(false) => Self::empty(),
            None => match inner {
                Self::Filter {
                    inner: nested_inner,
                    expression: e2,
                } => Self::Filter {
                    inner: nested_inner,
                    expression: expression & e2,
                },
                _ => Self::Filter {
                    inner: Box::new(inner),
                    expression,
                },
            },
        }
    }

    pub fn extend(inner: Self, variable: Variable, expression: Expression) -> Self {
        if inner.is_empty() {
            return Self::empty();
        }
        Self::Extend {
            inner: Box::new(inner),
            variable,
            expression,
        }
    }

    pub fn values(
        mut variables: Vec<Variable>,
        mut bindings: Vec<Vec<Option<GroundTerm>>>,
    ) -> Self {
        let empty_rows = (0..variables.len())
            .filter(|row| !bindings.iter().any(|binding| binding.get(*row).is_some()))
            .collect::<Vec<_>>();
        if !empty_rows.is_empty() {
            // We remove empty rows
            variables = variables
                .into_iter()
                .enumerate()
                .filter_map(|(i, v)| {
                    if empty_rows.contains(&i) {
                        None
                    } else {
                        Some(v)
                    }
                })
                .collect();
            bindings = bindings
                .into_iter()
                .map(|binding| {
                    binding
                        .into_iter()
                        .enumerate()
                        .filter_map(|(i, v)| {
                            if empty_rows.contains(&i) {
                                None
                            } else {
                                Some(v)
                            }
                        })
                        .collect()
                })
                .collect();
        }
        Self::Values {
            variables,
            bindings,
        }
    }

    pub fn order_by(inner: Self, expression: Vec<OrderExpression>) -> Self {
        if inner.is_empty() {
            return Self::empty();
        }
        if expression.is_empty() {
            return inner;
        }
        Self::OrderBy {
            inner: Box::new(inner),
            expression,
        }
    }

    pub fn project(inner: Self, variables: Vec<Variable>) -> Self {
        Self::Project {
            inner: Box::new(inner),
            variables,
        }
    }

    pub fn distinct(inner: Self) -> Self {
        if inner.is_empty() {
            return Self::empty();
        }
        Self::Distinct {
            inner: Box::new(inner),
        }
    }

    pub fn reduced(inner: Self) -> Self {
        if inner.is_empty() {
            return Self::empty();
        }
        Self::Reduced {
            inner: Box::new(inner),
        }
    }

    pub fn slice(inner: Self, start: usize, length: Option<usize>) -> Self {
        if inner.is_empty() {
            return Self::empty();
        }
        if start == 0 && length.is_none() {
            return inner;
        }
        Self::Slice {
            inner: Box::new(inner),
            start,
            length,
        }
    }

    pub fn group(
        inner: Self,
        variables: Vec<Variable>,
        aggregates: Vec<(Variable, AggregateExpression)>,
    ) -> Self {
        if inner.is_empty() {
            return Self::empty();
        }
        Self::Group {
            inner: Box::new(inner),
            variables,
            aggregates,
        }
    }

    pub fn service(inner: Self, name: NamedNodePattern, silent: bool) -> Self {
        Self::Service {
            inner: Box::new(inner),
            name,
            silent,
        }
    }

    pub fn lookup_used_variables<'a>(&'a self, callback: &mut impl FnMut(&'a Variable)) {
        match self {
            Self::Values { variables, .. }
            | Self::SourceValues { variables, .. }
            | Self::Project { variables, .. } => {
                for v in variables {
                    callback(v);
                }
            }
            Self::QuadPattern {
                subject,
                predicate,
                object,
                graph_name,
            } => {
                lookup_term_pattern_variables(subject, callback);
                if let NamedNodePattern::Variable(v) = predicate {
                    callback(v);
                }
                lookup_term_pattern_variables(object, callback);
                if let Some(NamedNodePattern::Variable(v)) = graph_name {
                    callback(v);
                }
            }
            Self::Path {
                subject,
                object,
                graph_name,
                ..
            } => {
                lookup_term_pattern_variables(subject, callback);
                lookup_term_pattern_variables(object, callback);
                if let Some(NamedNodePattern::Variable(v)) = graph_name {
                    callback(v);
                }
            }
            Self::Graph { graph_name } => {
                if let NamedNodePattern::Variable(v) = graph_name {
                    callback(v);
                }
            }
            Self::Filter { inner, expression } => {
                expression.lookup_used_variables(callback);
                inner.lookup_used_variables(callback);
            }
            Self::Union { inner } => {
                for child in inner {
                    child.lookup_used_variables(callback);
                }
            }
            Self::Join { left, right, .. } | Self::Minus { left, right, .. } => {
                left.lookup_used_variables(callback);
                right.lookup_used_variables(callback);
            }
            #[cfg(feature = "sep-0006")]
            Self::Lateral { left, right } => {
                left.lookup_used_variables(callback);
                right.lookup_used_variables(callback);
            }
            Self::LeftJoin {
                left,
                right,
                expression,
                ..
            } => {
                expression.lookup_used_variables(callback);
                left.lookup_used_variables(callback);
                right.lookup_used_variables(callback);
            }
            Self::Extend {
                inner,
                variable,
                expression,
            } => {
                callback(variable);
                expression.lookup_used_variables(callback);
                inner.lookup_used_variables(callback);
            }
            Self::OrderBy { inner, .. }
            | Self::SourceOrderBy { inner, .. }
            | Self::SourceContext { inner, .. }
            | Self::Distinct { inner }
            | Self::Reduced { inner }
            | Self::Slice { inner, .. } => inner.lookup_used_variables(callback),
            Self::Service { inner, name, .. } => {
                if let NamedNodePattern::Variable(v) = name {
                    callback(v);
                }
                inner.lookup_used_variables(callback);
            }
            Self::Group {
                variables,
                aggregates,
                ..
            } => {
                for v in variables {
                    callback(v);
                }
                for (v, _) in aggregates {
                    callback(v);
                }
            }
            Self::SourceGroup { aggregates, .. } => {
                for (variable, _) in aggregates {
                    callback(variable);
                }
            }
        }
    }

    fn from_sparql_algebra(
        pattern: &AlGraphPattern,
        graph_name: Option<&NamedNodePattern>,
        blank_nodes: &mut HashMap<BlankNode, Variable>,
    ) -> Self {
        match pattern {
            AlGraphPattern::SourceContext { inner, annotation } => Self::SourceContext {
                inner: Box::new(Self::from_sparql_algebra(inner, graph_name, blank_nodes)),
                annotation: annotation.clone(),
            },
            AlGraphPattern::Bgp { patterns } => patterns
                .iter()
                .map(|p| {
                    let (subject, predicate, object) =
                        Self::triple_pattern_from_algebra(p, blank_nodes);
                    Self::QuadPattern {
                        subject,
                        predicate,
                        object,
                        graph_name: graph_name.cloned(),
                    }
                })
                .reduce(|a, b| Self::Join {
                    left: Box::new(a),
                    right: Box::new(b),
                    algorithm: JoinAlgorithm::default(),
                })
                .unwrap_or_else(|| {
                    if let Some(graph_name) = graph_name {
                        Self::Graph {
                            graph_name: graph_name.clone(),
                        }
                    } else {
                        Self::empty_singleton()
                    }
                }),
            AlGraphPattern::Path {
                subject,
                path,
                object,
            } => Self::Path {
                subject: Self::term_pattern_from_algebra(subject, blank_nodes),
                path: path.clone(),
                object: Self::term_pattern_from_algebra(object, blank_nodes),
                graph_name: graph_name.cloned(),
            },
            AlGraphPattern::Join { left, right } => Self::Join {
                left: Box::new(Self::from_sparql_algebra(left, graph_name, blank_nodes)),
                right: Box::new(Self::from_sparql_algebra(right, graph_name, blank_nodes)),
                algorithm: JoinAlgorithm::default(),
            },
            AlGraphPattern::LeftJoin {
                left,
                right,
                expression,
            } => Self::LeftJoin {
                left: Box::new(Self::from_sparql_algebra(left, graph_name, blank_nodes)),
                right: Box::new(Self::from_sparql_algebra(right, graph_name, blank_nodes)),
                expression: expression.as_ref().map_or_else(
                    || true.into(),
                    |e| Expression::from_sparql_algebra(e, graph_name),
                ),
                algorithm: LeftJoinAlgorithm::default(),
            },
            #[cfg(feature = "sep-0006")]
            AlGraphPattern::Lateral { left, right } => Self::Lateral {
                left: Box::new(Self::from_sparql_algebra(left, graph_name, blank_nodes)),
                right: Box::new(Self::from_sparql_algebra(right, graph_name, blank_nodes)),
            },
            AlGraphPattern::Filter { inner, expr } => Self::Filter {
                inner: Box::new(Self::from_sparql_algebra(inner, graph_name, blank_nodes)),
                expression: Expression::from_sparql_algebra(expr, graph_name),
            },
            AlGraphPattern::Union { left, right } => Self::Union {
                inner: vec![
                    Self::from_sparql_algebra(left, graph_name, blank_nodes),
                    Self::from_sparql_algebra(right, graph_name, blank_nodes),
                ],
            },
            AlGraphPattern::Graph { inner, name } => {
                Self::from_sparql_algebra(inner, Some(name), blank_nodes)
            }
            AlGraphPattern::Extend {
                inner,
                expression,
                variable,
            } => Self::Extend {
                inner: Box::new(Self::from_sparql_algebra(inner, graph_name, blank_nodes)),
                expression: Expression::from_sparql_algebra(expression, graph_name),
                variable: variable.clone(),
            },
            AlGraphPattern::Minus { left, right } => Self::Minus {
                left: Box::new(Self::from_sparql_algebra(left, graph_name, blank_nodes)),
                right: Box::new(Self::from_sparql_algebra(right, graph_name, blank_nodes)),
                algorithm: MinusAlgorithm::default(),
            },
            AlGraphPattern::Values {
                variables,
                bindings,
            } => Self::Values {
                variables: variables.clone(),
                bindings: bindings.clone(),
            },
            AlGraphPattern::SourceValues {
                variables,
                bindings,
            } => Self::SourceValues {
                variables: variables.clone(),
                bindings: bindings.clone(),
            },
            AlGraphPattern::SourceOrderBy { inner, expression } => Self::SourceOrderBy {
                inner: Box::new(Self::from_sparql_algebra(inner, graph_name, blank_nodes)),
                expression: expression
                    .iter()
                    .map(|expression| match expression {
                        AlOrderExpression::Asc(expression) => OrderExpression::Asc(
                            Expression::from_sparql_algebra(expression, graph_name),
                        ),
                        AlOrderExpression::Desc(expression) => OrderExpression::Desc(
                            Expression::from_sparql_algebra(expression, graph_name),
                        ),
                    })
                    .collect(),
            },
            AlGraphPattern::OrderBy { inner, expression } => {
                let mut inner = Self::from_sparql_algebra(inner, graph_name, blank_nodes);
                let mut expressions = Vec::with_capacity(expression.len());
                for e in expression {
                    expressions.push(match e {
                        AlOrderExpression::Asc(e) => {
                            let v;
                            (v, inner) = Self::algebra_expression_to_constant_or_variable(
                                e, inner, graph_name,
                            );
                            OrderExpression::Asc(Expression::Variable(v))
                        }
                        AlOrderExpression::Desc(e) => {
                            let v;
                            (v, inner) = Self::algebra_expression_to_constant_or_variable(
                                e, inner, graph_name,
                            );
                            OrderExpression::Desc(Expression::Variable(v))
                        }
                    });
                }
                Self::OrderBy {
                    inner: Box::new(inner),
                    expression: expressions,
                }
            }
            AlGraphPattern::Project { inner, variables } => {
                let graph_name = if let Some(NamedNodePattern::Variable(graph_name)) = graph_name {
                    Some(NamedNodePattern::Variable(
                        if variables.contains(graph_name) {
                            graph_name.clone()
                        } else {
                            new_var()
                        },
                    ))
                } else {
                    graph_name.cloned()
                };
                Self::Project {
                    inner: Box::new(Self::from_sparql_algebra(
                        inner,
                        graph_name.as_ref(),
                        &mut HashMap::new(),
                    )),
                    variables: variables.clone(),
                }
            }
            AlGraphPattern::Distinct { inner } => Self::Distinct {
                inner: Box::new(Self::from_sparql_algebra(inner, graph_name, blank_nodes)),
            },
            AlGraphPattern::Reduced { inner } => Self::Distinct {
                inner: Box::new(Self::from_sparql_algebra(inner, graph_name, blank_nodes)),
            },
            AlGraphPattern::Slice {
                inner,
                start,
                length,
            } => Self::Slice {
                inner: Box::new(Self::from_sparql_algebra(inner, graph_name, blank_nodes)),
                start: *start,
                length: *length,
            },
            AlGraphPattern::Group {
                inner,
                variables,
                aggregates,
            } => Self::Group {
                inner: Box::new(Self::from_sparql_algebra(inner, graph_name, blank_nodes)),
                variables: variables.clone(),
                aggregates: aggregates
                    .iter()
                    .map(|(var, expr)| {
                        (
                            var.clone(),
                            AggregateExpression::from_sparql_algebra(expr, graph_name),
                        )
                    })
                    .collect(),
            },
            AlGraphPattern::Service {
                inner,
                name,
                silent,
            } => Self::Service {
                inner: Box::new(Self::from_sparql_algebra(inner, graph_name, blank_nodes)),
                name: name.clone(),
                silent: *silent,
            },
            AlGraphPattern::SourceGroup {
                inner,
                grouping,
                aggregates,
            } => Self::SourceGroup {
                inner: Box::new(Self::from_sparql_algebra(inner, graph_name, blank_nodes)),
                grouping: match grouping {
                    SourceGrouping::Implicit => SourceGrouping::Implicit,
                    SourceGrouping::Explicit(conditions) => SourceGrouping::Explicit(
                        conditions
                            .iter()
                            .map(|condition| match condition {
                                SourceGroupCondition::Raw(expression) => SourceGroupCondition::Raw(
                                    Expression::from_sparql_algebra(expression, graph_name),
                                ),
                                SourceGroupCondition::As {
                                    expression,
                                    variable,
                                } => SourceGroupCondition::As {
                                    expression: Expression::from_sparql_algebra(
                                        expression, graph_name,
                                    ),
                                    variable: variable.clone(),
                                },
                            })
                            .collect(),
                    ),
                },
                aggregates: aggregates
                    .iter()
                    .map(|(variable, expression)| {
                        (
                            variable.clone(),
                            AggregateExpression::from_sparql_algebra(expression, graph_name),
                        )
                    })
                    .collect(),
            },
        }
    }

    fn triple_pattern_from_algebra(
        pattern: &TriplePattern,
        blank_nodes: &mut HashMap<BlankNode, Variable>,
    ) -> (GroundTermPattern, NamedNodePattern, GroundTermPattern) {
        (
            Self::term_pattern_from_algebra(&pattern.subject, blank_nodes),
            pattern.predicate.clone(),
            Self::term_pattern_from_algebra(&pattern.object, blank_nodes),
        )
    }

    fn term_pattern_from_algebra(
        pattern: &TermPattern,
        blank_nodes: &mut HashMap<BlankNode, Variable>,
    ) -> GroundTermPattern {
        match pattern {
            TermPattern::NamedNode(node) => node.clone().into(),
            TermPattern::BlankNode(node) => blank_nodes
                .entry(node.clone())
                .or_insert_with(new_var)
                .clone()
                .into(),
            TermPattern::Literal(literal) => literal.clone().into(),
            TermPattern::SourceLiteral(literal) => {
                GroundTermPattern::SourceLiteral(literal.clone())
            }
            #[cfg(feature = "sparql-12")]
            TermPattern::Triple(pattern) => {
                let (subject, predicate, object) =
                    Self::triple_pattern_from_algebra(pattern, blank_nodes);
                GroundTriplePattern {
                    subject,
                    predicate,
                    object,
                }
                .into()
            }
            TermPattern::Variable(variable) => variable.clone().into(),
        }
    }

    /// Makes sure the expression is a variable, use Extend in the other cases
    fn algebra_expression_to_constant_or_variable(
        expression: &AlExpression,
        graph_pattern: GraphPattern,
        graph_name: Option<&NamedNodePattern>,
    ) -> (Variable, GraphPattern) {
        if let AlExpression::Variable(variable) = expression {
            (variable.clone(), graph_pattern)
        } else {
            let variable = Variable::new_unchecked(format!("{:x}", random::<u128>()));
            (
                variable.clone(),
                GraphPattern::Extend {
                    inner: Box::new(graph_pattern),
                    variable,
                    expression: Expression::from_sparql_algebra(expression, graph_name),
                },
            )
        }
    }
}

impl From<&AlGraphPattern> for GraphPattern {
    fn from(pattern: &AlGraphPattern) -> Self {
        Self::from_sparql_algebra(pattern, None, &mut HashMap::new())
    }
}

impl From<&GraphPattern> for AlGraphPattern {
    fn from(pattern: &GraphPattern) -> Self {
        match pattern {
            GraphPattern::QuadPattern {
                subject,
                predicate,
                object,
                graph_name,
            } => {
                let pattern = Self::Bgp {
                    patterns: vec![TriplePattern {
                        subject: subject.clone().into(),
                        predicate: predicate.clone(),
                        object: object.clone().into(),
                    }],
                };
                if let Some(graph_name) = graph_name {
                    Self::Graph {
                        inner: Box::new(pattern),
                        name: graph_name.clone(),
                    }
                } else {
                    pattern
                }
            }
            GraphPattern::Path {
                subject,
                path,
                object,
                graph_name,
            } => {
                let pattern = Self::Path {
                    subject: subject.clone().into(),
                    path: path.clone(),
                    object: object.clone().into(),
                };
                if let Some(graph_name) = graph_name {
                    Self::Graph {
                        inner: Box::new(pattern),
                        name: graph_name.clone(),
                    }
                } else {
                    pattern
                }
            }
            GraphPattern::Graph { graph_name } => Self::Graph {
                inner: Box::new(AlGraphPattern::Bgp {
                    patterns: Vec::new(),
                }),
                name: graph_name.clone(),
            },
            GraphPattern::Join { left, right, .. } => {
                match (left.as_ref().into(), right.as_ref().into()) {
                    (Self::Bgp { patterns: mut left }, Self::Bgp { patterns: right }) => {
                        left.extend(right);
                        Self::Bgp { patterns: left }
                    }
                    (left, right) => Self::Join {
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                }
            }
            GraphPattern::LeftJoin {
                left,
                right,
                expression,
                ..
            } => {
                let empty_expr = if let Expression::Literal(l) = expression {
                    l.datatype() == xsd::BOOLEAN && l.value() == "true"
                } else {
                    false
                };
                Self::LeftJoin {
                    left: Box::new(left.as_ref().into()),
                    right: Box::new(right.as_ref().into()),
                    expression: if empty_expr {
                        None
                    } else {
                        Some(expression.into())
                    },
                }
            }
            #[cfg(feature = "sep-0006")]
            GraphPattern::Lateral { left, right } => {
                match (left.as_ref().into(), right.as_ref().into()) {
                    (Self::Bgp { patterns: mut left }, Self::Bgp { patterns: right }) => {
                        left.extend(right);
                        Self::Bgp { patterns: left }
                    }
                    (left, right) => Self::Lateral {
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                }
            }
            GraphPattern::Filter { inner, expression } => Self::Filter {
                inner: Box::new(inner.as_ref().into()),
                expr: expression.into(),
            },
            GraphPattern::Union { inner } => inner
                .iter()
                .map(Into::into)
                .reduce(|a, b| Self::Union {
                    left: Box::new(a),
                    right: Box::new(b),
                })
                .unwrap_or_else(|| Self::Values {
                    variables: Vec::new(),
                    bindings: Vec::new(),
                }),
            GraphPattern::Extend {
                inner,
                expression,
                variable,
            } => Self::Extend {
                inner: Box::new(inner.as_ref().into()),
                expression: expression.into(),
                variable: variable.clone(),
            },
            GraphPattern::Minus { left, right, .. } => Self::Minus {
                left: Box::new(left.as_ref().into()),
                right: Box::new(right.as_ref().into()),
            },
            GraphPattern::Values {
                variables,
                bindings,
            } => Self::Values {
                variables: variables.clone(),
                bindings: bindings.clone(),
            },
            GraphPattern::SourceContext { inner, annotation } => {
                let mut inner: Self = inner.as_ref().into();
                // Physical LeftJoin uses a true expression for absent source
                // conditions. Restore the actual TrueFilter origin, not a
                // user-authored boolean expression, on reverse transport.
                if matches!(
                    annotation,
                    SourceContextAnnotation::LeftJoin {
                        unconditional: true,
                        ..
                    }
                ) {
                    if let Self::LeftJoin { expression, .. } = &mut inner {
                        *expression = None;
                    }
                }
                Self::SourceContext {
                    inner: Box::new(inner),
                    annotation: annotation.clone(),
                }
            }
            GraphPattern::SourceValues {
                variables,
                bindings,
            } => Self::SourceValues {
                variables: variables.clone(),
                bindings: bindings.clone(),
            },
            GraphPattern::SourceOrderBy { inner, expression } => Self::SourceOrderBy {
                inner: Box::new(inner.as_ref().into()),
                expression: expression.iter().map(Into::into).collect(),
            },
            GraphPattern::OrderBy { inner, expression } => Self::OrderBy {
                inner: Box::new(inner.as_ref().into()),
                expression: expression.iter().map(Into::into).collect(),
            },
            GraphPattern::Project { inner, variables } => Self::Project {
                inner: Box::new(inner.as_ref().into()),
                variables: variables.clone(),
            },
            GraphPattern::Distinct { inner } => Self::Distinct {
                inner: Box::new(inner.as_ref().into()),
            },
            GraphPattern::Reduced { inner } => Self::Distinct {
                inner: Box::new(inner.as_ref().into()),
            },
            GraphPattern::Slice {
                inner,
                start,
                length,
            } => Self::Slice {
                inner: Box::new(inner.as_ref().into()),
                start: *start,
                length: *length,
            },
            GraphPattern::Group {
                inner,
                variables,
                aggregates,
            } => Self::Group {
                inner: Box::new(inner.as_ref().into()),
                variables: variables.clone(),
                aggregates: aggregates
                    .iter()
                    .map(|(var, expr)| (var.clone(), expr.into()))
                    .collect(),
            },
            GraphPattern::Service {
                inner,
                name,
                silent,
            } => Self::Service {
                inner: Box::new(inner.as_ref().into()),
                name: name.clone(),
                silent: *silent,
            },
            GraphPattern::SourceGroup {
                inner,
                grouping,
                aggregates,
            } => Self::SourceGroup {
                inner: Box::new(inner.as_ref().into()),
                grouping: match grouping {
                    SourceGrouping::Implicit => SourceGrouping::Implicit,
                    SourceGrouping::Explicit(conditions) => SourceGrouping::Explicit(
                        conditions
                            .iter()
                            .map(|condition| match condition {
                                SourceGroupCondition::Raw(expression) => {
                                    SourceGroupCondition::Raw(expression.into())
                                }
                                SourceGroupCondition::As {
                                    expression,
                                    variable,
                                } => SourceGroupCondition::As {
                                    expression: expression.into(),
                                    variable: variable.clone(),
                                },
                            })
                            .collect(),
                    ),
                },
                aggregates: aggregates
                    .iter()
                    .map(|(variable, expression)| (variable.clone(), expression.into()))
                    .collect(),
            },
        }
    }
}

/// The join algorithm used (c.f. [`GraphPattern::Join`]).
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum JoinAlgorithm {
    HashBuildLeftProbeRight { keys: Vec<Variable> },
}

impl Default for JoinAlgorithm {
    fn default() -> Self {
        Self::HashBuildLeftProbeRight {
            keys: Vec::default(),
        }
    }
}

/// The left join algorithm used (c.f. [`GraphPattern::LeftJoin`]).
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum LeftJoinAlgorithm {
    HashBuildRightProbeLeft { keys: Vec<Variable> },
}

impl Default for LeftJoinAlgorithm {
    fn default() -> Self {
        Self::HashBuildRightProbeLeft {
            keys: Vec::default(),
        }
    }
}

/// The left join algorithm used (c.f. [`GraphPattern::Minus`]).
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum MinusAlgorithm {
    HashBuildRightProbeLeft { keys: Vec<Variable> },
}

impl Default for MinusAlgorithm {
    fn default() -> Self {
        Self::HashBuildRightProbeLeft {
            keys: Vec::default(),
        }
    }
}

/// A set function used in aggregates (c.f. [`GraphPattern::Group`]).
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum AggregateExpression {
    CountSolutions {
        distinct: bool,
    },
    FunctionCall {
        name: AggregateFunction,
        expr: Expression,
        distinct: bool,
    },
}

impl AggregateExpression {
    fn from_sparql_algebra(
        expression: &AlAggregateExpression,
        graph_name: Option<&NamedNodePattern>,
    ) -> Self {
        match expression {
            AlAggregateExpression::CountSolutions { distinct } => Self::CountSolutions {
                distinct: *distinct,
            },
            AlAggregateExpression::FunctionCall {
                name,
                expr,
                distinct,
            } => Self::FunctionCall {
                name: name.clone(),
                expr: Expression::from_sparql_algebra(expr, graph_name),
                distinct: *distinct,
            },
        }
    }
}

impl From<&AggregateExpression> for AlAggregateExpression {
    fn from(expression: &AggregateExpression) -> Self {
        match expression {
            AggregateExpression::CountSolutions { distinct } => Self::CountSolutions {
                distinct: *distinct,
            },
            AggregateExpression::FunctionCall {
                name,
                expr,
                distinct,
            } => Self::FunctionCall {
                name: name.clone(),
                expr: expr.into(),
                distinct: *distinct,
            },
        }
    }
}

/// An ordering comparator used by [`GraphPattern::OrderBy`].
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum OrderExpression {
    /// Ascending order
    Asc(Expression),
    /// Descending order
    Desc(Expression),
}

impl From<&OrderExpression> for AlOrderExpression {
    fn from(expression: &OrderExpression) -> Self {
        match expression {
            OrderExpression::Asc(e) => Self::Asc(e.into()),
            OrderExpression::Desc(e) => Self::Desc(e.into()),
        }
    }
}

fn new_var() -> Variable {
    Variable::new_unchecked(format!("{:x}", random::<u128>()))
}

fn order_pair<T: Hash>(a: T, b: T) -> (T, T) {
    if hash(&a) <= hash(&b) { (a, b) } else { (b, a) }
}

fn order_vec<T: Hash>(mut vec: Vec<T>) -> Vec<T> {
    vec.sort_unstable_by_key(|a| hash(a));
    vec
}

fn hash(v: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    v.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod nullable_chain_tests {
    use super::*;

    fn variable(name: &str) -> Variable {
        Variable::new(name).unwrap()
    }

    fn term(value: i64) -> AlExpression {
        AlExpression::Literal(Literal::from(value))
    }

    fn ordered_chain() -> AlExpression {
        AlExpression::AdditiveChain {
            first: Box::new(AlExpression::Variable(variable("z"))),
            tail: vec![
                (
                    AdditiveOperator::Subtract,
                    AlExpression::MultiplicativeChain {
                        first: Box::new(AlExpression::Variable(variable("b"))),
                        tail: vec![
                            (
                                MultiplicativeOperator::Multiply,
                                AlExpression::Variable(variable("a")),
                            ),
                            (
                                MultiplicativeOperator::Divide,
                                AlExpression::Variable(variable("c")),
                            ),
                        ],
                    },
                ),
                (AdditiveOperator::Add, AlExpression::Variable(variable("d"))),
            ],
        }
    }

    #[test]
    fn nullable_chain_lowering_preserves_order_nesting_and_reverse_structure() {
        let flat = AlExpression::MultiplicativeChain {
            first: Box::new(term(1)),
            tail: vec![
                (MultiplicativeOperator::Divide, term(0)),
                (MultiplicativeOperator::Multiply, term(1)),
            ],
        };
        let nested = AlExpression::MultiplicativeChain {
            first: Box::new(AlExpression::MultiplicativeChain {
                first: Box::new(term(1)),
                tail: vec![(MultiplicativeOperator::Divide, term(0))],
            }),
            tail: vec![(MultiplicativeOperator::Multiply, term(1))],
        };
        let lowered_flat = Expression::from_sparql_algebra(&flat, None);
        let lowered_nested = Expression::from_sparql_algebra(&nested, None);
        assert_ne!(lowered_flat, lowered_nested);
        for original in [flat, nested, ordered_chain()] {
            let lowered = Expression::from_sparql_algebra(&original, None);
            assert!(!lowered.returns_boolean());
            assert_eq!(lowered.effective_boolean_value(), None);
            let restored = AlExpression::from(&lowered);
            assert_eq!(restored, original);
            // sparopt has no separate Display implementation. Diagnostics use
            // this reverse conversion into the source-preserving algebra.
            assert_eq!(restored.to_string(), original.to_string());
        }
    }

    #[test]
    fn nullable_chain_variable_visitation_retains_source_order() {
        let lowered = Expression::from_sparql_algebra(&ordered_chain(), None);
        let mut names = Vec::new();
        lowered.lookup_used_variables(&mut |variable| names.push(variable.as_str()));
        assert_eq!(names, ["z", "b", "a", "c", "d"]);
        assert_eq!(
            lowered
                .used_variables()
                .iter()
                .map(|variable| variable.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from(["z", "b", "a", "c", "d"]),
        );
    }

    #[test]
    fn nullable_chains_cross_graph_aggregate_and_order_lowering() {
        let expression = ordered_chain();
        let empty = AlGraphPattern::Values {
            variables: Vec::new(),
            bindings: vec![vec![]],
        };
        let aggregate = AlAggregateExpression::FunctionCall {
            name: AggregateFunction::Count,
            expr: expression.clone(),
            distinct: true,
        };
        let lowered_aggregate = AggregateExpression::from_sparql_algebra(&aggregate, None);
        assert_eq!(AlAggregateExpression::from(&lowered_aggregate), aggregate);
        for original in [
            AlGraphPattern::Filter {
                inner: Box::new(empty.clone()),
                expr: expression.clone(),
            },
            AlGraphPattern::Extend {
                inner: Box::new(empty.clone()),
                variable: variable("result"),
                expression: expression.clone(),
            },
            AlGraphPattern::Group {
                inner: Box::new(empty.clone()),
                variables: Vec::new(),
                aggregates: vec![(variable("result"), aggregate)],
            },
        ] {
            assert_eq!(
                AlGraphPattern::from(&GraphPattern::from(&original)),
                original
            );
        }
        let original = AlGraphPattern::OrderBy {
            inner: Box::new(empty),
            expression: vec![AlOrderExpression::Desc(expression.clone())],
        };
        let lowered = GraphPattern::from(&original);
        let GraphPattern::OrderBy {
            inner,
            expression: order,
        } = &lowered
        else {
            panic!("actual OrderBy lowering is required");
        };
        let GraphPattern::Extend {
            variable: slot,
            expression: value,
            ..
        } = inner.as_ref()
        else {
            panic!("the existing complex-order expression lowering must stay explicit");
        };
        assert_eq!(AlExpression::from(value), expression);
        assert_eq!(
            order,
            &[OrderExpression::Desc(Expression::Variable(slot.clone()))]
        );
        assert_eq!(
            AlOrderExpression::from(&OrderExpression::Desc(value.clone())),
            AlOrderExpression::Desc(expression),
        );
        // The source expression survives the existing synthetic Extend. This
        // does not claim that ORDER BY is enabled in the typed evaluator.
        let AlGraphPattern::OrderBy { inner, .. } = AlGraphPattern::from(&lowered) else {
            panic!();
        };
        assert!(matches!(*inner, AlGraphPattern::Extend { .. }));
    }

    // Snapshot of the original 0.3.7 enum from archive SHA256
    // 973ee503ab3d6c3de66b4b459233cd5b22b44dad0fb7b8d0931daf73d2ede935.
    // This test-only value type freezes old hash discriminants and payloads;
    // it has no evaluator, conversions, or new chain variants.
    #[derive(Hash)]
    enum PreviousExpression {
        NamedNode(NamedNode),
        Literal(Literal),
        Variable(Variable),
        Or(Vec<Self>),
        And(Vec<Self>),
        Equal(Box<Self>, Box<Self>),
        SameTerm(Box<Self>, Box<Self>),
        Greater(Box<Self>, Box<Self>),
        GreaterOrEqual(Box<Self>, Box<Self>),
        Less(Box<Self>, Box<Self>),
        LessOrEqual(Box<Self>, Box<Self>),
        Add(Box<Self>, Box<Self>),
        Subtract(Box<Self>, Box<Self>),
        Multiply(Box<Self>, Box<Self>),
        Divide(Box<Self>, Box<Self>),
        UnaryPlus(Box<Self>),
        UnaryMinus(Box<Self>),
        Not(Box<Self>),
        Exists(Box<GraphPattern>),
        Bound(Variable),
        If(Box<Self>, Box<Self>, Box<Self>),
        Coalesce(Vec<Self>),
        FunctionCall(Function, Vec<Self>),
    }

    #[test]
    fn chain_variants_leave_every_previous_expression_hash_stream_unchanged() {
        #[derive(Default)]
        struct Bytes(Vec<u8>);
        impl Hasher for Bytes {
            fn finish(&self) -> u64 {
                0
            }
            fn write(&mut self, bytes: &[u8]) {
                self.0.extend_from_slice(bytes);
            }
        }
        fn bytes(value: &impl Hash) -> Vec<u8> {
            let mut recorded = Bytes::default();
            value.hash(&mut recorded);
            recorded.0
        }
        fn current() -> Box<Expression> {
            Box::new(Expression::Variable(variable("x")))
        }
        fn previous() -> Box<PreviousExpression> {
            Box::new(PreviousExpression::Variable(variable("x")))
        }
        let mut pairs = vec![
            (
                Expression::NamedNode(NamedNode::new("urn:fixed").unwrap()),
                PreviousExpression::NamedNode(NamedNode::new("urn:fixed").unwrap()),
            ),
            (
                Expression::Literal(Literal::from(7)),
                PreviousExpression::Literal(Literal::from(7)),
            ),
            (*current(), *previous()),
            (
                Expression::Or(vec![*current()]),
                PreviousExpression::Or(vec![*previous()]),
            ),
            (
                Expression::And(vec![*current()]),
                PreviousExpression::And(vec![*previous()]),
            ),
            (
                Expression::Exists(Box::new(GraphPattern::empty_singleton())),
                PreviousExpression::Exists(Box::new(GraphPattern::empty_singleton())),
            ),
            (
                Expression::Bound(variable("x")),
                PreviousExpression::Bound(variable("x")),
            ),
            (
                Expression::If(current(), current(), current()),
                PreviousExpression::If(previous(), previous(), previous()),
            ),
            (
                Expression::Coalesce(vec![*current()]),
                PreviousExpression::Coalesce(vec![*previous()]),
            ),
            (
                Expression::FunctionCall(Function::Str, vec![*current()]),
                PreviousExpression::FunctionCall(Function::Str, vec![*previous()]),
            ),
        ];
        macro_rules! binary {
            ($($variant:ident),+ $(,)?) => { $(pairs.push((Expression::$variant(current(), current()), PreviousExpression::$variant(previous(), previous())));)+ };
        }
        macro_rules! unary {
            ($($variant:ident),+ $(,)?) => { $(pairs.push((Expression::$variant(current()), PreviousExpression::$variant(previous())));)+ };
        }
        binary!(
            Equal,
            SameTerm,
            Greater,
            GreaterOrEqual,
            Less,
            LessOrEqual,
            Add,
            Subtract,
            Multiply,
            Divide
        );
        unary!(UnaryPlus, UnaryMinus, Not);
        assert_eq!(pairs.len(), 23);
        for (current, previous) in pairs {
            assert_eq!(bytes(&current), bytes(&previous));
            assert_eq!(hash(&current), hash(&previous));
        }
    }
}

fn lookup_term_pattern_variables<'a>(
    pattern: &'a GroundTermPattern,
    callback: &mut impl FnMut(&'a Variable),
) {
    if let GroundTermPattern::Variable(v) = pattern {
        callback(v);
    }
    #[cfg(feature = "sparql-12")]
    if let GroundTermPattern::Triple(t) = pattern {
        lookup_term_pattern_variables(&t.subject, callback);
        if let NamedNodePattern::Variable(v) = &t.predicate {
            callback(v);
        }
        lookup_term_pattern_variables(&t.object, callback);
    }
}

#[cfg(test)]
mod source_group_tests {
    use super::*;

    fn variable(name: &str) -> Variable {
        Variable::new_unchecked(name)
    }

    #[test]
    fn source_group_lowering_roundtrip_retains_raw_as_order_and_empty_child() {
        let chain = AlExpression::AdditiveChain {
            first: Box::new(AlExpression::Variable(variable("x"))),
            tail: vec![(AdditiveOperator::Add, AlExpression::Literal(1.into()))],
        };
        let original = AlGraphPattern::SourceGroup {
            inner: Box::new(AlGraphPattern::Values {
                variables: vec![variable("x")],
                bindings: Vec::new(),
            }),
            grouping: SourceGrouping::Explicit(vec![
                SourceGroupCondition::Raw(chain.clone()),
                SourceGroupCondition::As {
                    expression: chain,
                    variable: variable("key"),
                },
            ]),
            aggregates: vec![(
                variable("total"),
                AlAggregateExpression::FunctionCall {
                    name: AggregateFunction::Sum,
                    expr: AlExpression::Variable(variable("x")),
                    distinct: true,
                },
            )],
        };
        let lowered = GraphPattern::from(&original);
        assert_eq!(AlGraphPattern::from(&lowered), original);
        let mut outputs = Vec::new();
        lowered.lookup_used_variables(&mut |variable| outputs.push(variable.clone()));
        assert_eq!(outputs, [variable("total")]);
        assert!(
            matches!(lowered, GraphPattern::SourceGroup { inner, .. } if matches!(*inner, GraphPattern::Values { ref bindings, .. } if bindings.is_empty()))
        );
    }

    #[test]
    fn source_group_lowering_preserves_implicit_marker_and_aggregate_occurrences() {
        let expression = AlAggregateExpression::CountSolutions { distinct: false };
        let original = AlGraphPattern::SourceGroup {
            inner: Box::new(AlGraphPattern::Values {
                variables: Vec::new(),
                bindings: vec![Vec::new()],
            }),
            grouping: SourceGrouping::Implicit,
            aggregates: vec![
                (variable("a"), expression.clone()),
                (variable("b"), expression),
            ],
        };
        let lowered = GraphPattern::from(&original);
        assert_eq!(AlGraphPattern::from(&lowered), original);
        assert!(
            matches!(lowered, GraphPattern::SourceGroup { grouping: SourceGrouping::Implicit, aggregates, .. } if aggregates.len() == 2)
        );
    }

    #[test]
    fn source_group_append_preserves_legacy_group_service_hash_bytes() {
        #[derive(Default)]
        struct RecordingHasher(Vec<u8>);
        impl Hasher for RecordingHasher {
            fn finish(&self) -> u64 {
                0
            }
            fn write(&mut self, bytes: &[u8]) {
                self.0.extend_from_slice(bytes);
            }
        }
        let inner = Box::new(GraphPattern::empty_singleton());
        let variables = vec![variable("key")];
        let aggregates = vec![(
            variable("count"),
            AggregateExpression::CountSolutions { distinct: false },
        )];
        let name = NamedNodePattern::NamedNode(NamedNode::new("urn:service").unwrap());
        let old_group_tag = 15_isize + isize::from(cfg!(feature = "sep-0006"));
        for (pattern, tag) in [
            (
                GraphPattern::Group {
                    inner: inner.clone(),
                    variables: variables.clone(),
                    aggregates: aggregates.clone(),
                },
                old_group_tag,
            ),
            (
                GraphPattern::Service {
                    name: name.clone(),
                    inner: inner.clone(),
                    silent: false,
                },
                old_group_tag + 1,
            ),
        ] {
            let mut actual = RecordingHasher::default();
            pattern.hash(&mut actual);
            let mut previous = RecordingHasher::default();
            tag.hash(&mut previous);
            match &pattern {
                GraphPattern::Group {
                    inner,
                    variables,
                    aggregates,
                } => {
                    inner.hash(&mut previous);
                    variables.hash(&mut previous);
                    aggregates.hash(&mut previous);
                }
                GraphPattern::Service {
                    name,
                    inner,
                    silent,
                } => {
                    name.hash(&mut previous);
                    inner.hash(&mut previous);
                    silent.hash(&mut previous);
                }
                _ => unreachable!(),
            }
            assert_eq!(actual.0, previous.0);
        }
    }
}

#[cfg(test)]
mod source_literal_transport_tests {
    use super::*;
    use spargebra::{SourceDatatype, SourceLiteralOrigin};
    use std::hash::{Hash, Hasher};

    fn plain() -> ParsedSourceLiteral {
        ParsedSourceLiteral {
            occurrence: None,
            rdf: Literal::new_simple_literal("a"),
            datatype: SourceDatatype::Absent,
            language_original: None,
            origin: SourceLiteralOrigin::QueryPlain,
        }
    }

    #[test]
    fn source_literal_lowering_roundtrip_keeps_each_values_occurrence_and_empty_table() {
        let plain = plain();
        let typed = ParsedSourceLiteral {
            datatype: SourceDatatype::Specified(xsd::STRING.into_owned()),
            origin: SourceLiteralOrigin::QueryTypedExistingLiteral,
            ..plain.clone()
        };
        assert_eq!(plain.rdf, typed.rdf);
        for bindings in [
            vec![],
            vec![
                vec![Some(SourceValueTerm::Literal(plain.clone()))],
                vec![Some(SourceValueTerm::Literal(typed.clone()))],
                vec![None],
            ],
        ] {
            let original = AlGraphPattern::Extend {
                inner: Box::new(AlGraphPattern::SourceValues {
                    variables: vec![Variable::new_unchecked("x")],
                    bindings,
                }),
                variable: Variable::new_unchecked("copy"),
                expression: AlExpression::SourceLiteral(typed.clone()),
            };
            let lowered = GraphPattern::from(&original);
            assert_eq!(AlGraphPattern::from(&lowered), original);
            let GraphPattern::Extend {
                inner, expression, ..
            } = lowered
            else {
                panic!()
            };
            assert!(matches!(*inner, GraphPattern::SourceValues { .. }));
            assert!(matches!(expression, Expression::SourceLiteral(value) if value == typed));
        }
    }

    #[test]
    fn source_literal_append_retains_source_group_and_chain_hash_bytes() {
        #[derive(Default)]
        struct RecordingHasher(Vec<u8>);
        impl Hasher for RecordingHasher {
            fn finish(&self) -> u64 {
                0
            }
            fn write(&mut self, bytes: &[u8]) {
                self.0.extend_from_slice(bytes);
            }
        }
        let first = Box::new(Expression::Literal(1.into()));
        let tail = vec![(
            MultiplicativeOperator::Multiply,
            Expression::Literal(2.into()),
        )];
        let chain = Expression::MultiplicativeChain {
            first: first.clone(),
            tail: tail.clone(),
        };
        let mut actual = RecordingHasher::default();
        chain.hash(&mut actual);
        let mut previous = RecordingHasher::default();
        24_isize.hash(&mut previous);
        first.hash(&mut previous);
        tail.hash(&mut previous);
        assert_eq!(actual.0, previous.0);

        let inner = Box::new(GraphPattern::empty_singleton());
        let grouping = SourceGrouping::<Expression>::Implicit;
        let aggregates = vec![(
            Variable::new_unchecked("n"),
            AggregateExpression::CountSolutions { distinct: false },
        )];
        let group = GraphPattern::SourceGroup {
            inner: inner.clone(),
            grouping: grouping.clone(),
            aggregates: aggregates.clone(),
        };
        let mut actual = RecordingHasher::default();
        group.hash(&mut actual);
        let mut previous = RecordingHasher::default();
        (17_isize + isize::from(cfg!(feature = "sep-0006"))).hash(&mut previous);
        inner.hash(&mut previous);
        grouping.hash(&mut previous);
        aggregates.hash(&mut previous);
        assert_eq!(actual.0, previous.0);
    }
}

#[cfg(test)]
mod source_order_transport_tests {
    use super::*;

    fn raw_key() -> AlExpression {
        AlExpression::MultiplicativeChain {
            first: Box::new(AlExpression::AdditiveChain {
                first: Box::new(AlExpression::Variable(Variable::new_unchecked("missing"))),
                tail: vec![(AdditiveOperator::Add, AlExpression::Literal(0.into()))],
            }),
            tail: vec![(
                MultiplicativeOperator::Multiply,
                AlExpression::Literal(2.into()),
            )],
        }
    }

    #[test]
    fn source_order_lowering_roundtrip_preserves_keys_without_hidden_extend() {
        let original = AlGraphPattern::SourceOrderBy {
            inner: Box::new(AlGraphPattern::Values {
                variables: vec![Variable::new_unchecked("x")],
                bindings: vec![vec![Some(Literal::from(1).into())]],
            }),
            expression: vec![
                AlOrderExpression::Asc(raw_key()),
                AlOrderExpression::Desc(raw_key()),
            ],
        };
        let lowered = GraphPattern::from(&original);
        let GraphPattern::SourceOrderBy { inner, expression } = &lowered else {
            panic!()
        };
        assert!(matches!(&**inner, GraphPattern::Values { .. }));
        assert!(matches!(
            &expression[0],
            OrderExpression::Asc(Expression::MultiplicativeChain { .. })
        ));
        assert_eq!(AlGraphPattern::from(&lowered), original);
        let mut variables = Vec::new();
        lowered.lookup_used_variables(&mut |v| variables.push(v.clone()));
        assert_eq!(variables, [Variable::new_unchecked("x")]);
    }

    #[test]
    fn source_order_lowering_retains_empty_input_and_duplicate_conditions() {
        for expression in [
            vec![],
            vec![
                AlOrderExpression::Asc(raw_key()),
                AlOrderExpression::Asc(raw_key()),
            ],
        ] {
            let original = AlGraphPattern::SourceOrderBy {
                inner: Box::new(AlGraphPattern::Values {
                    variables: vec![Variable::new_unchecked("x")],
                    bindings: vec![],
                }),
                expression,
            };
            let lowered = GraphPattern::from(&original);
            assert!(
                matches!(&lowered, GraphPattern::SourceOrderBy { inner, .. } if matches!(&**inner, GraphPattern::Values { bindings, .. } if bindings.is_empty()))
            );
            assert_eq!(AlGraphPattern::from(&lowered), original);
        }
    }
}

#[cfg(test)]
mod source_order_hash_tests {
    use super::*;
    use std::hash::{Hash, Hasher};

    #[test]
    fn source_order_append_preserves_prior_graph_hash_bytes() {
        #[derive(Default)]
        struct RecordingHasher(Vec<u8>);
        impl Hasher for RecordingHasher {
            fn finish(&self) -> u64 {
                0
            }
            fn write(&mut self, bytes: &[u8]) {
                self.0.extend_from_slice(bytes);
            }
        }
        macro_rules! check {
            ($tag:expr, $pattern:expr, $($field:expr),+ $(,)?) => {{
                let mut actual = RecordingHasher::default();
                $pattern.hash(&mut actual);
                let mut previous = RecordingHasher::default();
                ($tag + isize::from(cfg!(feature = "sep-0006"))).hash(&mut previous);
                $($field.hash(&mut previous);)+
                assert_eq!(actual.0, previous.0);
            }};
        }
        let inner = Box::new(GraphPattern::empty_singleton());
        let variables = vec![Variable::new_unchecked("row")];
        let expression = vec![OrderExpression::Desc(Expression::Variable(
            variables[0].clone(),
        ))];
        let aggregates = vec![(
            Variable::new_unchecked("count"),
            AggregateExpression::CountSolutions { distinct: false },
        )];
        let grouping = SourceGrouping::<Expression>::Implicit;
        let name = NamedNodePattern::NamedNode(NamedNode::new_unchecked("urn:service"));
        let bindings: Vec<Vec<Option<SourceValueTerm>>> = vec![vec![None]];
        check!(
            10_isize,
            GraphPattern::OrderBy {
                inner: inner.clone(),
                expression: expression.clone()
            },
            inner,
            expression
        );
        check!(
            15_isize,
            GraphPattern::Group {
                inner: inner.clone(),
                variables: variables.clone(),
                aggregates: aggregates.clone()
            },
            inner,
            variables,
            aggregates
        );
        check!(
            16_isize,
            GraphPattern::Service {
                name: name.clone(),
                inner: inner.clone(),
                silent: false
            },
            name,
            inner,
            false
        );
        check!(
            17_isize,
            GraphPattern::SourceGroup {
                inner: inner.clone(),
                grouping: grouping.clone(),
                aggregates: aggregates.clone()
            },
            inner,
            grouping,
            aggregates
        );
        check!(
            18_isize,
            GraphPattern::SourceValues {
                variables: variables.clone(),
                bindings: bindings.clone()
            },
            variables,
            bindings
        );
    }
}

#[cfg(test)]
mod source_context_transport_tests {
    use super::*;

    fn table(name: &str) -> AlGraphPattern {
        AlGraphPattern::SourceContext {
            annotation: SourceContextAnnotation::Values {
                form: SourceValuesForm::Records,
            },
            inner: Box::new(AlGraphPattern::SourceValues {
                variables: vec![Variable::new_unchecked(name)],
                bindings: vec![vec![None]],
            }),
        }
    }

    #[test]
    fn source_context_lowering_roundtrip_keeps_annotation_and_true_filter_origin() {
        let left = table("left");
        let right = table("right");
        let original = AlGraphPattern::SourceContext {
            annotation: SourceContextAnnotation::LeftJoin {
                left_scope: SourceVariableScope::default(),
                unconditional: true,
            },
            inner: Box::new(AlGraphPattern::LeftJoin {
                left: Box::new(left),
                right: Box::new(right),
                expression: None,
            }),
        };
        let lowered = GraphPattern::from(&original);
        let GraphPattern::SourceContext { inner, annotation } = &lowered else {
            panic!()
        };
        assert!(matches!(
            annotation,
            SourceContextAnnotation::LeftJoin {
                unconditional: true,
                ..
            }
        ));
        assert!(matches!(&**inner, GraphPattern::LeftJoin { .. }));
        assert_eq!(AlGraphPattern::from(&lowered), original);
        let mut variables = Vec::new();
        lowered.lookup_used_variables(&mut |variable| variables.push(variable.clone()));
        assert_eq!(
            variables,
            vec![
                Variable::new_unchecked("left"),
                Variable::new_unchecked("right")
            ]
        );
    }

    #[test]
    fn source_context_lowering_preserves_group_none_and_invalid_values_without_simplification() {
        let originals = [
            AlGraphPattern::SourceContext {
                annotation: SourceContextAnnotation::Values {
                    form: SourceValuesForm::EmptyTranslatedList,
                },
                inner: Box::new(AlGraphPattern::SourceValues {
                    variables: vec![],
                    bindings: vec![vec![]],
                }),
            },
            AlGraphPattern::SourceContext {
                annotation: SourceContextAnnotation::Group {
                    conditions: vec![SourceGroupConditionContext {
                        syntax: SourceGroupConditionSyntax::ParenthesizedWithoutAlias,
                        assignment_scope: Some(SourceVariableScope::default()),
                    }],
                },
                inner: Box::new(AlGraphPattern::SourceGroup {
                    inner: Box::new(table("x")),
                    grouping: SourceGrouping::Explicit(vec![SourceGroupCondition::Raw(
                        AlExpression::Variable(Variable::new_unchecked("x")),
                    )]),
                    aggregates: vec![(
                        Variable::new_unchecked("n"),
                        AlAggregateExpression::CountSolutions { distinct: false },
                    )],
                }),
            },
        ];
        for original in originals {
            assert_eq!(
                AlGraphPattern::from(&GraphPattern::from(&original)),
                original
            );
        }
    }

    #[test]
    fn source_context_append_preserves_previous_source_order_hash_bytes() {
        #[derive(Default)]
        struct RecordingHasher(Vec<u8>);
        impl Hasher for RecordingHasher {
            fn finish(&self) -> u64 {
                0
            }
            fn write(&mut self, bytes: &[u8]) {
                self.0.extend_from_slice(bytes);
            }
        }
        let inner = Box::new(GraphPattern::empty_singleton());
        let expression = vec![OrderExpression::Asc(Expression::Variable(
            Variable::new_unchecked("x"),
        ))];
        let pattern = GraphPattern::SourceOrderBy {
            inner: inner.clone(),
            expression: expression.clone(),
        };
        let mut actual = RecordingHasher::default();
        pattern.hash(&mut actual);
        let mut expected = RecordingHasher::default();
        (19_isize + isize::from(cfg!(feature = "sep-0006"))).hash(&mut expected);
        inner.hash(&mut expected);
        expression.hash(&mut expected);
        assert_eq!(actual.0, expected.0);
        let mut actual = DefaultHasher::new();
        pattern.hash(&mut actual);
        let mut expected = DefaultHasher::new();
        (19_isize + isize::from(cfg!(feature = "sep-0006"))).hash(&mut expected);
        inner.hash(&mut expected);
        expression.hash(&mut expected);
        assert_eq!(actual.finish(), expected.finish());
    }
}
