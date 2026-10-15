use crate::{apply_resize, edit::Tx, layout};
use desigma_doc::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_OPERATIONS: usize = 256;
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyRequest {
    pub expected_revision: u64,
    #[serde(default)]
    pub dry_run: bool,
    pub ops: Vec<Operation>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    CreateNode {
        temp_id: String,
        parent: NodeRef,
        #[serde(default)]
        index: Option<usize>,
        kind: String,
        #[serde(default)]
        properties: NodeProperties,
        #[serde(default, deserialize_with = "strict_native")]
        board: Option<BoardNodeProperties>,
    },
    DuplicateNode {
        temp_id: String,
        id: NodeRef,
        #[serde(default)]
        parent: Option<NodeRef>,
        #[serde(default)]
        index: Option<usize>,
        #[serde(default, deserialize_with = "strict_native")]
        offset: Option<Vec2>,
    },
    DeleteNode {
        id: NodeRef,
    },
    MoveNode {
        id: NodeRef,
        parent: NodeRef,
        #[serde(default)]
        index: Option<usize>,
    },
    ReorderChildren {
        parent: NodeRef,
        order: Vec<NodeRef>,
    },
    CreatePage {
        temp_id: String,
        name: String,
        #[serde(default)]
        index: Option<usize>,
    },
    DeletePage {
        id: NodeRef,
    },
    ReorderPages {
        order: Vec<NodeRef>,
    },
    GroupNodes {
        temp_id: String,
        ids: Vec<NodeRef>,
        #[serde(default)]
        name: Option<String>,
    },
    Ungroup {
        id: NodeRef,
    },
    WrapInFrame {
        temp_id: String,
        ids: Vec<NodeRef>,
        #[serde(default)]
        name: Option<String>,
    },
    WrapInAutoLayout {
        temp_id: String,
        ids: Vec<NodeRef>,
        auto_layout: AutoLayout,
        #[serde(default)]
        name: Option<String>,
    },
    ReplaceVariables {
        #[serde(deserialize_with = "strict_native")]
        variables: BTreeMap<String, Variable>,
        #[serde(deserialize_with = "strict_native")]
        collections: BTreeMap<String, VariableCollection>,
    },
    SetStyles {
        #[serde(deserialize_with = "strict_native")]
        styles: BTreeMap<String, Style>,
        #[serde(default, deserialize_with = "strict_native")]
        style_meta: BTreeMap<String, StyleMeta>,
    },
    CreateStyle {
        key: String,
        #[serde(deserialize_with = "strict_native")]
        style: Style,
        #[serde(deserialize_with = "strict_native")]
        meta: StyleMeta,
    },
    UpdateStyle {
        key: String,
        #[serde(deserialize_with = "strict_native")]
        style: Style,
        #[serde(default, deserialize_with = "strict_native")]
        meta: Option<StyleMeta>,
    },
    DeleteStyle {
        key: String,
    },
    ApplyStyle {
        ids: Vec<NodeRef>,
        key: String,
        #[serde(default)]
        property: Option<StyleProperty>,
    },
    CreateComponent {
        id: NodeRef,
    },
    DetachInstance {
        id: NodeRef,
    },
    ResetInstance {
        id: NodeRef,
    },
    ResetOverrides {
        id: NodeRef,
        fields: Vec<OverrideField>,
    },
    SetComponentProperties {
        id: NodeRef,
        #[serde(deserialize_with = "strict_native")]
        props: Vec<ComponentPropDef>,
    },
    SetInstanceProperty {
        id: NodeRef,
        name: String,
        #[serde(default, deserialize_with = "strict_native")]
        value: Option<PropValue>,
    },
    SetReactions {
        id: NodeRef,
        #[serde(deserialize_with = "strict_native")]
        reactions: Vec<Reaction>,
    },
    SetFlows {
        #[serde(deserialize_with = "strict_native")]
        flows: Vec<Flow>,
    },
    SetPrototypeDevice {
        #[serde(deserialize_with = "strict_native")]
        device: PrototypeDevice,
    },
    SetVariableExpression {
        variable_id: String,
        mode_id: String,
        expression: String,
    },
    SetComments {
        #[serde(deserialize_with = "strict_native")]
        comments: Vec<CommentThread>,
    },
    AddComment {
        thread_id: String,
        message_id: String,
        #[serde(deserialize_with = "strict_native")]
        anchor: CommentAnchor,
        author: String,
        text: String,
        created: String,
    },
    ReplyComment {
        thread_id: String,
        message_id: String,
        author: String,
        text: String,
        created: String,
    },
    ResolveComment {
        thread_id: String,
        resolved: bool,
    },
    SetDevStatus {
        id: NodeRef,
        #[serde(default)]
        status: Option<DevStatus>,
        #[serde(default)]
        updated_at: Option<String>,
        #[serde(default)]
        snapshot_version: Option<u32>,
    },
    SetDevAnnotations {
        id: NodeRef,
        #[serde(deserialize_with = "strict_native")]
        annotations: Vec<Annotation>,
    },
    CreateConnector {
        parent: NodeRef,
        #[serde(deserialize_with = "strict_native")]
        connector: ConnectorProps,
        temp_id: String,
    },
    SetConnector {
        id: NodeRef,
        #[serde(deserialize_with = "strict_native")]
        connector: ConnectorProps,
    },
    AddSlide {
        page: NodeRef,
        #[serde(default)]
        after: Option<NodeRef>,
        #[serde(default)]
        name: Option<String>,
        temp_id: String,
    },
    DeleteSlide {
        id: NodeRef,
    },
    ReorderSlide {
        id: NodeRef,
        index: usize,
    },
    SetSlideNotes {
        id: NodeRef,
        notes: String,
    },
    SetExportPresets {
        id: NodeRef,
        #[serde(deserialize_with = "strict_native")]
        presets: Vec<ExportPreset>,
    },
    ImportLibraryAsset {
        #[serde(deserialize_with = "strict_native")]
        snapshot: LibrarySnapshot,
        key: String,
        #[serde(default, deserialize_with = "strict_native")]
        placement: Option<Placement>,
    },
    ApplyLibraryUpdates {
        #[serde(deserialize_with = "strict_native")]
        snapshot: LibrarySnapshot,
        keys: Vec<String>,
    },
    EnableLibrary {
        #[serde(deserialize_with = "strict_native")]
        snapshot: LibrarySnapshot,
    },
    DisableLibrary {
        library_id: String,
    },
    SetTextPatch {
        id: NodeId,
        #[serde(deserialize_with = "strict_native")]
        patch: crate::design_ops::TextPatch,
    },
    SetVisualPatch {
        id: NodeId,
        #[serde(deserialize_with = "strict_native")]
        patch: crate::design_ops::VisualPatch,
    },
    SetProperties {
        id: NodeId,
        properties: NodeProperties,
    },
    SetFills {
        id: NodeId,
        #[serde(deserialize_with = "strict_native")]
        fills: Vec<Paint>,
        #[serde(default)]
        bindings: BTreeMap<String, String>,
    },
    SetAutoLayout {
        id: NodeId,
        #[serde(deserialize_with = "strict_native")]
        auto_layout: Option<AutoLayout>,
    },
    CreateInstance {
        component: NodeId,
        parent: NodeId,
        #[serde(deserialize_with = "strict_native")]
        at: Vec2,
    },
    SetVariant {
        id: NodeId,
        component: NodeId,
    },
    CombineAsVariants {
        temp_id: String,
        ids: Vec<NodeRef>,
    },
    EditVariable {
        id: String,
        mode: String,
        #[serde(deserialize_with = "strict_native")]
        value: VariableValue,
    },
    CreateVariableCollection {
        temp_id: String,
        name: String,
    },
    CreateVariableMode {
        temp_id: String,
        collection_id: String,
        name: String,
    },
    CreateVariable {
        temp_id: String,
        collection_id: String,
        name: String,
        kind: VariableKind,
    },
    RenameVariable {
        id: String,
        name: String,
    },
    DeleteVariable {
        id: String,
    },
    BindVariable {
        id: NodeRef,
        property: String,
        variable_id: String,
    },
    UnbindVariable {
        id: NodeRef,
        property: String,
    },
    SetNodeMode {
        id: NodeRef,
        collection_id: String,
        mode_id: Option<String>,
    },
    SetVectorNetwork {
        id: NodeRef,
        #[serde(deserialize_with = "strict_native")]
        network: VectorNetwork,
    },
    BooleanOp {
        temp_id: String,
        ids: Vec<NodeRef>,
        boolean_op: BooleanOp,
    },
    Flatten {
        temp_id: String,
        ids: Vec<NodeRef>,
    },
    OutlineStroke {
        temp_id: String,
        id: NodeRef,
    },
}

/// Numeric document IDs or a `$newN` handle returned by an earlier create_node in this batch.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum NodeRef {
    Id(NodeId),
    Temp(String),
}
fn resolve_ref(r: &NodeRef, temps: &BTreeMap<String, NodeId>) -> Result<NodeId, EngineError> {
    match r {
        NodeRef::Id(id) => Ok(*id),
        NodeRef::Temp(s) => temps.get(s).copied().ok_or_else(|| invalid(format!("unknown or forward temporary ID {s}"))),
    }
}
fn resolve_variable_ref(value: &str, temps: &BTreeMap<String, String>) -> Result<String, EngineError> {
    if value.starts_with("$new") {
        temps.get(value).cloned().ok_or_else(|| invalid("temporary variable ID must refer to an earlier variable create operation"))
    } else {
        if value.trim().is_empty() || value.len() > 1024 {
            return Err(invalid("variable ID must contain 1..1024 bytes"));
        }
        Ok(value.to_string())
    }
}

