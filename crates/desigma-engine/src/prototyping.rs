//! Validated transactional helpers for prototype reactions, flows and device settings.
use crate::edit::Tx;
use desigma_doc::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum PrototypeEditError {
    #[error("node not found")]
    MissingNode,
    #[error("invalid prototype data: {0}")]
    Invalid(&'static str),
    #[error("document operation failed: {0}")]
    Document(String),
}
impl From<DocError> for PrototypeEditError {
    fn from(e: DocError) -> Self {
        Self::Document(e.to_string())
    }
}

fn finite(v: f32) -> bool {
    v.is_finite() && v.abs() <= 1_000_000.0
}
fn color_ok(c: Color) -> bool {
    [c.r, c.g, c.b, c.a].iter().all(|x| x.is_finite() && (0.0..=1.0).contains(x))
}
fn valid_expr(doc: &Document, e: &Expression) -> bool {
    e.within_limits() && e.variables().iter().all(|id| doc.variables.contains_key(id)) && e.type_of(&|id| doc.variables.get(id).map(|v| v.kind)).is_ok()
}

fn valid_action(doc: &Document, action: &Action, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    match action {
        Action::Navigate { dest, .. }
        | Action::OpenOverlay { dest, .. }
        | Action::SwapOverlay { dest }
        | Action::ScrollTo { dest, .. }
        | Action::ChangeTo { dest }
        | Action::MediaControl { dest, .. } => {
            if doc.get(*dest).is_none() {
                return false;
            }
            match action {
                Action::OpenOverlay { offset, background, .. } => finite(offset.x) && finite(offset.y) && background.is_none_or(color_ok),
                Action::ScrollTo { offset, .. } => finite(offset.x) && finite(offset.y),
                Action::MediaControl { op: MediaOp::SkipBy(s) | MediaOp::SeekTo(s), .. } => finite(*s) && *s >= 0.0,
                _ => true,
            }
        }
        Action::OpenUrl { url, .. } => {
            let u = url.trim();
            (u.starts_with("https://") || u.starts_with("http://") || u.starts_with("mailto:")) && u.len() <= 2048
        }
        Action::SetVariable { variable, value } => doc.variables.get(variable).is_some_and(|v| {
            valid_expr(doc, value)
                && value.type_of(&|id| doc.variables.get(id).map(|x| x.kind)).is_ok_and(|t| t == ExprType::from(v.kind) || v.kind == VariableKind::String)
        }),
        Action::SetVariableMode { collection, mode } => doc.variable_collections.get(collection).is_some_and(|c| c.modes.iter().any(|m| m.id == *mode)),
        Action::Conditional { branches, otherwise } => {
            branches.len() <= 32
                && branches.iter().all(|(e, a)| {
                    valid_expr(doc, e)
                        && e.type_of(&|id| doc.variables.get(id).map(|x| x.kind)).is_ok_and(|t| t == ExprType::Boolean)
                        && a.len() <= 64
                        && a.iter().all(|x| valid_action(doc, x, depth + 1))
                })
                && otherwise.len() <= 64
                && otherwise.iter().all(|x| valid_action(doc, x, depth + 1))
        }
        Action::CloseOverlay | Action::Back => true,
    }
}

fn valid_reaction(doc: &Document, r: &Reaction) -> bool {
    r.actions.len() <= 64
        && r.actions.iter().all(|a| valid_action(doc, a, 0))
        && r.transition.is_none_or(|t| {
            t.duration_ms <= 60_000
                && match t.easing {
                    Easing::Bezier(v) => v.iter().all(|x| finite(*x)),
                    Easing::Spring { mass, stiffness, damping } => {
                        finite(mass) && finite(stiffness) && finite(damping) && mass > 0.0 && stiffness > 0.0 && damping >= 0.0
                    }
                    _ => true,
                }
        })
        && match &r.trigger {
            Trigger::KeyPress { keys } | Trigger::GamepadPress { buttons: keys, .. } => !keys.trim().is_empty() && keys.len() <= 128,
            Trigger::MouseEnter { delay_ms }
            | Trigger::MouseLeave { delay_ms }
            | Trigger::MouseDown { delay_ms }
            | Trigger::MouseUp { delay_ms }
            | Trigger::AfterDelay { delay_ms } => *delay_ms <= 60_000,
            _ => true,
        }
}

/// Replace prototype reactions on one node; destinations and expressions are validated before mutation.
pub fn set_reactions(doc: &mut Document, tx: &mut Tx, id: NodeId, reactions: Vec<Reaction>) -> Result<bool, PrototypeEditError> {
    let node = doc.get(id).ok_or(PrototypeEditError::MissingNode)?;
    if reactions.len() > 128 || !reactions.iter().all(|r| valid_reaction(doc, r)) {
        return Err(PrototypeEditError::Invalid("reactions"));
    }
    if node.prototype.reactions == reactions {
        return Ok(false);
    }
    tx.ed.node(doc, id).ok_or(PrototypeEditError::MissingNode)?.prototype.reactions = reactions;
    Ok(true)
}

fn valid_flows(doc: &Document, flows: &[Flow]) -> bool {
    if flows.len() > 128 {
        return false;
    }
    let mut names = BTreeSet::new();
    flows.iter().all(|f| {
        !f.name.trim().is_empty()
            && f.name.len() <= 256
            && f.description.len() <= 16_384
            && names.insert((f.page, f.name.to_lowercase()))
            && doc.get(f.page).is_some_and(|n| matches!(n.kind, NodeKind::Page { .. }))
            && doc.get(f.start).is_some_and(|n| n.parent == Some(f.page) && n.kind.frame_props().is_some())
    })
}

/// Applies flow CRUD to a working list as one undoable SetFlows command.
pub fn edit_flows(doc: &mut Document, tx: &mut Tx, edit: impl FnOnce(&mut Vec<Flow>)) -> Result<bool, PrototypeEditError> {
    let mut flows = doc.flows.clone();
    edit(&mut flows);
    if !valid_flows(doc, &flows) {
        return Err(PrototypeEditError::Invalid("flows"));
    }
    if flows == doc.flows {
        return Ok(false);
    }
    tx.cmd(doc, Command::SetFlows { flows })?;
    Ok(true)
}

/// Changes the device preview configuration with bounded geometry and colors.
pub fn set_device(doc: &mut Document, tx: &mut Tx, device: PrototypeDevice) -> Result<bool, PrototypeEditError> {
    if device.preset.as_ref().is_some_and(|s| s.trim().is_empty() || s.len() > 128)
        || device.size.is_some_and(|s| !finite(s.x) || !finite(s.y) || s.x <= 0.0 || s.y <= 0.0)
        || device.background.is_some_and(|c| !color_ok(c))
    {
        return Err(PrototypeEditError::Invalid("prototype device"));
    }
    if doc.prototype_device == device {
        return Ok(false);
    }
    tx.cmd(doc, Command::SetPrototypeDevice { device })?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reactions_are_transactional_and_validate_destinations() {
        let mut doc = Document::new("prototype");
        let page = doc.add_page("Page");
        let a = doc.alloc_id();
        let b = doc.alloc_id();
        doc.insert(page, Node::frame(a, Rect::new(0., 0., 100., 100.))).unwrap();
        doc.insert(page, Node::frame(b, Rect::new(120., 0., 100., 100.))).unwrap();
        let r = Reaction { trigger: Trigger::OnClick, actions: vec![Action::Navigate { dest: b, preserve_scroll: false }], transition: None };
        let mut tx = Tx::new();
        assert!(set_reactions(&mut doc, &mut tx, a, vec![r]).unwrap());
        let mut h = History::default();
        tx.commit(&mut doc, &mut h).unwrap();
        assert_eq!(doc.get(a).unwrap().prototype.reactions.len(), 1);
        h.undo(&mut doc).unwrap();
        assert!(doc.get(a).unwrap().prototype.reactions.is_empty());
        let mut tx = Tx::new();
        let bad = Reaction { trigger: Trigger::OnClick, actions: vec![Action::Navigate { dest: NodeId(999), preserve_scroll: false }], transition: None };
        assert_eq!(set_reactions(&mut doc, &mut tx, a, vec![bad]), Err(PrototypeEditError::Invalid("reactions")));
        assert!(tx.ed.touched().is_empty());
    }

    #[test]
    fn flow_edits_validate_and_undo() {
        let mut doc = Document::new("prototype");
        let page = doc.add_page("Page");
        let start = doc.alloc_id();
        doc.insert(page, Node::frame(start, Rect::new(0., 0., 40., 40.))).unwrap();
        let mut tx = Tx::new();
        edit_flows(&mut doc, &mut tx, |v| v.push(Flow { name: "Main".into(), page, start, description: String::new() })).unwrap();
        let mut h = History::default();
        tx.commit(&mut doc, &mut h).unwrap();
        assert_eq!(doc.flows.len(), 1);
        h.undo(&mut doc).unwrap();
        assert!(doc.flows.is_empty());
    }

    #[test]
    fn device_rejects_invalid_size_without_edit() {
        let mut doc = Document::new("prototype");
        let mut tx = Tx::new();
        assert_eq!(
            set_device(&mut doc, &mut tx, PrototypeDevice { size: Some(Vec2::new(0., 400.)), ..Default::default() }),
            Err(PrototypeEditError::Invalid("prototype device"))
        );
        assert!(tx.ed.touched().is_empty());
    }
}
