//! Transactional helpers for agent-facing operations that already have
//! document-model implementations (variables, styles and components).
use crate::{edit::Tx, variables};
use desigma_doc::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExtrasError {
    #[error("invalid variable or collection state")]
    InvalidVariables,
    #[error("invalid style state")]
    InvalidStyles,
    #[error("node not found")]
    MissingNode,
    #[error("operation is not valid for this node")]
    InvalidComponentOperation,
    #[error("document operation failed: {0}")]
    Document(String),
}
impl From<DocError> for ExtrasError {
    fn from(value: DocError) -> Self {
        Self::Document(value.to_string())
    }
}

fn valid_variables(s: &VariableStore) -> bool {
    for (id, c) in &s.collections {
        if id != &c.id || c.name.trim().is_empty() || c.modes.is_empty() {
            return false;
        }
        let modes: BTreeSet<_> = c.modes.iter().map(|m| m.id.as_str()).collect();
        if modes.len() != c.modes.len() || !modes.contains(c.default_mode.as_str()) {
            return false;
        }
        let vars: BTreeSet<_> = c.variables.iter().map(String::as_str).collect();
        if vars.len() != c.variables.len() {
            return false;
        }
        for vid in &c.variables {
            let Some(v) = s.variables.get(vid) else { return false };
            if v.collection != *id || v.id != *vid || v.name.trim().is_empty() {
                return false;
            }
            for (mode, value) in &v.values {
                if !modes.contains(mode.as_str()) {
                    return false;
                }
                match value {
                    VariableValue::Color(color) if v.kind == VariableKind::Color => {
                        if [color.r, color.g, color.b, color.a].iter().any(|x| !x.is_finite() || !(0.0..=1.0).contains(x)) {
                            return false;
                        }
                    }
                    VariableValue::Number(n) if v.kind == VariableKind::Number => {
                        if !n.is_finite() || n.abs() > 1_000_000.0 {
                            return false;
                        }
                    }
                    VariableValue::String(_) if v.kind == VariableKind::String => {}
                    VariableValue::Boolean(_) if v.kind == VariableKind::Boolean => {}
                    VariableValue::Alias(target) if s.variables.get(target).is_some_and(|t| t.kind == v.kind) && s.can_alias(vid, target) => {}
                    _ => return false,
                }
            }
        }
    }
    s.variables.iter().all(|(id, v)| id == &v.id && s.collections.get(&v.collection).is_some_and(|c| c.variables.contains(id)))
}

/// Applies any VariableStore CRUD operation as one undo step, preserving the
/// current appearance when bindings disappear and reflowing surviving bindings.
pub fn apply_variable_edit(doc: &mut Document, tx: &mut Tx, edit: impl FnOnce(&mut VariableStore)) -> Result<bool, ExtrasError> {
    let old = VariableStore::from_doc(doc);
    let mut new = old.clone();
    edit(&mut new);
    if !valid_variables(&new) {
        return Err(ExtrasError::InvalidVariables);
    }
    if old == new {
        return Ok(false);
    }
    let removed_vars: Vec<_> = old.variables.keys().filter(|k| !new.variables.contains_key(*k)).cloned().collect();
    let removed_colls: Vec<_> = old.collections.keys().filter(|k| !new.collections.contains_key(*k)).cloned().collect();
    let affected = variables::changed_variables(doc, &new, &removed_vars);
    let cleanup = dangling_binding_cleanup(doc, &removed_vars, &removed_colls);
    let cleanup_ids: BTreeSet<_> = cleanup.iter().map(|n| n.id).collect();
    let bound = variables::bound_nodes(doc, &affected);
    for id in bound.iter().copied().chain(cleanup_ids.iter().copied()).collect::<BTreeSet<_>>() {
        tx.ed.touch(doc, id);
    }
    for mut replacement in cleanup {
        // Removed bindings bake their currently resolved values before they vanish.
        let keys: Vec<_> =
            replacement.variables.bindings.keys().filter(|k| removed_vars.iter().any(|v| replacement.variables.bindings.get(*k) == Some(v))).cloned().collect();
        bake_bindings(doc, &mut replacement, &keys);
        if let Some(node) = tx.ed.node(doc, replacement.id) {
            *node = replacement;
        }
    }
    tx.cmd(doc, new.command())?;
    variables::reflow_bound(doc, &affected, &bound);
    Ok(true)
}

