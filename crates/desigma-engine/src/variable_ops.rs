//! Typed, validated variable collection and node-binding operations.
use crate::{edit::Tx, variables};
use desigma_doc::*;
use std::collections::HashSet;

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum VariableOpError {
    #[error("variable, collection, mode, or node was not found")]
    Missing,
    #[error("invalid variable operation: {0}")]
    Invalid(&'static str),
    #[error("document operation failed: {0}")]
    Document(String),
}
impl From<DocError> for VariableOpError {
    fn from(e: DocError) -> Self {
        Self::Document(e.to_string())
    }
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.trim().len() <= 256 && !name.chars().any(char::is_control)
}

fn store_edit(doc: &mut Document, tx: &mut Tx, edit: impl FnOnce(&mut VariableStore)) -> Result<bool, VariableOpError> {
    let mut staged = doc.clone();
    let mut staged_tx = Tx::new();
    let changed = crate::agent_extras::apply_variable_edit(&mut staged, &mut staged_tx, edit).map_err(|e| {
        VariableOpError::Invalid(match e {
            crate::agent_extras::ExtrasError::InvalidVariables => "variable store validation",
            _ => "variable store edit",
        })
    })?;
    if !changed {
        return Ok(false);
    }
    tx.cmd(doc, Command::SetVariables { variables: staged.variables.clone(), collections: staged.variable_collections.clone() })?;
    copy_changed_nodes(doc, tx, &staged);
    Ok(true)
}

fn copy_changed_nodes(doc: &mut Document, tx: &mut Tx, staged: &Document) {
    let changed: Vec<_> = staged.nodes.iter().filter_map(|(id, node)| (doc.get(*id) != Some(node)).then_some((*id, node.clone()))).collect();
    for (id, replacement) in changed {
        if let Some(node) = tx.ed.node(doc, id) {
            *node = replacement;
        }
    }
}

pub fn create_collection(doc: &mut Document, tx: &mut Tx, name: &str) -> Result<String, VariableOpError> {
    if !valid_name(name) {
        return Err(VariableOpError::Invalid("collection name"));
    }
    let mut result = None;
    store_edit(doc, tx, |store| result = Some(store.add_collection(name.trim())))?;
    result.ok_or(VariableOpError::Invalid("collection creation"))
}

pub fn create_mode(doc: &mut Document, tx: &mut Tx, collection_id: &str, name: &str) -> Result<String, VariableOpError> {
    if !valid_name(name) {
        return Err(VariableOpError::Invalid("mode name"));
    }
    if !doc.variable_collections.contains_key(collection_id) {
        return Err(VariableOpError::Missing);
    }
    let mut result = None;
    store_edit(doc, tx, |store| result = store.add_mode(collection_id, name.trim()))?;
    result.ok_or(VariableOpError::Missing)
}

pub fn create_variable(doc: &mut Document, tx: &mut Tx, collection_id: &str, name: &str, kind: VariableKind) -> Result<String, VariableOpError> {
    if !valid_name(name) {
        return Err(VariableOpError::Invalid("variable name"));
    }
    if !doc.variable_collections.contains_key(collection_id) {
        return Err(VariableOpError::Missing);
    }
    let mut result = None;
    store_edit(doc, tx, |store| result = store.add_variable(collection_id, name.trim(), kind))?;
    result.ok_or(VariableOpError::Missing)
}

pub fn rename_variable(doc: &mut Document, tx: &mut Tx, variable_id: &str, name: &str) -> Result<bool, VariableOpError> {
    if !doc.variables.contains_key(variable_id) {
        return Err(VariableOpError::Missing);
    }
    if !valid_name(name) {
        return Err(VariableOpError::Invalid("variable name"));
    }
    store_edit(doc, tx, |store| store.rename_variable(variable_id, name.trim()))
}

pub fn delete_variable(doc: &mut Document, tx: &mut Tx, variable_id: &str) -> Result<bool, VariableOpError> {
    if !doc.variables.contains_key(variable_id) {
        return Err(VariableOpError::Missing);
    }
    store_edit(doc, tx, |store| store.delete_variable(variable_id))
}

fn supported_property(node: &Node, property: &str) -> Option<VariableKind> {
    let indexed_paint = |prefix: &str, paints: &[Paint]| {
        let index = property.strip_prefix(prefix)?.parse::<usize>().ok()?;
        matches!(paints.get(index)?.kind, PaintKind::Solid(_)).then_some(VariableKind::Color)
    };
    if property.starts_with("fills/") {
        let fills = node.kind.any_text().map(|t| t.style.fills.as_slice()).unwrap_or(node.fills.as_slice());
        return indexed_paint("fills/", fills);
    }
    if property.starts_with("strokes/") {
        return indexed_paint("strokes/", &node.stroke.paints);
    }
    match property {
        "visible" => Some(VariableKind::Boolean),
        "characters" if node.kind.any_text().is_some() => Some(VariableKind::String),
        "width" | "height" | "opacity" | "topLeftRadius" | "topRightRadius" | "bottomRightRadius" | "bottomLeftRadius" | "strokeWeight" | "strokeTopWeight"
        | "strokeRightWeight" | "strokeBottomWeight" | "strokeLeftWeight" => Some(VariableKind::Number),
        "itemSpacing" | "counterAxisSpacing" | "paddingTop" | "paddingRight" | "paddingBottom" | "paddingLeft"
            if node.kind.frame_props().is_some_and(|f| f.auto_layout.is_some()) =>
        {
            Some(VariableKind::Number)
        }
        "fontSize" | "fontWeight" | "lineHeight" | "letterSpacing" if node.kind.any_text().is_some() => Some(VariableKind::Number),
        _ => None,
    }
}

fn validate_binding(doc: &Document, node_id: NodeId, property: &str, variable_id: &str) -> Result<(), VariableOpError> {
    let node = doc.get(node_id).ok_or(VariableOpError::Missing)?;
    let variable = doc.variables.get(variable_id).ok_or(VariableOpError::Missing)?;
    let expected = supported_property(node, property).ok_or(VariableOpError::Invalid("unsupported property binding"))?;
    if expected != variable.kind {
        return Err(VariableOpError::Invalid("variable type does not match property"));
    }
    if doc.resolve_variable(node_id, variable_id).is_none() {
        return Err(VariableOpError::Invalid("variable has no resolvable value"));
    }
    Ok(())
}

pub fn bind_property(doc: &mut Document, tx: &mut Tx, node_id: NodeId, property: &str, variable_id: &str) -> Result<bool, VariableOpError> {
    validate_binding(doc, node_id, property, variable_id)?;
    let mut staged = doc.clone();
    let node = staged.get_mut(node_id).ok_or(VariableOpError::Missing)?;
    if node.variables.bindings.get(property).is_some_and(|id| id == variable_id) {
        return Ok(false);
    }
    node.variables.bindings.insert(property.to_string(), variable_id.to_string());
    let vars = HashSet::from([variable_id.to_string()]);
    let nodes = vec![node_id];
    variables::reflow_bound(&mut staged, &vars, &nodes);
    copy_changed_nodes(doc, tx, &staged);
    Ok(true)
}

pub fn unbind_property(doc: &mut Document, tx: &mut Tx, node_id: NodeId, property: &str) -> Result<bool, VariableOpError> {
    if supported_property(doc.get(node_id).ok_or(VariableOpError::Missing)?, property).is_none() {
        return Err(VariableOpError::Invalid("unsupported property binding"));
    }
    if !doc.get(node_id).unwrap().variables.bindings.contains_key(property) {
        return Ok(false);
    }
    let node = tx.ed.node(doc, node_id).ok_or(VariableOpError::Missing)?;
    node.variables.bindings.remove(property);
    // The resolved value is already baked into the property, so unbinding preserves appearance.
    Ok(true)
}

/// Set an explicit collection mode at `node`; `None` removes the override and
/// restores inherited mode resolution. Descendant bindings are reflowed too.
pub fn set_node_mode(doc: &mut Document, tx: &mut Tx, node_id: NodeId, collection_id: &str, mode_id: Option<&str>) -> Result<bool, VariableOpError> {
    let target = doc.get(node_id).ok_or(VariableOpError::Missing)?;
    let collection = doc.variable_collections.get(collection_id).ok_or(VariableOpError::Missing)?;
    if mode_id.is_some_and(|mode| !collection.modes.iter().any(|m| m.id == mode)) {
        return Err(VariableOpError::Missing);
    }
    if target.variables.modes.get(collection_id).map(String::as_str) == mode_id {
        return Ok(false);
    }
    let mut staged = doc.clone();
    let target = staged.get_mut(node_id).ok_or(VariableOpError::Missing)?;
    match mode_id {
        Some(mode) => {
            target.variables.modes.insert(collection_id.into(), mode.into());
        }
        None => {
            target.variables.modes.remove(collection_id);
        }
    }

    let mut affected_vars = HashSet::new();
    let mut bound_nodes = Vec::new();
    for (id, node) in &doc.nodes {
        for (property, variable) in &node.variables.bindings {
            let before = doc.bound_value(*id, property);
            let after = staged.bound_value(*id, property);
            if before != after {
                affected_vars.insert(variable.clone());
                bound_nodes.push(*id);
            }
        }
    }
    if !affected_vars.is_empty() {
        variables::reflow_bound(&mut staged, &affected_vars, &bound_nodes);
    }
    copy_changed_nodes(doc, tx, &staged);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Document, NodeId, NodeId) {
        let mut doc = Document::new("variables");
        let page = doc.add_page("Page");
        let frame = doc.alloc_id();
        doc.insert(page, Node::frame(frame, Rect::new(0., 0., 200., 120.))).unwrap();
        let rect = doc.alloc_id();
        doc.insert(frame, Node::rectangle(rect, Rect::new(0., 0., 40., 20.))).unwrap();
        (doc, frame, rect)
    }

    #[test]
    fn typed_collection_mode_variable_crud_is_undoable() {
        let (mut doc, _, _) = fixture();
        let mut tx = Tx::new();
        let collection = create_collection(&mut doc, &mut tx, "Theme").unwrap();
        let mode = create_mode(&mut doc, &mut tx, &collection, "Dark").unwrap();
        let variable = create_variable(&mut doc, &mut tx, &collection, "Spacing", VariableKind::Number).unwrap();
        rename_variable(&mut doc, &mut tx, &variable, "Space/Small").unwrap();
        let mut history = History::default();
        assert!(tx.commit(&mut doc, &mut history).unwrap());
        assert_eq!(doc.variables[&variable].name, "Space/Small");
        assert!(doc.variables[&variable].values.contains_key(&mode));
        history.undo(&mut doc).unwrap();
        assert!(doc.variables.is_empty());
        assert!(doc.variable_collections.is_empty());
    }

    #[test]
    fn bindings_validate_property_types_bake_unbind_and_undo() {
        let (mut doc, _, rect) = fixture();
        let mut tx = Tx::new();
        let coll = create_collection(&mut doc, &mut tx, "Sizing").unwrap();
        let width = create_variable(&mut doc, &mut tx, &coll, "Width", VariableKind::Number).unwrap();
        doc.variables.get_mut(&width).unwrap().values.values_mut().for_each(|v| *v = VariableValue::Number(75.0));
        assert_eq!(bind_property(&mut doc, &mut tx, rect, "width", &width), Ok(true));
        assert_eq!(doc.get(rect).unwrap().size.x, 75.0);
        assert!(bind_property(&mut doc, &mut tx, rect, "unknown", &width).is_err());
        assert!(unbind_property(&mut doc, &mut tx, rect, "width").unwrap());
        assert_eq!(doc.get(rect).unwrap().size.x, 75.0);
        let mut history = History::default();
        assert!(tx.commit(&mut doc, &mut history).unwrap());
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.get(rect).unwrap().size.x, 40.0);
        assert!(!doc.get(rect).unwrap().variables.bindings.contains_key("width"));
    }

    #[test]
    fn deleting_bound_variable_bakes_value_and_undo_restores_binding() {
        let (mut doc, _, rect) = fixture();
        let mut setup = Tx::new();
        let coll = create_collection(&mut doc, &mut setup, "Sizing").unwrap();
        let width = create_variable(&mut doc, &mut setup, &coll, "Width", VariableKind::Number).unwrap();
        doc.variables.get_mut(&width).unwrap().values.values_mut().for_each(|v| *v = VariableValue::Number(64.0));
        bind_property(&mut doc, &mut setup, rect, "width", &width).unwrap();
        let mut history = History::default();
        setup.commit(&mut doc, &mut history).unwrap();
        let mut deletion = Tx::new();
        delete_variable(&mut doc, &mut deletion, &width).unwrap();
        assert_eq!(doc.get(rect).unwrap().size.x, 64.0);
        assert!(!doc.get(rect).unwrap().variables.bindings.contains_key("width"));
        deletion.commit(&mut doc, &mut history).unwrap();
        history.undo(&mut doc).unwrap();
        assert!(doc.variables.contains_key(&width));
        assert_eq!(doc.get(rect).unwrap().variables.bindings["width"], width);
        assert_eq!(doc.get(rect).unwrap().size.x, 64.0);
    }

    #[test]
    fn node_mode_reflows_bound_descendants_and_rejects_invalid_mode() {
        let (mut doc, frame, rect) = fixture();
        let mut tx = Tx::new();
        let coll = create_collection(&mut doc, &mut tx, "Theme").unwrap();
        let dark = create_mode(&mut doc, &mut tx, &coll, "Dark").unwrap();
        let width = create_variable(&mut doc, &mut tx, &coll, "Width", VariableKind::Number).unwrap();
        let modes = doc.variable_collections[&coll].modes.clone();
        let light = modes[0].id.clone();
        let v = doc.variables.get_mut(&width).unwrap();
        v.values.insert(light, VariableValue::Number(50.0));
        v.values.insert(dark.clone(), VariableValue::Number(90.0));
        bind_property(&mut doc, &mut tx, rect, "width", &width).unwrap();
        assert_eq!(doc.get(rect).unwrap().size.x, 50.0);
        assert!(set_node_mode(&mut doc, &mut tx, frame, &coll, Some(&dark)).is_ok());
        assert_eq!(doc.get(rect).unwrap().size.x, 90.0);
        assert!(set_node_mode(&mut doc, &mut tx, frame, &coll, Some("missing")).is_err());
        let mut history = History::default();
        tx.commit(&mut doc, &mut history).unwrap();
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.get(rect).unwrap().size.x, 40.0);
        assert!(!doc.get(frame).unwrap().variables.modes.contains_key(&coll));
    }
}
