//! Helpers for editing variable values with the prototype expression language.
use crate::edit::Tx;
use desigma_doc::*;

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum PrototypeVariableError {
    #[error("variable or mode was not found")]
    Missing,
    #[error("invalid expression: {0}")]
    Expression(String),
    #[error("expression result does not match the variable type")]
    TypeMismatch,
    #[error("invalid variable value")]
    InvalidValue,
    #[error("document operation failed: {0}")]
    Document(String),
}

impl From<DocError> for PrototypeVariableError {
    fn from(e: DocError) -> Self {
        Self::Document(e.to_string())
    }
}

/// Parse an expression using variable display names, resolving only unique names.
pub fn parse_variable_expression(doc: &Document, source: &str) -> Result<Expression, PrototypeVariableError> {
    parse_expression(source, &|name| {
        let mut matches = doc.variables.values().filter(|v| v.name == name).map(|v| v.id.clone());
        let id = matches.next()?;
        matches.next().is_none().then_some(id)
    })
    .map_err(|e| PrototypeVariableError::Expression(e.to_string()))
}

/// Render an expression with current variable display names.
pub fn variable_expression_source(doc: &Document, expression: &Expression) -> String {
    expression.to_source(&|id| doc.variables.get(id).map(|v| v.name.clone()))
}

/// Evaluate and store an expression as the literal value for a variable mode.
/// References resolve in the selected mode where available and otherwise use
/// each referenced collection's default mode.
pub fn set_variable_expression(
    doc: &mut Document,
    tx: &mut Tx,
    variable_id: &str,
    mode_id: &str,
    expression: &Expression,
) -> Result<bool, PrototypeVariableError> {
    let variable = doc.variables.get(variable_id).ok_or(PrototypeVariableError::Missing)?;
    let collection = doc.variable_collections.get(&variable.collection).ok_or(PrototypeVariableError::Missing)?;
    if !collection.modes.iter().any(|m| m.id == mode_id) {
        return Err(PrototypeVariableError::Missing);
    }
    if !expression.within_limits() {
        return Err(PrototypeVariableError::Expression("expression exceeds limits".into()));
    }
    let ty = expression.type_of(&|id| doc.variables.get(id).map(|v| v.kind)).map_err(PrototypeVariableError::Expression)?;
    if ty != ExprType::from(variable.kind) && variable.kind != VariableKind::String {
        return Err(PrototypeVariableError::TypeMismatch);
    }
    let store = VariableStore::from_doc(doc);
    let result = expression
        .eval(&|id| {
            let v = store.variables.get(id)?;
            let c = store.collections.get(&v.collection)?;
            let mode = if c.modes.iter().any(|m| m.id == mode_id) { mode_id } else { &c.default_mode };
            Some(ExprValue::from(&store.resolve(id, mode)?))
        })
        .map_err(|e| PrototypeVariableError::Expression(e.to_string()))?;
    let value = result.to_variable_value(variable.kind).ok_or(PrototypeVariableError::TypeMismatch)?;
    match &value {
        VariableValue::Number(n) if !n.is_finite() || n.abs() > 1_000_000.0 => return Err(PrototypeVariableError::InvalidValue),
        VariableValue::String(s) if s.len() > 4096 => return Err(PrototypeVariableError::InvalidValue),
        VariableValue::Color(c) if ![c.r, c.g, c.b, c.a].iter().all(|x| x.is_finite() && (0.0..=1.0).contains(x)) => {
            return Err(PrototypeVariableError::InvalidValue)
        }
        _ => {}
    }
    let mut staged = doc.clone();
    let mut staged_tx = Tx::new();
    crate::agent_extras::apply_variable_edit(&mut staged, &mut staged_tx, |s| {
        s.set_value(variable_id, mode_id, value.clone());
    })
    .map_err(|e| PrototypeVariableError::Expression(e.to_string()))?;
    if staged.variables.get(variable_id).and_then(|v| v.values.get(mode_id)) == doc.variables.get(variable_id).and_then(|v| v.values.get(mode_id)) {
        return Ok(false);
    }
    tx.cmd(doc, Command::SetVariables { variables: staged.variables.clone(), collections: staged.variable_collections.clone() })?;
    for (id, replacement) in &staged.nodes {
        if doc.get(*id) != Some(replacement) {
            if let Some(node) = tx.ed.node(doc, *id) {
                *node = replacement.clone();
            }
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expression_value_edits_are_typed_transactional_and_undoable() {
        let mut doc = Document::new("expressions");
        let mut tx = Tx::new();
        let collection = crate::variable_ops::create_collection(&mut doc, &mut tx, "Cart").unwrap();
        let source = crate::variable_ops::create_variable(&mut doc, &mut tx, &collection, "Count", VariableKind::Number).unwrap();
        let total = crate::variable_ops::create_variable(&mut doc, &mut tx, &collection, "Total", VariableKind::Number).unwrap();
        let mode = doc.variable_collections[&collection].default_mode.clone();
        let mut history = History::default();
        tx.commit(&mut doc, &mut history).unwrap();
        doc.variables.get_mut(&source).unwrap().values.insert(mode.clone(), VariableValue::Number(4.0));
        let mut tx = Tx::new();
        let expr = parse_variable_expression(&doc, "$Count * 2").unwrap();
        assert_eq!(variable_expression_source(&doc, &expr), "$Count * 2");
        assert!(set_variable_expression(&mut doc, &mut tx, &total, &mode, &expr).unwrap());
        assert_eq!(doc.variables[&total].values[&mode], VariableValue::Number(8.0));
        assert!(!set_variable_expression(&mut doc, &mut tx, &total, &mode, &expr).unwrap());
        assert!(tx.commit(&mut doc, &mut history).unwrap());
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.variables[&total].values[&mode], VariableValue::Number(0.0));
    }

    #[test]
    fn ambiguous_names_and_wrong_types_are_rejected_without_mutation() {
        let mut doc = Document::new("expressions");
        let mut tx = Tx::new();
        let c1 = crate::variable_ops::create_collection(&mut doc, &mut tx, "A").unwrap();
        let c2 = crate::variable_ops::create_collection(&mut doc, &mut tx, "B").unwrap();
        let a = crate::variable_ops::create_variable(&mut doc, &mut tx, &c1, "Value", VariableKind::Number).unwrap();
        let _b = crate::variable_ops::create_variable(&mut doc, &mut tx, &c2, "Value", VariableKind::Number).unwrap();
        assert!(matches!(parse_variable_expression(&doc, "$Value + 1"), Err(PrototypeVariableError::Expression(_))));
        let mode = doc.variable_collections[&c1].default_mode.clone();
        assert_eq!(set_variable_expression(&mut doc, &mut tx, &a, &mode, &Expression::boolean(true)), Err(PrototypeVariableError::TypeMismatch));
        assert!(tx.ed.touched().is_empty());
    }
}