fn valid_styles(styles: &BTreeMap<String, Style>, meta: &BTreeMap<String, StyleMeta>) -> bool {
    styles.keys().all(|k| !k.trim().is_empty())
        && meta.keys().all(|k| styles.contains_key(k))
        && meta.values().all(|m| m.name.trim().len() <= 256 && m.description.len() <= 16_384)
}

/// Runs shared-style CRUD against a working copy and commits as one command.
pub fn apply_style_edit(
    doc: &mut Document,
    tx: &mut Tx,
    edit: impl FnOnce(&mut BTreeMap<String, Style>, &mut BTreeMap<String, StyleMeta>),
) -> Result<bool, ExtrasError> {
    let mut styles = doc.styles.clone();
    let mut meta = doc.style_meta.clone();
    edit(&mut styles, &mut meta);
    if !valid_styles(&styles, &meta) {
        return Err(ExtrasError::InvalidStyles);
    }
    if serde_json::to_value(&styles).ok() == serde_json::to_value(&doc.styles).ok() && meta == doc.style_meta {
        return Ok(false);
    }
    tx.cmd(doc, Command::SetStyles { styles, style_meta: meta })?;
    Ok(true)
}

fn apply_component(doc: &mut Document, tx: &mut Tx, cmd: Command) -> Result<bool, ExtrasError> {
    if matches!(&cmd, Command::Batch(items) if items.is_empty()) {
        return Ok(false);
    }
    tx.cmd(doc, cmd)?;
    Ok(true)
}

fn is_instance_root(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Instance { component: Some(_), .. }) && is_top_instance(n)
}

pub fn create_component(doc: &mut Document, tx: &mut Tx, id: NodeId) -> Result<bool, ExtrasError> {
    if doc.get(id).is_none() {
        return Err(ExtrasError::MissingNode);
    }
    let cmd = desigma_doc::create_component(doc, id);
    apply_component(doc, tx, cmd)
}

pub fn detach_instance(doc: &mut Document, tx: &mut Tx, id: NodeId) -> Result<bool, ExtrasError> {
    if !doc.get(id).is_some_and(is_instance_root) {
        return Err(ExtrasError::InvalidComponentOperation);
    }
    let command = desigma_doc::detach_instance(doc, id);
    apply_component(doc, tx, command)
}

pub fn reset_instance(doc: &mut Document, tx: &mut Tx, id: NodeId) -> Result<bool, ExtrasError> {
    if !doc.get(id).is_some_and(is_instance_root) {
        return Err(ExtrasError::InvalidComponentOperation);
    }
    let command = desigma_doc::reset_instance(doc, id);
    apply_component(doc, tx, command)
}

pub fn reset_overrides(doc: &mut Document, tx: &mut Tx, id: NodeId, fields: &[OverrideField]) -> Result<bool, ExtrasError> {
    if doc.get(id).is_none() {
        return Err(ExtrasError::MissingNode);
    }
    let command = desigma_doc::reset_overrides(doc, id, fields);
    apply_component(doc, tx, command)
}

pub fn swap_instance(doc: &mut Document, tx: &mut Tx, id: NodeId, component: NodeId) -> Result<bool, ExtrasError> {
    if !doc.get(id).is_some_and(is_instance_root) || !doc.get(component).is_some_and(|n| matches!(n.kind, NodeKind::Component(_))) {
        return Err(ExtrasError::InvalidComponentOperation);
    }
    let command = desigma_doc::swap_instance(doc, id, component);
    apply_component(doc, tx, command)
}

pub fn set_instance_property(doc: &mut Document, tx: &mut Tx, id: NodeId, name: &str, value: Option<PropValue>) -> Result<bool, ExtrasError> {
    if !doc.get(id).is_some_and(is_instance_root) {
        return Err(ExtrasError::InvalidComponentOperation);
    }
    let command = desigma_doc::set_instance_prop(doc, id, name, value);
    apply_component(doc, tx, command)
}