// Native document types remain permissive for file-format compatibility. Agent inputs
// reject extra fields, including nested paints, colors and layout properties.
fn strict_native<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Serialize,
{
    let raw = serde_json::Value::deserialize(deserializer)?;
    let parsed: T = serde_json::from_value(raw.clone()).map_err(serde::de::Error::custom)?;
    let canonical = serde_json::to_value(&parsed).map_err(serde::de::Error::custom)?;
    fn check(raw: &serde_json::Value, canonical: &serde_json::Value) -> bool {
        match (raw, canonical) {
            (serde_json::Value::Object(a), serde_json::Value::Object(b)) => a.iter().all(|(key, value)| b.get(key).is_some_and(|other| check(value, other))),
            (serde_json::Value::Array(a), serde_json::Value::Array(b)) => a.iter().zip(b).all(|(a, b)| check(a, b)),
            _ => true,
        }
    }
    if !check(&raw, &canonical) {
        return Err(serde::de::Error::custom("unknown field in design operation"));
    }
    Ok(parsed)
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct NodeProperties {
    pub name: Option<String>,
    pub visible: Option<bool>,
    pub locked: Option<bool>,
    pub opacity: Option<f32>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub corner_radii: Option<[f32; 4]>,
    pub characters: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BoardNodeProperties {
    pub color: Option<Color>,
    pub author: Option<String>,
    pub show_author: Option<bool>,
    pub shape: Option<ShapeKind>,
    pub stamp_kind: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChangeSummary {
    pub revision: u64,
    pub dry_run: bool,
    pub changes: Vec<NodeChange>,
    pub created_node_ids: Vec<NodeId>,
    #[serde(default)]
    pub temp_ids: BTreeMap<String, NodeId>,
    #[serde(default)]
    pub duplicate_mappings: BTreeMap<String, BTreeMap<NodeId, NodeId>>,
    #[serde(default)]
    pub temp_variable_ids: BTreeMap<String, String>,
    pub changed_variable_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NodeChange {
    pub id: NodeId,
    pub properties: Vec<String>,
    pub derived: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    StaleRevision,
    MissingNode,
    InvalidOperation,
    PermissionDenied,
    OperationConflict,
    UnsupportedOperation,
}

#[derive(Clone, Debug, Deserialize, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct EngineError {
    pub code: ErrorCode,
    pub message: String,
}
impl EngineError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}
impl From<DocError> for EngineError {
    fn from(e: DocError) -> Self {
        let code = if matches!(e, DocError::NotFound(_)) { ErrorCode::MissingNode } else { ErrorCode::OperationConflict };
        Self::new(code, e.to_string())
    }
}
fn invalid(s: impl Into<String>) -> EngineError {
    EngineError::new(ErrorCode::InvalidOperation, s)
}
fn editable(doc: &Document, id: NodeId) -> Result<(), EngineError> {
    let mut cur = Some(id);
    let mut seen = BTreeSet::new();
    while let Some(nid) = cur {
        if !seen.insert(nid) {
            return Err(EngineError::new(ErrorCode::OperationConflict, "cyclic node ancestry"));
        }
        let n = doc.get(nid).ok_or_else(|| EngineError::new(ErrorCode::MissingNode, format!("node {} not found", nid.0)))?;
        if n.locked {
            return Err(EngineError::new(ErrorCode::PermissionDenied, format!("node {} is locked", nid.0)));
        }
        cur = n.parent;
    }
    Ok(())
}
fn finite(v: f32) -> Result<(), EngineError> {
    if v.is_finite() && v.abs() <= 1_000_000. {
        Ok(())
    } else {
        Err(invalid("numbers must be finite and within ±1000000"))
    }
}
fn positive(v: f32) -> Result<(), EngineError> {
    finite(v)?;
    if v < 0. {
        Err(invalid("sizes must be nonnegative"))
    } else {
        Ok(())
    }
}
fn valid_temp_id(s: &str) -> bool {
    s.strip_prefix("$new").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}
fn new_node(id: NodeId, kind: &str, p: &NodeProperties, board: Option<&BoardNodeProperties>) -> Result<Node, EngineError> {
    let rect = Rect::new(p.x.unwrap_or(0.), p.y.unwrap_or(0.), p.width.unwrap_or(100.), p.height.unwrap_or(100.));
    for v in [rect.x, rect.y] {
        finite(v)?;
    }
    for v in [rect.w, rect.h] {
        positive(v)?;
    }
    let mut n = match kind {
        "frame" => Node::frame(id, rect),
        "group" => Node::new(id, NodeKind::Group),
        "section" => Node::new(id, NodeKind::Section),
        "rectangle" => Node::rectangle(id, rect),
        "ellipse" => Node::ellipse(id, rect),
        "line" => Node::new(id, NodeKind::Line),
        "polygon" => Node::new(id, NodeKind::Polygon { count: 6 }),
        "star" => Node::new(id, NodeKind::Star { count: 5, inner_ratio: 0.5 }),
        "vector" => Node::new(id, NodeKind::Vector { paths: Vec::new(), network: None }),
        "text" => Node::text(id, rect, "", TextStyle::default()),
        "sticky" => {
            let b = board.cloned().unwrap_or_default();
            if b.shape.is_some() || b.stamp_kind.is_some() {
                return Err(invalid("sticky supports only color, author and show_author"));
            }
            if let Some(color) = b.color {
                color_check(color)?;
            }
            if b.author.as_ref().is_some_and(|s| s.len() > 256 || s.chars().any(char::is_control)) {
                return Err(invalid("sticky author must be at most 256 bytes without control characters"));
            }
            let mut node = Node::sticky(id, rect, p.characters.clone().unwrap_or_default());
            if let NodeKind::Sticky(props) = &mut node.kind {
                if let Some(color) = b.color {
                    props.color = color;
                }
                props.author = b.author;
                if let Some(show_author) = b.show_author {
                    props.show_author = show_author;
                }
            }
            node
        }
        "shape_with_text" => {
            let b = board.cloned().unwrap_or_default();
            if b.author.is_some() || b.show_author.is_some() || b.stamp_kind.is_some() {
                return Err(invalid("shape_with_text supports only color and shape"));
            }
            if let Some(color) = b.color {
                color_check(color)?;
            }
            let mut node = Node::shape_with_text(id, rect, b.shape.unwrap_or(ShapeKind::Square), p.characters.clone().unwrap_or_default());
            if let Some(color) = b.color {
                node.fills = vec![Paint::solid(color)];
            }
            node
        }
        "stamp" => {
            let b = board.cloned().unwrap_or_default();
            if b.color.is_some() || b.author.is_some() || b.show_author.is_some() || b.shape.is_some() || p.characters.is_some() {
                return Err(invalid("stamp supports only stamp_kind"));
            }
            if (rect.w - rect.h).abs() > 0.001 {
                return Err(invalid("stamp width and height must match"));
            }
            let stamp = b.stamp_kind.as_deref().unwrap_or("thumbs_up");
            if !STAMP_KINDS.contains(&stamp) {
                return Err(invalid("stamp_kind must be a built-in stamp kind"));
            }
            Node::stamp(id, Vec2::new(rect.x, rect.y), stamp, rect.w)
        }
        "boolean_op" => Node::new(id, NodeKind::BooleanOp(BooleanOp::Union)),
        _ => return Err(invalid("kind must be a supported shape, text, board item, or container kind")),
    };
    if !matches!(kind, "sticky" | "shape_with_text" | "stamp") && board.is_some() {
        return Err(invalid("board properties are only valid for sticky, shape_with_text or stamp nodes"));
    }
    n.transform = Affine::translate(rect.x, rect.y);
    n.size = Vec2::new(rect.w, rect.h);
    if let Some(name) = &p.name {
        n.name = name.clone();
    }
    if let Some(visible) = p.visible {
        n.visible = visible;
    }
    if let Some(locked) = p.locked {
        n.locked = locked;
    }
    if let Some(opacity) = p.opacity {
        unit(opacity)?;
        n.opacity = opacity;
    }
    if let Some(radii) = p.corner_radii {
        for r in radii {
            positive(r)?;
        }
        n.corner_radii = radii;
    }
    if let Some(text) = &p.characters {
        let Some(t) = n.kind.any_text_mut() else {
            return Err(invalid("characters requires a text-carrying node"));
        };
        t.content = text.clone();
    }
    Ok(n)
}
fn color_check(c: Color) -> Result<(), EngineError> {
    for v in [c.r, c.g, c.b, c.a] {
        unit(v)?;
    }
    Ok(())
}
enum WrapKind {
    Group,
    Frame(Option<AutoLayout>),
}
fn wrap_nodes(
    doc: &mut Document,
    tx: &mut Tx,
    refs: &[NodeRef],
    temp_id: &str,
    name: Option<&str>,
    kind: WrapKind,
    temps: &mut BTreeMap<String, NodeId>,
) -> Result<NodeId, EngineError> {
    if !valid_temp_id(temp_id) || temps.contains_key(temp_id) {
        return Err(invalid("temp_id must be a unique $newN handle"));
    }
    let min_count = if matches!(&kind, WrapKind::Group) { 2 } else { 1 };
    if refs.len() < min_count {
        return Err(invalid("wrapping requires at least one node; grouping requires at least two"));
    }
    if name.is_some_and(|s| s.trim().is_empty() || s.len() > 1024) {
        return Err(invalid("name must contain 1..1024 bytes"));
    }
    let mut ids = refs.iter().map(|r| resolve_ref(r, temps)).collect::<Result<Vec<_>, _>>()?;
    if ids.iter().copied().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(invalid("node selection contains duplicates"));
    }
    let first = *ids.first().unwrap();
    let parent = doc.get(first).and_then(|n| n.parent).ok_or_else(|| invalid("page roots cannot be wrapped"))?;
    editable(doc, parent)?;
    for id in &ids {
        editable(doc, *id)?;
        if doc.get(*id).and_then(|n| n.parent) != Some(parent) {
            return Err(invalid("all wrapped nodes must share one parent"));
        }
    }
    let siblings = doc.get(parent).unwrap().children.clone();
    ids.sort_by_key(|id| siblings.iter().position(|child| child == id).unwrap_or(usize::MAX));
    let index = siblings.iter().position(|child| *child == ids[0]).ok_or_else(|| invalid("selected node is detached"))?;
    let wrapper_id = doc.try_alloc_id().ok_or_else(|| invalid("document has exhausted node IDs"))?;
    let mut wrapper = match kind {
        WrapKind::Group => Node::new(wrapper_id, NodeKind::Group),
        WrapKind::Frame(auto) => {
            let mut n = Node::frame(wrapper_id, Rect::new(0., 0., doc.get(parent).unwrap().size.x, doc.get(parent).unwrap().size.y));
            n.fills.clear();
            if let Some(frame) = n.kind.frame_props_mut() {
                frame.auto_layout = auto;
                frame.clips_content = false;
            }
            n
        }
    };
    wrapper.transform = Affine::IDENTITY;
    wrapper.size = doc.get(parent).unwrap().size;
    if let Some(name) = name {
        wrapper.name = name.into();
    }
    tx.cmd(doc, Command::Insert { parent, index, nodes: vec![wrapper] })?;
    for (offset, id) in ids.iter().enumerate() {
        tx.cmd(doc, Command::Reparent { id: *id, parent: wrapper_id, index: offset })?;
    }
    temps.insert(temp_id.into(), wrapper_id);
    layout::relayout_around(doc, &mut tx.ed, parent);
    Ok(wrapper_id)
}
fn unit(v: f32) -> Result<(), EngineError> {
    finite(v)?;
    if (0.0..=1.0).contains(&v) {
        Ok(())
    } else {
        Err(invalid("color, opacity and gradient stops must be between zero and one"))
    }
}
fn color(c: Color) -> Result<(), EngineError> {
    for v in [c.r, c.g, c.b, c.a] {
        unit(v)?;
    }
    Ok(())
}
fn paint(p: &Paint, doc: &Document) -> Result<(), EngineError> {
    unit(p.opacity)?;
    match &p.kind {
        PaintKind::Solid(c) => color(*c)?,
        PaintKind::Image { image_ref, .. } => {
            if !doc.images.contains_key(image_ref) {
                return Err(invalid("fill image does not exist"));
            }
        }
        PaintKind::LinearGradient { handles, stops }
        | PaintKind::RadialGradient { handles, stops }
        | PaintKind::AngularGradient { handles, stops }
        | PaintKind::DiamondGradient { handles, stops } => {
            if stops.is_empty() || stops.len() > 256 {
                return Err(invalid("gradient must have between 1 and 256 stops"));
            }
            for h in handles {
                finite(h.x)?;
                finite(h.y)?;
            }
            for s in stops {
                unit(s.pos)?;
                color(s.color)?;
            }
        }
    }
    Ok(())
}
fn operation(
    doc: &mut Document,
    tx: &mut Tx,
    op: &Operation,
    explicit: &mut BTreeSet<NodeId>,
    temps: &mut BTreeMap<String, NodeId>,
    variable_temps: &mut BTreeMap<String, String>,
    duplicate_mappings: &mut BTreeMap<String, BTreeMap<NodeId, NodeId>>,
) -> Result<(), EngineError> {
    match op {
        Operation::CreateNode { temp_id, parent, index, kind, properties, board } => {
            if !valid_temp_id(temp_id) || temps.contains_key(temp_id) {
                return Err(invalid("temp_id must be a unique $newN handle"));
            }
            let parent = resolve_ref(parent, temps)?;
            editable(doc, parent)?;
            if !doc.get(parent).is_some_and(|n| n.kind.is_container()) {
                return Err(invalid("parent must be a container"));
            }
            if matches!(kind.as_str(), "sticky" | "shape_with_text" | "stamp") && doc.kind != DocumentKind::Board {
                return Err(invalid("board node kinds require a board document"));
            }
            let id = doc.try_alloc_id().ok_or_else(|| invalid("document has exhausted node IDs"))?;
            let mut node = new_node(id, kind, properties, board.as_ref())?;
            if properties.x.is_some() || properties.y.is_some() {
                node.transform.tx = properties.x.unwrap_or(0.);
                node.transform.ty = properties.y.unwrap_or(0.);
            }
            if properties.width.is_some() || properties.height.is_some() {
                node.size = Vec2::new(properties.width.unwrap_or(100.), properties.height.unwrap_or(100.));
            }
            let at = index.unwrap_or_else(|| doc.get(parent).map_or(0, |p| p.children.len()));
            tx.cmd(doc, Command::Insert { parent, index: at, nodes: vec![node] })?;
            temps.insert(temp_id.clone(), id);
            explicit.insert(id);
            layout::relayout_around(doc, &mut tx.ed, parent);
        }
        Operation::DeleteNode { id } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            if doc.get(id).is_none_or(|n| n.parent.is_none()) {
                return Err(invalid("page roots cannot be deleted"));
            }
            let parent = doc.get(id).and_then(|n| n.parent).unwrap();
            tx.cmd(doc, Command::Remove { id })?;
            layout::relayout_around(doc, &mut tx.ed, parent);
        }
        Operation::MoveNode { id, parent, index } => {
            let id = resolve_ref(id, temps)?;
            let parent = resolve_ref(parent, temps)?;
            editable(doc, id)?;
            editable(doc, parent)?;
            if doc.get(id).is_none_or(|n| n.parent.is_none()) {
                return Err(invalid("page roots cannot be moved"));
            }
            if !doc.get(parent).is_some_and(|n| n.kind.is_container()) {
                return Err(invalid("parent must be a container"));
            }
            if id == parent || doc.is_ancestor(id, parent) {
                return Err(EngineError::new(ErrorCode::OperationConflict, "cannot move a node into its descendant"));
            }
            let old_parent = doc.get(id).and_then(|n| n.parent).unwrap();
            let world = doc.world_transform(id);
            let parent_world_inverse = doc
                .world_transform(parent)
                .invert()
                .ok_or_else(|| EngineError::new(ErrorCode::OperationConflict, "new parent has a noninvertible transform"))?;
            let ix = index.unwrap_or_else(|| doc.get(parent).map_or(0, |n| n.children.len()));
            tx.cmd(doc, Command::Reparent { id, parent, index: ix })?;
            tx.ed.node(doc, id).unwrap().transform = parent_world_inverse * world;
            explicit.insert(id);
            layout::relayout_around(doc, &mut tx.ed, old_parent);
            layout::relayout_around(doc, &mut tx.ed, parent);
        }
        Operation::ReorderChildren { parent, order } => {
            let parent = resolve_ref(parent, temps)?;
            editable(doc, parent)?;
            let children = doc.get(parent).ok_or_else(|| EngineError::new(ErrorCode::MissingNode, "parent not found"))?.children.clone();
            let desired: Vec<_> = order.iter().map(|r| resolve_ref(r, temps)).collect::<Result<_, _>>()?;
            if desired.len() != children.len() || desired.iter().copied().collect::<BTreeSet<_>>() != children.iter().copied().collect() {
                return Err(invalid("order must contain every direct child exactly once"));
            }
            // Reparenting to each target index preserves the model's normal undo path.
            for (i, id) in desired.iter().enumerate() {
                tx.cmd(doc, Command::Reparent { id: *id, parent, index: i })?;
            }
            explicit.insert(parent);
            layout::relayout_around(doc, &mut tx.ed, parent);
        }
        Operation::CreatePage { temp_id, name, index } => {
            if !valid_temp_id(temp_id) || temps.contains_key(temp_id) {
                return Err(invalid("temp_id must be a unique $newN handle"));
            }
            if name.trim().is_empty() || name.len() > 1024 {
                return Err(invalid("page name must contain 1..1024 bytes"));
            }
            let id = doc.try_alloc_id().ok_or_else(|| invalid("document has exhausted node IDs"))?;
            let mut page = Node::new(id, NodeKind::Page { background: Color::rgb(0xF5, 0xF5, 0xF5) });
            page.name = name.clone();
            tx.cmd(doc, Command::InsertPage { index: index.unwrap_or(doc.pages.len()), nodes: vec![page] })?;
            temps.insert(temp_id.clone(), id);
            explicit.insert(id);
        }
        Operation::DuplicateNode { temp_id, id, parent, index, offset } => {
            if !valid_temp_id(temp_id) || temps.contains_key(temp_id) {
                return Err(invalid("temp_id must be a unique $newN handle"));
            }
            let source = resolve_ref(id, temps)?;
            editable(doc, source)?;
            let parent = match parent {
                Some(parent) => resolve_ref(parent, temps)?,
                None => doc.get(source).and_then(|n| n.parent).ok_or_else(|| invalid("duplicate source must have a parent"))?,
            };
            editable(doc, parent)?;
            let (new_root, mapping) =
                crate::duplicate_ops::duplicate_node(doc, tx, source, parent, *index, offset.unwrap_or(Vec2::ZERO)).map_err(|e| invalid(e.to_string()))?;
            explicit.extend(mapping.values().copied());
            duplicate_mappings.insert(temp_id.clone(), mapping);
            temps.insert(temp_id.clone(), new_root);
            layout::relayout_around(doc, &mut tx.ed, parent);
        }
        Operation::DeletePage { id } => {
            let id = resolve_ref(id, temps)?;
            if doc.pages.len() <= 1 {
                return Err(invalid("a document must retain at least one page"));
            }
            if !doc.pages.contains(&id) {
                return Err(EngineError::new(ErrorCode::MissingNode, "page not found"));
            }
            let flows = doc.flows.iter().filter(|f| f.page != id).cloned().collect::<Vec<_>>();
            if flows != doc.flows {
                tx.cmd(doc, Command::SetFlows { flows })?;
            }
            let comments = doc.comments.iter().filter(|c| c.anchor.page != id).cloned().collect::<Vec<_>>();
            if comments != doc.comments {
                tx.cmd(doc, Command::SetComments { comments })?;
            }
            if doc.guides.contains_key(&id) {
                tx.cmd(doc, Command::SetGuides { page: id, guides: Vec::new() })?;
            }
            tx.cmd(doc, Command::RemovePage { id })?;
        }
        Operation::ReorderPages { order } => {
            let current = doc.pages.clone();
            let desired: Vec<_> = order.iter().map(|r| resolve_ref(r, temps)).collect::<Result<_, _>>()?;
            if desired.len() != current.len() || desired.iter().copied().collect::<BTreeSet<_>>() != current.iter().copied().collect() {
                return Err(invalid("order must contain every page exactly once"));
            }
            for (index, id) in desired.iter().enumerate() {
                if doc.pages.get(index) == Some(id) {
                    continue;
                }
                let mut nodes = Vec::new();
                doc.walk(*id, |n, _| nodes.push(n.clone()));
                tx.cmd(doc, Command::Batch(vec![Command::RemovePage { id: *id }, Command::InsertPage { index, nodes }]))?;
            }
        }
        Operation::GroupNodes { temp_id, ids, name } => {
            let id = wrap_nodes(doc, tx, ids, temp_id, name.as_deref(), WrapKind::Group, temps)?;
            explicit.insert(id);
        }
        Operation::WrapInFrame { temp_id, ids, name } => {
            let id = wrap_nodes(doc, tx, ids, temp_id, name.as_deref(), WrapKind::Frame(None), temps)?;
            explicit.insert(id);
        }
        Operation::WrapInAutoLayout { temp_id, ids, auto_layout, name } => {
            for value in auto_layout.padding {
                positive(value)?;
            }
            finite(auto_layout.item_spacing)?;
            finite(auto_layout.counter_spacing)?;
            let id = wrap_nodes(doc, tx, ids, temp_id, name.as_deref(), WrapKind::Frame(Some(*auto_layout)), temps)?;
            explicit.insert(id);
        }
        Operation::Ungroup { id } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            let group = doc.get(id).ok_or_else(|| EngineError::new(ErrorCode::MissingNode, "group not found"))?;
            if !matches!(group.kind, NodeKind::Group) || group.parent.is_none() {
                return Err(invalid("ungroup requires a non-root group"));
            }
            if group.transform != Affine::IDENTITY {
                return Err(invalid("ungroup requires an untransformed group to preserve child positions"));
            }
            let parent = group.parent.unwrap();
            editable(doc, parent)?;
            let index = doc.get(parent).unwrap().children.iter().position(|child| *child == id).unwrap();
            let children = group.children.clone();
            for (offset, child) in children.iter().enumerate() {
                tx.cmd(doc, Command::Reparent { id: *child, parent, index: index + offset })?;
            }
            tx.cmd(doc, Command::Remove { id })?;
            layout::relayout_around(doc, &mut tx.ed, parent);
        }
        Operation::ReplaceVariables { variables, collections } => {
            let store = VariableStore { variables: variables.clone(), collections: collections.clone() };
            crate::agent_extras::apply_variable_edit(doc, tx, |current| *current = store).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::SetStyles { styles, style_meta } => {
            crate::agent_extras::apply_style_edit(doc, tx, |current, meta| {
                *current = styles.clone();
                *meta = style_meta.clone();
            })
            .map_err(|e| invalid(e.to_string()))?;
        }
        Operation::CreateStyle { key, style, meta } => {
            crate::style_ops::create_style(doc, tx, key.clone(), style.clone(), meta.clone()).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::UpdateStyle { key, style, meta } => {
            crate::style_ops::update_style(doc, tx, key, style.clone(), meta.clone()).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::DeleteStyle { key } => {
            crate::style_ops::delete_style(doc, tx, key).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::ApplyStyle { ids, key, property } => {
            let ids: Vec<_> = ids.iter().map(|id| resolve_ref(id, temps)).collect::<Result<_, _>>()?;
            for id in &ids {
                editable(doc, *id)?;
            }
            crate::style_ops::apply_style(doc, tx, &ids, key, *property).map_err(|e| invalid(e.to_string()))?;
            explicit.extend(ids);
        }
        Operation::CreateComponent { id } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::agent_extras::create_component(doc, tx, id).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::DetachInstance { id } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::agent_extras::detach_instance(doc, tx, id).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::ResetInstance { id } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::agent_extras::reset_instance(doc, tx, id).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::ResetOverrides { id, fields } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::agent_extras::reset_overrides(doc, tx, id, fields).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::SetComponentProperties { id, props } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::agent_extras::set_component_properties(doc, tx, id, props.clone()).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::SetInstanceProperty { id, name, value } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::agent_extras::set_instance_property(doc, tx, id, name, value.clone()).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::SetReactions { id, reactions } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::prototyping::set_reactions(doc, tx, id, reactions.clone()).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::SetFlows { flows } => {
            crate::prototyping::edit_flows(doc, tx, |current| *current = flows.clone()).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::SetPrototypeDevice { device } => {
            crate::prototyping::set_device(doc, tx, device.clone()).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::SetVariableExpression { variable_id, mode_id, expression } => {
            let expression = crate::prototype_variables::parse_variable_expression(doc, expression).map_err(|e| invalid(e.to_string()))?;
            crate::prototype_variables::set_variable_expression(doc, tx, variable_id, mode_id, &expression).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::SetComments { comments } => {
            crate::board_ops::edit_comments(doc, tx, |current| *current = comments.clone()).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::AddComment { thread_id, message_id, anchor, author, text, created } => {
            crate::comments_ops::add_comment(doc, tx, thread_id.clone(), message_id.clone(), *anchor, author.clone(), text.clone(), created.clone())
                .map_err(|e| invalid(e.to_string()))?;
        }
        Operation::ReplyComment { thread_id, message_id, author, text, created } => {
            crate::comments_ops::reply_comment(doc, tx, thread_id, message_id.clone(), author.clone(), text.clone(), created.clone())
                .map_err(|e| invalid(e.to_string()))?;
        }
        Operation::ResolveComment { thread_id, resolved } => {
            crate::comments_ops::resolve_comment(doc, tx, thread_id, *resolved).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::SetDevStatus { id, status, updated_at, snapshot_version } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::handoff_ops::set_dev_status(doc, tx, id, *status, updated_at.as_deref(), *snapshot_version).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::SetDevAnnotations { id, annotations } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::handoff_ops::set_dev_annotations(doc, tx, id, annotations.clone()).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::CreateConnector { parent, connector, temp_id } => {
            if !valid_temp_id(temp_id) || temps.contains_key(temp_id) {
                return Err(invalid("temp_id must be a unique $newN handle"));
            }
            let parent = resolve_ref(parent, temps)?;
            editable(doc, parent)?;
            let id = crate::board_ops::create_connector(doc, tx, parent, connector.clone()).map_err(|e| invalid(e.to_string()))?;
            temps.insert(temp_id.clone(), id);
            explicit.insert(id);
        }
        Operation::SetConnector { id, connector } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::board_ops::set_connector(doc, tx, id, connector.clone()).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::AddSlide { page, after, name, temp_id } => {
            if !valid_temp_id(temp_id) || temps.contains_key(temp_id) {
                return Err(invalid("temp_id must be a unique $newN handle"));
            }
            let page = resolve_ref(page, temps)?;
            let after = after.as_ref().map(|r| resolve_ref(r, temps)).transpose()?;
            let id = crate::decks_ops::add_slide(doc, tx, page, after, name.clone()).map_err(|e| invalid(e.to_string()))?;
            temps.insert(temp_id.clone(), id);
            explicit.insert(id);
        }
        Operation::DeleteSlide { id } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::decks_ops::delete_slide(doc, tx, id).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::ReorderSlide { id, index } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::decks_ops::reorder_slide(doc, tx, id, *index).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::SetSlideNotes { id, notes } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::decks_ops::set_slide_notes(doc, tx, id, notes.clone()).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::SetExportPresets { id, presets } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::decks_ops::set_export_presets(doc, tx, id, presets.clone()).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::ImportLibraryAsset { snapshot, key, placement } => {
            if let Some(p) = placement {
                editable(doc, p.parent)?;
            }
            crate::library_ops::import_library_asset(doc, tx, snapshot, key, *placement).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::ApplyLibraryUpdates { snapshot, keys } => {
            crate::library_ops::apply_library_updates(doc, tx, snapshot, keys).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::EnableLibrary { snapshot } => {
            crate::library_ops::enable_library(doc, tx, snapshot).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::DisableLibrary { library_id } => {
            crate::library_ops::disable_library(doc, tx, library_id).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::SetTextPatch { id, patch } => {
            editable(doc, *id)?;
            explicit.insert(*id);
            let families = desigma_render::available_families();
            let mut requested = Vec::new();
            if let Some(style) = &patch.style {
                requested.push(style.font_family.as_str());
            }
            if let Some(runs) = &patch.runs {
                requested.extend(runs.iter().map(|run| run.style.font_family.as_str()));
            }
            if let Some(family) = requested.into_iter().find(|family| !families.iter().any(|available| available.eq_ignore_ascii_case(family))) {
                let suggestions = families.iter().take(8).cloned().collect::<Vec<_>>();
                return Err(EngineError::new(
                    ErrorCode::UnsupportedOperation,
                    format!("font family '{family}' is unavailable; use desigma_list_fonts. Suggestions: {}", suggestions.join(", ")),
                ));
            }
            crate::design_ops::apply_text_patch(doc, tx, *id, patch).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::SetVisualPatch { id, patch } => {
            editable(doc, *id)?;
            explicit.insert(*id);
            crate::design_ops::apply_visual_patch(doc, tx, *id, patch).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::SetProperties { id, properties: p } => {
            editable(doc, *id)?;
            explicit.insert(*id);
            for v in [p.x, p.y, p.opacity].into_iter().flatten() {
                finite(v)?;
            }
            for v in [p.width, p.height].into_iter().flatten() {
                positive(v)?;
            }
            if p.opacity.is_some_and(|v| !(0.0..=1.0).contains(&v)) {
                return Err(invalid("opacity must be between zero and one"));
            }
            if let Some(rs) = p.corner_radii {
                for r in rs {
                    positive(r)?;
                }
            }
            let n = doc.get(*id).unwrap();
            if p.characters.is_some() && !matches!(n.kind, NodeKind::Text(_)) {
                return Err(invalid("characters requires a text node"));
            }
            if p.width.is_some() || p.height.is_some() || p.x.is_some() || p.y.is_some() {
                let mut t = n.transform;
                t.tx = p.x.unwrap_or(t.tx);
                t.ty = p.y.unwrap_or(t.ty);
                let size = Vec2::new(p.width.unwrap_or(n.size.x), p.height.unwrap_or(n.size.y));
                if p.width.is_some() || p.height.is_some() {
                    apply_resize(doc, &mut tx.ed, *id, size, t, true);
                } else {
                    tx.ed.node(doc, *id).unwrap().transform = t;
                }
            }
            let n = tx.ed.node(doc, *id).unwrap();
            if let Some(v) = &p.name {
                n.name = v.clone();
            }
            if let Some(v) = p.visible {
                n.visible = v;
            }
            if let Some(v) = p.locked {
                n.locked = v;
            }
            if let Some(v) = p.opacity {
                n.opacity = v;
            }
            if let Some(v) = p.corner_radii {
                n.corner_radii = v;
            }
            if let (Some(v), NodeKind::Text(t)) = (&p.characters, &mut n.kind) {
                if t.content != *v {
                    t.content = v.clone();
                    t.runs.clear();
                    t.lists.clear();
                    n.size = layout::fit_text(t, n.size);
                }
            }
            layout::relayout_around(doc, &mut tx.ed, *id);
        }
        Operation::SetFills { id, fills, bindings } => {
            editable(doc, *id)?;
            explicit.insert(*id);
            if fills.len() > 64 {
                return Err(invalid("at most 64 fills are allowed"));
            }
            for fill in fills {
                paint(fill, doc)?;
            }
            for (key, var) in bindings {
                let index = key.strip_prefix("fills/").and_then(|s| s.parse::<usize>().ok()).ok_or_else(|| invalid("fill bindings must use fills/<index>"))?;
                if index >= fills.len() {
                    return Err(invalid("fill binding index out of bounds"));
                }
                if !doc.variables.get(var).is_some_and(|v| v.kind == VariableKind::Color) {
                    return Err(invalid("fill binding requires an existing color variable"));
                }
                if !matches!(doc.resolve_variable(*id, var), Some(VariableValue::Color(_))) {
                    return Err(invalid("fill variable cannot be resolved"));
                }
            }
            let n = tx.ed.node(doc, *id).unwrap();
            n.fills = fills.clone();
            n.variables.bindings.retain(|k, _| !k.starts_with("fills/"));
            n.variables.bindings.extend(bindings.clone());
        }
        Operation::SetAutoLayout { id, auto_layout } => {
            editable(doc, *id)?;
            explicit.insert(*id);
            if let Some(al) = auto_layout {
                for v in al.padding {
                    positive(v)?;
                }
                finite(al.item_spacing)?;
                finite(al.counter_spacing)?;
            }
            let n = tx.ed.node(doc, *id).unwrap();
            let f = n.kind.frame_props_mut().ok_or_else(|| invalid("auto layout requires a frame"))?;
            f.auto_layout = *auto_layout;
            layout::relayout_around(doc, &mut tx.ed, *id);
        }
        Operation::CreateInstance { component, parent, at } => {
            editable(doc, *parent)?;
            finite(at.x)?;
            finite(at.y)?;
            if !doc.get(*component).is_some_and(|n| matches!(n.kind, NodeKind::Component(_))) {
                return Err(invalid("source must be a component"));
            }
            let c = create_instance(doc, *component, *parent, *at);
            if matches!(&c,Command::Batch(v) if v.is_empty()) {
                return Err(EngineError::new(ErrorCode::OperationConflict, "cannot create recursive instance"));
            }
            if let Command::Insert { nodes, .. } = &c {
                explicit.extend(nodes.first().map(|n| n.id));
            }
            tx.cmd(doc, c)?;
            layout::relayout_around(doc, &mut tx.ed, *parent);
        }
        Operation::SetVariant { id, component } => {
            editable(doc, *id)?;
            explicit.insert(*id);
            if !doc.get(*id).is_some_and(is_top_instance) {
                return Err(invalid("variant requires an instance"));
            }
            if !doc.get(*component).is_some_and(|n| matches!(n.kind, NodeKind::Component(_))) {
                return Err(invalid("variant source must be a component"));
            }
            if *id == *component || doc.is_ancestor(*component, *id) {
                return Err(EngineError::new(ErrorCode::OperationConflict, "cannot create recursive instance"));
            }
            let c = swap_instance(doc, *id, *component);
            tx.cmd(doc, c)?;
            refit_texts(doc, &mut tx.ed, *id);
            layout::relayout_around(doc, &mut tx.ed, *id);
        }
        Operation::CombineAsVariants { temp_id, ids } => {
            if !valid_temp_id(temp_id) || temps.contains_key(temp_id) {
                return Err(invalid("temp_id must be a unique $newN handle"));
            }
            if ids.len() < 2 || ids.len() > 10_000 {
                return Err(invalid("combine_as_variants requires 2..10000 component IDs"));
            }
            let component_ids: Vec<_> = ids.iter().map(|id| resolve_ref(id, temps)).collect::<Result<_, _>>()?;
            let mut unique = BTreeSet::new();
            let mut parent = None;
            for id in &component_ids {
                if !unique.insert(*id) {
                    return Err(invalid("component IDs must be unique"));
                }
                editable(doc, *id)?;
                let node = doc.get(*id).ok_or_else(|| EngineError::new(ErrorCode::MissingNode, format!("node {} not found", id.0)))?;
                if !matches!(node.kind, NodeKind::Component(_)) {
                    return Err(invalid("combine_as_variants requires component nodes"));
                }
                let current_parent = node.parent.ok_or_else(|| invalid("components must be siblings"))?;
                if parent.is_some_and(|p| p != current_parent) {
                    return Err(invalid("components must be siblings"));
                }
                parent = Some(current_parent);
            }
            let command = combine_as_variants(doc, &component_ids);
            let set_id = match &command {
                Command::Batch(commands) => commands.iter().find_map(|command| match command {
                    Command::Insert { nodes, .. } => nodes.first().map(|node| node.id),
                    _ => None,
                }),
                _ => None,
            }
            .ok_or_else(|| invalid("could not create component set"))?;
            tx.cmd(doc, command)?;
            temps.insert(temp_id.clone(), set_id);
            explicit.extend(component_ids);
            explicit.insert(set_id);
        }
        Operation::CreateVariableCollection { temp_id, name } => {
            if !valid_temp_id(temp_id) || variable_temps.contains_key(temp_id) || temps.contains_key(temp_id) {
                return Err(invalid("temp_id must be a unique $newN handle"));
            }
            let id = crate::variable_ops::create_collection(doc, tx, name).map_err(|e| invalid(e.to_string()))?;
            let default_mode = doc.variable_collections.get(&id).map(|c| c.default_mode.clone()).ok_or_else(|| invalid("created collection missing"))?;
            variable_temps.insert(temp_id.clone(), id);
            variable_temps.insert(format!("{temp_id}_mode"), default_mode);
        }
        Operation::CreateVariableMode { temp_id, collection_id, name } => {
            if !valid_temp_id(temp_id) || variable_temps.contains_key(temp_id) || temps.contains_key(temp_id) {
                return Err(invalid("temp_id must be a unique $newN handle"));
            }
            let collection_id = resolve_variable_ref(collection_id, variable_temps)?;
            let id = crate::variable_ops::create_mode(doc, tx, &collection_id, name).map_err(|e| invalid(e.to_string()))?;
            variable_temps.insert(temp_id.clone(), id);
        }
        Operation::CreateVariable { temp_id, collection_id, name, kind } => {
            if !valid_temp_id(temp_id) || variable_temps.contains_key(temp_id) || temps.contains_key(temp_id) {
                return Err(invalid("temp_id must be a unique $newN handle"));
            }
            let collection_id = resolve_variable_ref(collection_id, variable_temps)?;
            let id = crate::variable_ops::create_variable(doc, tx, &collection_id, name, *kind).map_err(|e| invalid(e.to_string()))?;
            variable_temps.insert(temp_id.clone(), id);
        }
        Operation::RenameVariable { id, name } => {
            let id = resolve_variable_ref(id, variable_temps)?;
            crate::variable_ops::rename_variable(doc, tx, &id, name).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::DeleteVariable { id } => {
            let id = resolve_variable_ref(id, variable_temps)?;
            crate::variable_ops::delete_variable(doc, tx, &id).map_err(|e| invalid(e.to_string()))?;
        }
        Operation::BindVariable { id, property, variable_id } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            let variable_id = resolve_variable_ref(variable_id, variable_temps)?;
            crate::variable_ops::bind_property(doc, tx, id, property, &variable_id).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::UnbindVariable { id, property } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::variable_ops::unbind_property(doc, tx, id, property).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::SetNodeMode { id, collection_id, mode_id } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            let collection_id = resolve_variable_ref(collection_id, variable_temps)?;
            let mode_id = mode_id.as_ref().map(|value| resolve_variable_ref(value, variable_temps)).transpose()?;
            crate::variable_ops::set_node_mode(doc, tx, id, &collection_id, mode_id.as_deref()).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::SetVectorNetwork { id, network } => {
            let id = resolve_ref(id, temps)?;
            editable(doc, id)?;
            crate::vector_ops::set_vector_network(doc, tx, id, network.clone()).map_err(|e| invalid(e.to_string()))?;
            explicit.insert(id);
        }
        Operation::BooleanOp { temp_id, ids, boolean_op } => {
            if !valid_temp_id(temp_id) || temps.contains_key(temp_id) {
                return Err(invalid("temp_id must be a unique $newN handle"));
            }
            let ids = ids.iter().map(|r| resolve_ref(r, temps)).collect::<Result<Vec<_>, _>>()?;
            for id in &ids {
                editable(doc, *id)?;
            }
            let id = crate::vector_ops::boolean_op(doc, tx, &ids, *boolean_op).map_err(|e| invalid(e.to_string()))?;
            temps.insert(temp_id.clone(), id);
            explicit.insert(id);
        }
        Operation::Flatten { temp_id, ids } => {
            if !valid_temp_id(temp_id) || temps.contains_key(temp_id) {
                return Err(invalid("temp_id must be a unique $newN handle"));
            }
            let ids = ids.iter().map(|r| resolve_ref(r, temps)).collect::<Result<Vec<_>, _>>()?;
            for id in &ids {
                editable(doc, *id)?;
            }
            let id = crate::vector_ops::flatten(doc, tx, &ids).map_err(|e| invalid(e.to_string()))?;
            temps.insert(temp_id.clone(), id);
            explicit.insert(id);
        }
        Operation::OutlineStroke { temp_id, id } => {
            if !valid_temp_id(temp_id) || temps.contains_key(temp_id) {
                return Err(invalid("temp_id must be a unique $newN handle"));
            }
            let source = resolve_ref(id, temps)?;
            editable(doc, source)?;
            let id = crate::vector_ops::outline_stroke_node(doc, tx, source).map_err(|e| invalid(e.to_string()))?;
            temps.insert(temp_id.clone(), id);
            explicit.insert(id);
            explicit.insert(source);
        }
        Operation::EditVariable { id, mode, value } => {
            let id = resolve_variable_ref(id, variable_temps)?;
            let mode = resolve_variable_ref(mode, variable_temps)?;
            let mut value = value.clone();
            if let VariableValue::Alias(alias) = &mut value {
                *alias = resolve_variable_ref(alias, variable_temps)?;
            }
            let v = doc.variables.get(&id).ok_or_else(|| invalid("variable not found"))?;
            let coll = doc.variable_collections.get(&v.collection).ok_or_else(|| invalid("variable collection not found"))?;
            if !coll.modes.iter().any(|m| m.id == mode) {
                return Err(invalid("variable mode not found"));
            }
            let compatible = match &value {
                VariableValue::Color(c) => {
                    color(*c)?;
                    v.kind == VariableKind::Color
                }
                VariableValue::Number(n) => {
                    finite(*n)?;
                    v.kind == VariableKind::Number
                }
                VariableValue::String(_) => v.kind == VariableKind::String,
                VariableValue::Boolean(_) => v.kind == VariableKind::Boolean,
                VariableValue::Alias(a) => doc.variables.get(a).is_some_and(|other| a != &id && other.kind == v.kind),
            };
            if !compatible {
                return Err(invalid("variable value type mismatch or invalid alias"));
            }
            let mut store = VariableStore { variables: doc.variables.clone(), collections: doc.variable_collections.clone() };
            if !match &value {
                VariableValue::Alias(target) => store.set_alias(&id, &mode, target),
                _ => store.set_value(&id, &mode, value.clone()),
            } {
                return Err(invalid("invalid variable value or cyclic alias"));
            }
            if store.variables == doc.variables {
                return Ok(());
            }
            let changed = crate::variables::changed_variables(doc, &store, &[]);
            let nodes = crate::variables::bound_nodes(doc, &changed);
            for node in &nodes {
                editable(doc, *node)?;
            }
            tx.cmd(doc, Command::SetVariables { variables: store.variables, collections: store.collections })?;
            // Reflow writes through document helpers; remember every original node before doing so.
            for node in doc.nodes.keys().copied().collect::<Vec<_>>() {
                tx.ed.touch(doc, node);
            }
            crate::variables::reflow_bound(doc, &changed, &nodes);
        }
    }
    Ok(())
}
fn refit_texts(doc: &mut Document, ed: &mut crate::edit::Edit, root: NodeId) {
    let mut ids = Vec::new();
    doc.walk(root, |n, _| ids.push(n.id));
    for id in ids {
        let size = doc.get(id).and_then(|n| if let NodeKind::Text(t) = &n.kind { Some(layout::fit_text(t, n.size)) } else { None });
        if let Some(size) = size {
            ed.node(doc, id).unwrap().size = size;
        }
    }
}

fn allocation_budget(doc: &Document, operations: usize) -> Result<(), EngineError> {
    let nodes = (doc.nodes.len().max(1)) as u64;
    let ops = (operations as u64).checked_add(1).ok_or_else(|| invalid("operation count overflow"))?;
    let budget =
        nodes.checked_mul(nodes).and_then(|n| n.checked_mul(ops)).and_then(|n| n.checked_mul(ops)).ok_or_else(|| invalid("allocation budget overflow"))?;
    if doc.remaining_node_ids() < budget {
        return Err(EngineError::new(
            ErrorCode::OperationConflict,
            "document has exhausted node IDs or insufficient capacity for transaction and component synchronization",
        ));
    }
    Ok(())
}
fn reserve_existing_ids(doc: &mut Document) {
    if let Some(id) = doc.nodes.keys().next_back().copied() {
        doc.reserve_ids(id);
    }
}

// Guard recursive document algorithms against malformed imported trees before editing.
fn validate_structure(doc: &Document) -> Result<(), EngineError> {
    if doc.nodes.len() > 100_000 {
        return Err(invalid("document exceeds 100000 node editing limit"));
    }
    for (id, n) in &doc.nodes {
        if *id != n.id {
            return Err(invalid("node key differs from node id"));
        }
        if let Some(parent) = n.parent {
            let p = doc.get(parent).ok_or_else(|| invalid("missing structural parent"))?;
            if p.children.iter().filter(|child| **child == *id).count() != 1 {
                return Err(invalid("inconsistent parent/child links"));
            }
        }
        let mut children = BTreeSet::new();
        for child in &n.children {
            if !children.insert(*child) || doc.get(*child).is_none_or(|c| c.parent != Some(*id)) {
                return Err(invalid("inconsistent or duplicate children"));
            }
        }
        if !n.children.is_empty() && !n.kind.is_container() {
            return Err(invalid("non-container node has children"));
        }
        let mut seen = BTreeSet::new();
        let mut cur = Some(*id);
        while let Some(id) = cur {
            if !seen.insert(id) || seen.len() > 256 {
                return Err(EngineError::new(ErrorCode::OperationConflict, "cyclic or excessively deep document tree"));
            }
            cur = doc.get(id).and_then(|n| n.parent);
        }
    }
    Ok(())
}

/// Validate and apply on an isolated document. Only a successful commit replaces the caller's
/// document; history and all dependent changes form exactly one undo step.
pub fn apply_ops(doc: &mut Document, history: &mut History, request: ApplyRequest) -> Result<ChangeSummary, EngineError> {
    if request.expected_revision != doc.revision {
        return Err(EngineError::new(ErrorCode::StaleRevision, format!("expected revision {}, current revision {}", request.expected_revision, doc.revision)));
    }
    if request.ops.is_empty() || request.ops.len() > MAX_OPERATIONS {
        return Err(invalid("batch must contain between 1 and 256 operations"));
    }
    if serde_json::to_vec(&request).map_err(|e| invalid(e.to_string()))?.len() > MAX_REQUEST_BYTES {
        return Err(invalid("request exceeds one MiB"));
    }
    validate_structure(doc)?;
    allocation_budget(doc, request.ops.len())?;
    let mut trial = doc.clone();
    reserve_existing_ids(&mut trial);
    let mut tx = Tx::new();
    let mut explicit = BTreeSet::new();
    let mut temp_ids = BTreeMap::new();
    let mut temp_variable_ids = BTreeMap::new();
    let mut duplicate_mappings = BTreeMap::new();
    for op in &request.ops {
        operation(&mut trial, &mut tx, op, &mut explicit, &mut temp_ids, &mut temp_variable_ids, &mut duplicate_mappings)?;
    }
    if request.dry_run {
        tx.commit(&mut trial, &mut History::default())?;
    } else {
        tx.commit(&mut trial, history)?;
    }
    let mut changes = Vec::new();
    let mut created_node_ids = Vec::new();
    for id in doc.nodes.keys().chain(trial.nodes.keys()).copied().collect::<BTreeSet<_>>() {
        let old = doc.nodes.get(&id);
        let new = trial.nodes.get(&id);
        if old == new {
            continue;
        }
        let properties = match (old, new) {
            (Some(a), Some(b)) => {
                let a = serde_json::to_value(a).unwrap();
                let b = serde_json::to_value(b).unwrap();
                a.as_object().unwrap().iter().filter(|(key, value)| b.get(*key) != Some(*value)).map(|(key, _)| key.clone()).collect()
            }
            (None, Some(_)) => {
                created_node_ids.push(id);
                vec!["created".into()]
            }
            _ => vec!["removed".into()],
        };
        changes.push(NodeChange { id, properties, derived: !explicit.contains(&id) });
    }
    let changed_variable_ids = doc
        .variables
        .keys()
        .chain(trial.variables.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|id| doc.variables.get(*id) != trial.variables.get(*id))
        .cloned()
        .collect();
    let revision = if request.dry_run { doc.revision } else { trial.revision };
    if !request.dry_run {
        *doc = trial;
    }
    Ok(ChangeSummary { revision, dry_run: request.dry_run, changes, created_node_ids, temp_ids, duplicate_mappings, temp_variable_ids, changed_variable_ids })
}

/// Trusted compatibility entry point for existing editor and offline command callers.
/// Agent-facing callers should use the constrained typed operations above.
pub fn commit_commands(doc: &mut Document, history: &mut History, commands: Vec<Command>) -> Result<bool, EngineError> {
    validate_structure(doc)?;
    allocation_budget(doc, commands.len())?;
    let mut trial = doc.clone();
    reserve_existing_ids(&mut trial);
    let mut tx = Tx::new();
    for c in commands {
        tx.cmd(&mut trial, c)?;
    }
    validate_structure(&trial)?;
    for id in trial.pages.clone() {
        layout::relayout_around(&mut trial, &mut tx.ed, id);
    }
    let changed = tx.commit(&mut trial, history)?;
    *doc = trial;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Document, NodeId) {
        let mut doc = Document::new("test");
        let page = doc.add_page("page");
        let id = doc.alloc_id();
        doc.insert(page, Node::rectangle(id, Rect::new(0., 0., 20., 20.))).unwrap();
        (doc, id)
    }
    fn variant_components() -> (Document, NodeId, NodeId) {
        let mut doc = Document::new("variants");
        let page = doc.add_page("Page");
        let a = doc.alloc_id();
        let mut small = Node::frame(a, Rect::new(0.0, 0.0, 100.0, 40.0));
        small.name = "Size=Small".into();
        doc.insert(page, small).unwrap();
        let b = doc.alloc_id();
        let mut large = Node::frame(b, Rect::new(0.0, 80.0, 160.0, 60.0));
        large.name = "Size=Large".into();
        doc.insert(page, large).unwrap();
        for id in [a, b] {
            let command = create_component(&mut doc, id);
            command.apply(&mut doc).unwrap();
        }
        (doc, a, b)
    }
    #[test]
    fn combine_as_variants_creates_mapped_set_in_one_undo_step() {
        let (mut doc, a, b) = variant_components();
        let mut history = History::default();
        let revision = doc.revision;
        let summary = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![Operation::CombineAsVariants { temp_id: "$new1".into(), ids: vec![NodeRef::Id(a), NodeRef::Id(b)] }],
            },
        )
        .unwrap();
        let set = summary.temp_ids["$new1"];
        assert!(matches!(doc.get(set).unwrap().kind, NodeKind::ComponentSet(_)));
        assert_eq!(doc.get(set).unwrap().children, vec![a, b]);
        history.undo(&mut doc).unwrap();
        assert!(doc.get(set).is_none());
        assert_eq!(doc.get(a).unwrap().parent, doc.get(b).unwrap().parent);
    }

    #[test]
    fn combine_as_variants_rejects_invalid_components_atomically() {
        let (mut doc, a, _b) = variant_components();
        let page = doc.pages[0];
        let invalid = doc.alloc_id();
        doc.insert(page, Node::rectangle(invalid, Rect::new(200.0, 0.0, 20.0, 20.0))).unwrap();
        let before = doc.to_json().unwrap();
        let mut history = History::default();
        let revision = doc.revision;
        for ids in [vec![NodeRef::Id(a), NodeRef::Id(a)], vec![NodeRef::Id(a), NodeRef::Id(invalid)]] {
            assert!(apply_ops(
                &mut doc,
                &mut history,
                ApplyRequest { expected_revision: revision, dry_run: false, ops: vec![Operation::CombineAsVariants { temp_id: "$new1".into(), ids }] },
            )
            .is_err());
            assert_eq!(doc.to_json().unwrap(), before);
            assert!(!history.can_undo());
        }
        let holder = doc.alloc_id();
        doc.insert(page, Node::frame(holder, Rect::new(300.0, 0.0, 100.0, 80.0))).unwrap();
        let other = doc.alloc_id();
        let mut frame = Node::frame(other, Rect::new(0.0, 0.0, 80.0, 40.0));
        frame.name = "Size=Other".into();
        doc.insert(holder, frame).unwrap();
        let command = create_component(&mut doc, other);
        command.apply(&mut doc).unwrap();
        let revision = doc.revision;
        assert!(apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![Operation::CombineAsVariants { temp_id: "$new2".into(), ids: vec![NodeRef::Id(a), NodeRef::Id(other)] }],
            },
        )
        .is_err());
    }
    #[test]
    fn set_variable_expression_is_typed_dry_runnable_and_undoable() {
        let (mut doc, _) = fixture();
        let mut store = VariableStore::from_doc(&doc);
        let collection = store.add_collection("Cart");
        let source = store.add_variable(&collection, "Count", VariableKind::Number).unwrap();
        let target = store.add_variable(&collection, "Total", VariableKind::Number).unwrap();
        let mode = store.collections[&collection].default_mode.clone();
        store.variables.get_mut(&source).unwrap().values.insert(mode.clone(), VariableValue::Number(4.0));
        doc.variables = store.variables;
        doc.variable_collections = store.collections;
        let mut history = History::default();
        let request = ApplyRequest {
            expected_revision: doc.revision,
            dry_run: true,
            ops: vec![Operation::SetVariableExpression { variable_id: target.clone(), mode_id: mode.clone(), expression: "$Count * 2".into() }],
        };
        let preview = apply_ops(&mut doc, &mut history, request.clone()).unwrap();
        assert!(preview.dry_run);
        assert_eq!(doc.variables[&target].values[&mode], VariableValue::Number(0.0));
        apply_ops(&mut doc, &mut history, ApplyRequest { dry_run: false, ..request }).unwrap();
        assert_eq!(doc.variables[&target].values[&mode], VariableValue::Number(8.0));
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.variables[&target].values[&mode], VariableValue::Number(0.0));
    }

    #[test]
    fn set_variable_expression_rejects_invalid_expression_atomically() {
        let (mut doc, _) = fixture();
        let mut store = VariableStore::from_doc(&doc);
        let collection = store.add_collection("Cart");
        let target = store.add_variable(&collection, "Total", VariableKind::Number).unwrap();
        let mode = store.collections[&collection].default_mode.clone();
        doc.variables = store.variables;
        doc.variable_collections = store.collections;
        let before = doc.to_json().unwrap();
        let mut history = History::default();
        let revision = doc.revision;
        let result = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![Operation::SetVariableExpression { variable_id: target, mode_id: mode, expression: "unknown + 1".into() }],
            },
        );
        assert!(result.is_err());
        assert_eq!(doc.to_json().unwrap(), before);
        assert!(!history.can_undo());
    }
    #[test]
    fn typed_style_crud_preview_commit_and_undo_are_atomic() {
        let (mut doc, id) = fixture();
        let original = doc.clone();
        let mut history = History::default();
        let ops = vec![
            Operation::CreateStyle {
                key: "brand".into(),
                style: Style::Fill(vec![Paint::solid(Color::WHITE)]),
                meta: StyleMeta { name: "Brand".into(), property: StyleProperty::Fill, ..Default::default() },
            },
            Operation::ApplyStyle { ids: vec![NodeRef::Id(id)], key: "brand".into(), property: None },
            Operation::UpdateStyle { key: "brand".into(), style: Style::Fill(vec![Paint::solid(Color::BLACK)]), meta: None },
            Operation::DeleteStyle { key: "brand".into() },
        ];
        let revision = doc.revision;
        let preview = apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: true, ops: ops.clone() }).unwrap();
        assert!(preview.dry_run);
        assert_eq!(doc.to_json().unwrap(), original.to_json().unwrap());
        assert!(!history.can_undo());

        let result = apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: false, ops }).unwrap();
        assert_eq!(result.revision, revision + 1);
        assert!(!doc.styles.contains_key("brand"));
        assert!(!doc.get(id).unwrap().style_refs.contains_key("fill"));
        assert_eq!(doc.get(id).unwrap().fills, vec![Paint::solid(Color::BLACK)]);
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.nodes, original.nodes);
        assert_eq!(serde_json::to_value(&doc.styles).unwrap(), serde_json::to_value(&original.styles).unwrap());
        assert_eq!(doc.style_meta, original.style_meta);
        assert!(!history.can_undo());
    }

    #[test]
    fn typed_style_operation_failure_discards_prior_changes() {
        let (mut doc, id) = fixture();
        let original = doc.to_json().unwrap();
        let mut history = History::default();
        let revision = doc.revision;
        let error = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![
                    Operation::CreateStyle {
                        key: "brand".into(),
                        style: Style::Fill(vec![Paint::solid(Color::WHITE)]),
                        meta: StyleMeta { name: "Brand".into(), property: StyleProperty::Fill, ..Default::default() },
                    },
                    Operation::ApplyStyle { ids: vec![NodeRef::Id(id)], key: "missing".into(), property: None },
                ],
            },
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidOperation);
        assert_eq!(doc.to_json().unwrap(), original);
        assert!(!history.can_undo());
    }

    #[test]
    fn typed_dev_handoff_is_dry_runnable_and_one_step_undoable() {
        let mut doc = Document::new("handoff");
        let page = doc.add_page("page");
        let frame = doc.alloc_id();
        doc.insert(page, Node::frame(frame, Rect::new(0., 0., 100., 100.))).unwrap();
        let before = doc.to_json().unwrap();
        let mut history = History::default();
        let ops = vec![
            Operation::SetDevStatus {
                id: NodeRef::Id(frame),
                status: Some(DevStatus::ReadyForDev),
                updated_at: Some("2026-10-15T10:30:00Z".into()),
                snapshot_version: None,
            },
            Operation::SetDevAnnotations {
                id: NodeRef::Id(frame),
                annotations: vec![Annotation { text: "Use 8px radius".into(), properties: vec!["Corner radius".into()], category: Some("Layout".into()) }],
            },
        ];
        let revision = doc.revision;
        let preview = apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: true, ops: ops.clone() }).unwrap();
        assert!(preview.dry_run);
        assert_eq!(doc.to_json().unwrap(), before);
        assert!(!history.can_undo());

        apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: false, ops }).unwrap();
        assert_eq!(doc.get(frame).unwrap().dev.status, Some(DevStatus::ReadyForDev));
        assert_eq!(doc.get(frame).unwrap().dev.annotations.len(), 1);
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.get(frame).unwrap().dev, DevInfo::default());
        assert!(!history.can_undo());
    }

    #[test]
    fn typed_duplicate_deep_copies_with_mapping_and_undo() {
        let mut doc = Document::new("duplicate");
        let page = doc.add_page("page");
        let frame = doc.alloc_id();
        doc.insert(page, Node::frame(frame, Rect::new(3., 4., 80., 60.))).unwrap();
        let child = doc.alloc_id();
        doc.insert(frame, Node::rectangle(child, Rect::new(5., 6., 10., 10.))).unwrap();
        let original = doc.clone();
        let revision = doc.revision;
        let mut history = History::default();
        let ops =
            vec![Operation::DuplicateNode { temp_id: "$new1".into(), id: NodeRef::Id(frame), parent: None, index: None, offset: Some(Vec2::new(20., -8.)) }];
        let preview = apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: true, ops: ops.clone() }).unwrap();
        assert_eq!(preview.duplicate_mappings["$new1"].len(), 2);
        assert_eq!(doc.to_json().unwrap(), original.to_json().unwrap());
        assert!(!history.can_undo());
        let result = apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: false, ops }).unwrap();
        let mapping = &result.duplicate_mappings["$new1"];
        let copy = mapping[&frame];
        let copy_child = mapping[&child];
        assert_eq!(result.temp_ids["$new1"], copy);
        assert_eq!(doc.get(copy).unwrap().transform.tx, 23.);
        assert_eq!(doc.get(copy).unwrap().transform.ty, -4.);
        assert_eq!(doc.get(copy).unwrap().children, vec![copy_child]);
        assert_eq!(doc.get(copy_child).unwrap().parent, Some(copy));
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.nodes, original.nodes);
        assert_eq!(doc.pages, original.pages);
        assert!(!history.can_undo());
    }

    #[test]
    fn typed_duplicate_failure_rolls_back_prior_create() {
        let (mut doc, source) = fixture();
        let page = doc.pages[0];
        let before = doc.to_json().unwrap();
        let mut history = History::default();
        let revision = doc.revision;
        let error = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![
                    Operation::CreateNode {
                        temp_id: "$new1".into(),
                        parent: NodeRef::Id(page),
                        index: None,
                        kind: "rectangle".into(),
                        properties: NodeProperties::default(),
                        board: None,
                    },
                    Operation::DuplicateNode { temp_id: "$new2".into(), id: NodeRef::Id(source), parent: Some(NodeRef::Id(source)), index: None, offset: None },
                ],
            },
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidOperation);
        assert_eq!(doc.to_json().unwrap(), before);
        assert!(!history.can_undo());
    }
    #[test]
    fn typed_create_temp_refs_are_atomic_previewable_and_undoable() {
        let (mut doc, _) = fixture();
        let page = doc.pages[0];
        let mut history = History::default();
        let request = ApplyRequest {
            expected_revision: doc.revision,
            dry_run: true,
            ops: vec![
                Operation::CreateNode {
                    temp_id: "$new1".into(),
                    parent: NodeRef::Id(page),
                    index: None,
                    kind: "frame".into(),
                    properties: NodeProperties::default(),
                    board: None,
                },
                Operation::CreateNode {
                    temp_id: "$new2".into(),
                    parent: NodeRef::Temp("$new1".into()),
                    index: None,
                    kind: "text".into(),
                    properties: NodeProperties { characters: Some("hello".into()), ..Default::default() },
                    board: None,
                },
                Operation::DeleteNode { id: NodeRef::Temp("$new2".into()) },
            ],
        };
        let before = doc.to_json().unwrap();
        let preview = apply_ops(&mut doc, &mut history, request.clone()).unwrap();
        assert!(preview.dry_run && preview.temp_ids.contains_key("$new1"));
        assert_eq!(doc.to_json().unwrap(), before);
        let committed = apply_ops(&mut doc, &mut history, ApplyRequest { dry_run: false, ..request }).unwrap();
        let parent = committed.temp_ids["$new1"];
        assert!(doc.get(parent).is_some());
        assert_eq!(doc.get(parent).unwrap().children.len(), 0);
        assert!(history.undo(&mut doc).unwrap());
        assert_eq!(doc.nodes.len(), 2);
    }
    #[test]
    fn group_and_ungroup_preserve_sibling_positions_and_are_undoable() {
        let (mut doc, first) = fixture();
        let page = doc.pages[0];
        let second = doc.alloc_id();
        doc.insert(page, Node::rectangle(second, Rect::new(60., 5., 15., 15.))).unwrap();
        let before = doc.nodes.clone();
        let mut history = History::default();
        let initial_revision = doc.revision;
        let result = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: initial_revision,
                dry_run: false,
                ops: vec![Operation::GroupNodes { temp_id: "$new1".into(), ids: vec![NodeRef::Id(first), NodeRef::Id(second)], name: Some("Pair".into()) }],
            },
        )
        .unwrap();
        let group = result.temp_ids["$new1"];
        assert_eq!(doc.get(group).unwrap().children, vec![first, second]);
        assert_eq!(doc.get(first).unwrap().transform.tx, 0.);
        let revision = doc.revision;
        apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest { expected_revision: revision, dry_run: false, ops: vec![Operation::Ungroup { id: NodeRef::Id(group) }] },
        )
        .unwrap();
        assert_eq!(doc.get(page).unwrap().children, vec![first, second]);
        history.undo(&mut doc).unwrap();
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.nodes, before);
    }
    #[test]
    fn reparent_keeps_world_position_across_translated_frames() {
        let mut doc = Document::new("move");
        let page = doc.add_page("page");
        let a = doc.alloc_id();
        let b = doc.alloc_id();
        let child = doc.alloc_id();
        doc.insert(page, Node::frame(a, Rect::new(100., 20., 80., 80.))).unwrap();
        doc.insert(page, Node::frame(b, Rect::new(200., 30., 80., 80.))).unwrap();
        doc.insert(a, Node::rectangle(child, Rect::new(10., 5., 10., 10.))).unwrap();
        let before = doc.world_transform(child);
        let mut history = History::default();
        let revision = doc.revision;
        apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![Operation::MoveNode { id: NodeRef::Id(child), parent: NodeRef::Id(b), index: None }],
            },
        )
        .unwrap();
        assert_eq!(doc.world_transform(child), before);
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.get(child).unwrap().parent, Some(a));
        assert_eq!(doc.world_transform(child), before);
    }
    #[test]
    fn page_lifecycle_and_reordering_are_transactional() {
        let (mut doc, _) = fixture();
        let first = doc.pages[0];
        let mut history = History::default();
        let revision = doc.revision;
        let created = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![Operation::CreatePage { temp_id: "$new1".into(), name: "Second".into(), index: None }],
            },
        )
        .unwrap();
        let second = created.temp_ids["$new1"];
        let revision = doc.revision;
        apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![Operation::ReorderPages { order: vec![NodeRef::Id(second), NodeRef::Id(first)] }],
            },
        )
        .unwrap();
        assert_eq!(doc.pages, vec![second, first]);
        let revision = doc.revision;
        apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest { expected_revision: revision, dry_run: false, ops: vec![Operation::DeletePage { id: NodeRef::Id(first) }] },
        )
        .unwrap();
        assert_eq!(doc.pages, vec![second]);
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.pages, vec![second, first]);
    }
    #[test]
    fn transparent_frame_wrap_is_undoable_and_preserves_geometry() {
        let (mut doc, id) = fixture();
        let before = doc.nodes.clone();
        let mut history = History::default();
        let revision = doc.revision;
        let result = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![Operation::WrapInFrame { temp_id: "$new1".into(), ids: vec![NodeRef::Id(id)], name: None }],
            },
        )
        .unwrap();
        let frame = result.temp_ids["$new1"];
        assert_eq!(doc.get(id).unwrap().transform, Affine::IDENTITY);
        assert!(doc.get(frame).unwrap().fills.is_empty());
        assert_eq!(doc.get(id).unwrap().parent, Some(frame));
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.nodes, before);
    }

    #[test]
    fn typed_variable_crud_bindings_and_modes_batch_atomically() {
        let (mut doc, rect) = fixture();
        let frame = doc.pages[0];
        let original = doc.nodes.clone();
        let mut history = History::default();
        let ops = vec![
            Operation::CreateVariableCollection { temp_id: "$new1".into(), name: "Sizing".into() },
            Operation::CreateVariableMode { temp_id: "$new2".into(), collection_id: "$new1".into(), name: "Wide".into() },
            Operation::CreateVariable { temp_id: "$new3".into(), collection_id: "$new1".into(), name: "Width".into(), kind: VariableKind::Number },
            Operation::EditVariable { id: "$new3".into(), mode: "$new1_mode".into(), value: VariableValue::Number(64.0) },
            Operation::EditVariable { id: "$new3".into(), mode: "$new2".into(), value: VariableValue::Number(96.0) },
            Operation::BindVariable { id: NodeRef::Id(rect), property: "width".into(), variable_id: "$new3".into() },
            Operation::SetNodeMode { id: NodeRef::Id(frame), collection_id: "$new1".into(), mode_id: Some("$new2".into()) },
            Operation::RenameVariable { id: "$new3".into(), name: "Sizing/Width".into() },
        ];
        let revision = doc.revision;
        let preview = apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: true, ops: ops.clone() }).unwrap();
        assert_eq!(preview.temp_variable_ids["$new3"], preview.changed_variable_ids.iter().find(|id| id.contains("VariableID")).cloned().unwrap());
        assert_eq!(doc.nodes, original);
        assert!(doc.variables.is_empty());
        let revision = doc.revision;
        let result = apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: false, ops }).unwrap();
        let variable_id = &result.temp_variable_ids["$new3"];
        assert_eq!(doc.get(rect).unwrap().size.x, 96.0);
        assert_eq!(doc.variables[variable_id].name, "Sizing/Width");
        assert!(doc.get(rect).unwrap().variables.bindings.contains_key("width"));
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.nodes, original);
        assert!(doc.variables.is_empty());
        assert!(doc.variable_collections.is_empty());
    }

    #[test]
    fn typed_variable_invalid_binding_rolls_back_entire_batch() {
        let (mut doc, rect) = fixture();
        let before = doc.to_json().unwrap();
        let mut history = History::default();
        let revision = doc.revision;
        let error = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![
                    Operation::CreateVariableCollection { temp_id: "$new1".into(), name: "Sizing".into() },
                    Operation::CreateVariable { temp_id: "$new2".into(), collection_id: "$new1".into(), name: "Flag".into(), kind: VariableKind::Boolean },
                    Operation::BindVariable { id: NodeRef::Id(rect), property: "width".into(), variable_id: "$new2".into() },
                ],
            },
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidOperation);
        assert_eq!(doc.to_json().unwrap(), before);
        assert!(!history.can_undo());
    }
    #[test]
    fn rejects_structural_properties_at_schema_boundary() {
        assert!(serde_json::from_value::<Operation>(serde_json::json!({"op":"set_properties","id":1,"properties":{"children":[2]}})).is_err());
        assert!(serde_json::from_value::<Operation>(serde_json::json!({"op":"set_properties","id":1,"properties":{"parent":2}})).is_err());
    }
    #[test]
    fn rejects_unknown_nested_native_fields() {
        let mut fill = serde_json::to_value(Paint::solid(Color::WHITE)).unwrap();
        fill["kind"]["Solid"]["typo"] = serde_json::json!(1);
        assert!(serde_json::from_value::<Operation>(serde_json::json!({"op":"set_fills","id":1,"fills":[fill]})).is_err());
    }
    #[test]
    fn rejects_nonfinite_sizes_without_mutation() {
        let (mut doc, id) = fixture();
        let before = serde_json::to_value(&doc).unwrap();
        let mut hist = History::default();
        let err = apply_ops(
            &mut doc,
            &mut hist,
            ApplyRequest {
                expected_revision: 0,
                dry_run: false,
                ops: vec![Operation::SetProperties { id, properties: NodeProperties { width: Some(f32::NAN), ..Default::default() } }],
            },
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidOperation);
        assert_eq!(serde_json::to_value(&doc).unwrap(), before);
        assert!(!hist.can_undo());
    }
    #[test]
    fn rejects_malformed_tree_before_layout_recursion() {
        let (mut doc, id) = fixture();
        doc.get_mut(id).unwrap().parent = Some(id);
        let mut hist = History::default();
        assert!(apply_ops(
            &mut doc,
            &mut hist,
            ApplyRequest {
                expected_revision: 0,
                dry_run: false,
                ops: vec![Operation::SetProperties { id, properties: NodeProperties { width: Some(30.), ..Default::default() } }]
            }
        )
        .is_err());
        assert!(!hist.can_undo());
    }
    #[test]
    fn rejects_edits_beneath_locked_parent() {
        let (mut doc, id) = fixture();
        let page = doc.pages[0];
        doc.get_mut(page).unwrap().locked = true;
        let mut hist = History::default();
        let err = apply_ops(
            &mut doc,
            &mut hist,
            ApplyRequest {
                expected_revision: 0,
                dry_run: false,
                ops: vec![Operation::SetProperties { id, properties: NodeProperties { opacity: Some(0.5), ..Default::default() } }],
            },
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::PermissionDenied);
        assert_eq!(doc.get(id).unwrap().opacity, 1.);
    }
    #[test]
    fn creates_validated_board_node_kinds_and_payloads() {
        let (mut doc, _) = fixture();
        doc.kind = DocumentKind::Board;
        let page = doc.pages[0];
        let mut history = History::default();
        let revision = doc.revision;
        let result = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![
                    Operation::CreateNode {
                        temp_id: "$new1".into(),
                        parent: NodeRef::Id(page),
                        index: None,
                        kind: "sticky".into(),
                        properties: NodeProperties { characters: Some("Idea".into()), width: Some(140.0), height: Some(120.0), ..Default::default() },
                        board: Some(BoardNodeProperties {
                            color: Some(Color::rgb(255, 240, 120)),
                            author: Some("Ada".into()),
                            show_author: Some(true),
                            ..Default::default()
                        }),
                    },
                    Operation::CreateNode {
                        temp_id: "$new2".into(),
                        parent: NodeRef::Id(page),
                        index: None,
                        kind: "shape_with_text".into(),
                        properties: NodeProperties { characters: Some("Decision".into()), width: Some(100.0), height: Some(80.0), ..Default::default() },
                        board: Some(BoardNodeProperties { shape: Some(ShapeKind::Diamond), ..Default::default() }),
                    },
                    Operation::CreateNode {
                        temp_id: "$new3".into(),
                        parent: NodeRef::Id(page),
                        index: None,
                        kind: "stamp".into(),
                        properties: NodeProperties { width: Some(32.0), height: Some(32.0), ..Default::default() },
                        board: Some(BoardNodeProperties { stamp_kind: Some("heart".into()), ..Default::default() }),
                    },
                ],
            },
        )
        .unwrap();
        let sticky = doc.get(result.temp_ids["$new1"]).unwrap();
        assert!(matches!(&sticky.kind,NodeKind::Sticky(p) if p.text.content=="Idea" && p.author.as_deref()==Some("Ada") && p.show_author));
        assert!(
            matches!(&doc.get(result.temp_ids["$new2"]).unwrap().kind,NodeKind::ShapeWithText(p) if p.shape==ShapeKind::Diamond && p.text.content=="Decision")
        );
        assert!(matches!(&doc.get(result.temp_ids["$new3"]).unwrap().kind,NodeKind::Stamp{kind} if kind=="heart"));
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.get(page).unwrap().children.len(), 1);
    }

    #[test]
    fn board_node_creation_rejects_wrong_document_and_payloads_atomically() {
        let (mut doc, _) = fixture();
        let page = doc.pages[0];
        let mut history = History::default();
        let before = doc.to_json().unwrap();
        let revision = doc.revision;
        let error = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![Operation::CreateNode {
                    temp_id: "$new1".into(),
                    parent: NodeRef::Id(page),
                    index: None,
                    kind: "sticky".into(),
                    properties: NodeProperties::default(),
                    board: None,
                }],
            },
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidOperation);
        assert_eq!(doc.to_json().unwrap(), before);
        doc.kind = DocumentKind::Board;
        let before = doc.to_json().unwrap();
        let revision = doc.revision;
        let error = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![Operation::CreateNode {
                    temp_id: "$new1".into(),
                    parent: NodeRef::Id(page),
                    index: None,
                    kind: "stamp".into(),
                    properties: NodeProperties { width: Some(30.0), height: Some(20.0), ..Default::default() },
                    board: Some(BoardNodeProperties { stamp_kind: Some("made_up".into()), ..Default::default() }),
                }],
            },
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidOperation);
        assert_eq!(doc.to_json().unwrap(), before);
        assert!(!history.can_undo());
    }
    #[test]
    fn unchanged_properties_do_not_advance_revision_or_history() {
        let (mut doc, id) = fixture();
        let mut hist = History::default();
        let summary = apply_ops(
            &mut doc,
            &mut hist,
            ApplyRequest {
                expected_revision: 0,
                dry_run: false,
                ops: vec![Operation::SetProperties { id, properties: NodeProperties { opacity: Some(1.), ..Default::default() } }],
            },
        )
        .unwrap();
        assert_eq!(summary.revision, 0);
        assert!(summary.changes.is_empty());
        assert!(!hist.can_undo());
    }
    #[test]
    fn typed_comment_workflow_is_dry_runnable_atomic_and_undoable() {
        let (mut doc, node) = fixture();
        let page = doc.pages[0];
        let mut history = History::default();
        let ops = vec![
            Operation::AddComment {
                thread_id: "thread-a".into(),
                message_id: "message-a".into(),
                anchor: CommentAnchor { page, node: Some(node), offset: Vec2::new(3.0, 4.0) },
                author: "Ada".into(),
                text: "Review this shape".into(),
                created: "2026-05-04T10:00:00Z".into(),
            },
            Operation::ReplyComment {
                thread_id: "thread-a".into(),
                message_id: "message-b".into(),
                author: "Lin".into(),
                text: "Agreed".into(),
                created: "2026-05-04T10:01:00Z".into(),
            },
            Operation::ResolveComment { thread_id: "thread-a".into(), resolved: true },
            Operation::ResolveComment { thread_id: "thread-a".into(), resolved: false },
        ];
        let revision = doc.revision;
        let preview = apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: true, ops: ops.clone() }).unwrap();
        assert!(preview.dry_run);
        assert!(doc.comments.is_empty());
        let revision = doc.revision;
        apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: false, ops }).unwrap();
        assert_eq!(doc.comments.len(), 1);
        assert_eq!(doc.comments[0].messages.len(), 2);
        assert!(!doc.comments[0].resolved);
        history.undo(&mut doc).unwrap();
        assert!(doc.comments.is_empty());
    }

    #[test]
    fn typed_comment_validation_failure_leaves_batch_and_history_unchanged() {
        let (mut doc, node) = fixture();
        let page = doc.pages[0];
        let before = doc.to_json().unwrap();
        let mut history = History::default();
        let revision = doc.revision;
        let error = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![
                    Operation::AddComment {
                        thread_id: "thread-a".into(),
                        message_id: "message-a".into(),
                        anchor: CommentAnchor { page, node: Some(node), offset: Vec2::ZERO },
                        author: "Ada".into(),
                        text: "valid first".into(),
                        created: "2026-05-04T10:00:00Z".into(),
                    },
                    Operation::ReplyComment {
                        thread_id: "thread-a".into(),
                        message_id: "message-b".into(),
                        author: "Lin".into(),
                        text: "bad timestamp".into(),
                        created: "not-a-timestamp".into(),
                    },
                ],
            },
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidOperation);
        assert_eq!(doc.to_json().unwrap(), before);
        assert!(!history.can_undo());
    }

    #[test]
    fn vector_operations_use_temporary_ids_and_rollback_failed_batches() {
        let (mut doc, first) = fixture();
        let page = doc.pages[0];
        let second = doc.alloc_id();
        doc.insert(page, Node::rectangle(second, Rect::new(10., 0., 20., 20.))).unwrap();
        let before = doc.to_json().unwrap();
        let before_nodes = doc.nodes.clone();
        let mut history = History::default();
        let revision = doc.revision;
        let error = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![
                    Operation::BooleanOp { temp_id: "$new1".into(), ids: vec![NodeRef::Id(first), NodeRef::Id(second)], boolean_op: BooleanOp::Union },
                    Operation::Flatten { temp_id: "$new2".into(), ids: vec![NodeRef::Temp("$new9".into())] },
                ],
            },
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidOperation);
        assert_eq!(doc.to_json().unwrap(), before);
        assert!(!history.can_undo());

        let revision = doc.revision;
        let result = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![
                    Operation::BooleanOp { temp_id: "$new1".into(), ids: vec![NodeRef::Id(first), NodeRef::Id(second)], boolean_op: BooleanOp::Union },
                    Operation::Flatten { temp_id: "$new2".into(), ids: vec![NodeRef::Temp("$new1".into())] },
                ],
            },
        )
        .unwrap();
        let flattened = result.temp_ids["$new2"];
        assert!(matches!(doc.get(flattened).unwrap().kind, NodeKind::Vector { .. }));
        assert!(doc.get(first).is_none() && doc.get(second).is_none());
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.nodes, before_nodes);
    }

    #[test]
    fn typed_vector_network_and_outline_stroke_are_undoable() {
        let (mut doc, id) = fixture();
        let page = doc.pages[0];
        let p = |x, y| Vec2::new(x, y);
        let path = Path {
            cmds: vec![PathCmd::MoveTo(p(0., 0.)), PathCmd::LineTo(p(20., 0.)), PathCmd::LineTo(p(20., 20.)), PathCmd::LineTo(p(0., 20.)), PathCmd::Close],
            fill_rule: FillRule::NonZero,
        };
        let network = VectorNetwork::from_paths(&[path]);
        {
            let node = doc.get_mut(id).unwrap();
            node.kind = NodeKind::Vector { paths: network.to_paths(), network: Some(network.clone()) };
            node.stroke.weight = 2.;
            node.stroke.paints = vec![Paint::solid(Color::rgb(0, 0, 0))];
            node.fills.clear();
        }
        let mut history = History::default();
        let revision = doc.revision;
        apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![Operation::SetVectorNetwork {
                    id: NodeRef::Id(id),
                    network: VectorNetwork::from_paths(&[Path {
                        cmds: vec![
                            PathCmd::MoveTo(p(0., 0.)),
                            PathCmd::LineTo(p(30., 0.)),
                            PathCmd::LineTo(p(30., 30.)),
                            PathCmd::LineTo(p(0., 30.)),
                            PathCmd::Close,
                        ],
                        fill_rule: FillRule::NonZero,
                    }]),
                }],
            },
        )
        .unwrap();
        let after_network_nodes = doc.nodes.clone();
        let revision = doc.revision;
        let result = apply_ops(
            &mut doc,
            &mut history,
            ApplyRequest { expected_revision: revision, dry_run: false, ops: vec![Operation::OutlineStroke { temp_id: "$new1".into(), id: NodeRef::Id(id) }] },
        )
        .unwrap();
        let outlined = result.temp_ids["$new1"];
        assert!(matches!(doc.get(outlined).unwrap().kind, NodeKind::Vector { .. }));
        assert!(doc.get(id).is_none());
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.nodes, after_network_nodes);
        history.undo(&mut doc).unwrap();
        assert!(matches!(doc.get(id).unwrap().kind, NodeKind::Vector { .. }));
        assert_eq!(page, doc.pages[0]);
    }
}

#[cfg(test)]
mod plain_text_replacement_tests {
    use super::*;
    #[test]
    fn replacement_clears_obsolete_ranges_and_undo_restores_them() {
        let mut doc = Document::new("text");
        let page = doc.add_page("P");
        let id = doc.alloc_id();
        let mut node = Node::text(id, Rect::new(0.0, 0.0, 100.0, 50.0), "item", TextStyle::default());
        if let NodeKind::Text(t) = &mut node.kind {
            t.lists.push(ListRange { start: 0, end: 4, kind: ListKind::Bullet, level: 0 });
        }
        doc.insert(page, node).unwrap();
        let original = doc.nodes.clone();
        let mut history = History::default();
        for text in ["item", "x"] {
            let revision = doc.revision;
            apply_ops(
                &mut doc,
                &mut history,
                ApplyRequest {
                    expected_revision: revision,
                    dry_run: false,
                    ops: vec![Operation::SetProperties { id, properties: NodeProperties { characters: Some(text.into()), ..Default::default() } }],
                },
            )
            .unwrap();
            let lists = &doc.get(id).unwrap().kind.any_text().unwrap().lists;
            assert_eq!(lists.is_empty(), text == "x");
        }
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.nodes, original);
        assert!(!history.can_undo());
    }
}