pub fn set_component_properties(doc: &mut Document, tx: &mut Tx, id: NodeId, props: Vec<ComponentPropDef>) -> Result<bool, ExtrasError> {
    let n = doc.get(id).ok_or(ExtrasError::MissingNode)?;
    if !matches!(n.kind, NodeKind::Component(_) | NodeKind::ComponentSet(_)) {
        return Err(ExtrasError::InvalidComponentOperation);
    }
    let mut names = BTreeSet::new();
    if props.iter().any(|p| p.name.trim().is_empty() || !names.insert(p.name.clone())) {
        return Err(ExtrasError::InvalidComponentOperation);
    }
    let mut node = n.clone();
    node.component.props = props;
    apply_component(doc, tx, Command::Update { node: Box::new(node) })
}

pub fn list_component_properties(doc: &Document, id: NodeId) -> Result<Vec<ComponentPropDef>, ExtrasError> {
    let n = doc.get(id).ok_or(ExtrasError::MissingNode)?;
    if !matches!(n.kind, NodeKind::Component(_) | NodeKind::ComponentSet(_)) {
        return Err(ExtrasError::InvalidComponentOperation);
    }
    Ok(prop_defs(doc, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Document, NodeId) {
        let mut doc = Document::new("extras");
        let page = doc.add_page("Page");
        let id = doc.alloc_id();
        doc.insert(page, Node::frame(id, Rect::new(0., 0., 100., 80.))).unwrap();
        (doc, id)
    }

    #[test]
    fn variable_crud_reflows_and_undoes_as_one_step() {
        let (mut doc, _) = fixture();
        let mut tx = Tx::new();
        let mut new_variable = None;
        apply_variable_edit(&mut doc, &mut tx, |store| {
            let c = store.add_collection("Theme");
            new_variable = store.add_variable(&c, "Spacing", VariableKind::Number);
        })
        .unwrap();
        let mut history = History::default();
        assert!(tx.commit(&mut doc, &mut history).unwrap());
        assert!(new_variable.as_ref().is_some_and(|id| doc.variables.contains_key(id)));
        history.undo(&mut doc).unwrap();
        assert!(doc.variables.is_empty());
        assert!(doc.variable_collections.is_empty());
    }

    #[test]
    fn style_crud_commits_and_undoes() {
        let (mut doc, _) = fixture();
        let mut tx = Tx::new();
        apply_style_edit(&mut doc, &mut tx, |styles, meta| {
            styles.insert("fill:primary".into(), Style::Fill(vec![Paint::solid(Color::WHITE)]));
            meta.insert("fill:primary".into(), StyleMeta { name: "Primary".into(), ..Default::default() });
        })
        .unwrap();
        let mut history = History::default();
        assert!(tx.commit(&mut doc, &mut history).unwrap());
        assert!(doc.styles.contains_key("fill:primary"));
        history.undo(&mut doc).unwrap();
        assert!(doc.styles.is_empty());
    }

    #[test]
    fn component_lifecycle_helper_uses_undoable_command() {
        let (mut doc, id) = fixture();
        let mut tx = Tx::new();
        assert!(create_component(&mut doc, &mut tx, id).unwrap());
        let mut history = History::default();
        assert!(tx.commit(&mut doc, &mut history).unwrap());
        assert!(matches!(doc.get(id).unwrap().kind, NodeKind::Component(_)));
        history.undo(&mut doc).unwrap();
        assert!(matches!(doc.get(id).unwrap().kind, NodeKind::Frame(_)));
    }

    #[test]
    fn component_operations_reject_wrong_node_kind() {
        let (mut doc, id) = fixture();
        let mut tx = Tx::new();
        assert_eq!(detach_instance(&mut doc, &mut tx, id), Err(ExtrasError::InvalidComponentOperation));
        assert!(tx.ed.touched().is_empty());
    }
}
