//! Bounded inspection using the editor's document geometry and CPU renderer.
use base64::Engine as _;
use desigma_doc::{AutoLayout, Color, Document, LayoutAlign, LayoutDirection, NodeId, NodeKind, PaintKind, Sizing, Style, StyleProperty, Vec2};
use serde::Deserialize;
use serde_json::{json, Value};

fn world_bounds(doc: &Document, id: NodeId) -> Option<(f32, f32, f32, f32)> {
    let n = doc.get(id)?;
    let m = doc.world_transform(id);
    let corners = [Vec2::ZERO, Vec2::new(n.size.x, 0.0), n.size, Vec2::new(0.0, n.size.y)].map(|p| m.apply(p));
    let left = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let top = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let right = corners.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
    let bottom = corners.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
    (left.is_finite() && top.is_finite() && right.is_finite() && bottom.is_finite()).then_some((left, top, right, bottom))
}

fn solid_color(fills: &[desigma_doc::Paint]) -> Option<Color> {
    fills.iter().find_map(|paint| {
        if !paint.visible || paint.opacity < 0.99 {
            return None;
        }
        match &paint.kind {
            PaintKind::Solid(color) if color.a >= 0.99 => Some(*color),
            _ => None,
        }
    })
}

fn luminance(color: Color) -> f32 {
    fn linear(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

fn contrast(a: Color, b: Color) -> f32 {
    let (light, dark) = if luminance(a) >= luminance(b) { (luminance(a), luminance(b)) } else { (luminance(b), luminance(a)) };
    (light + 0.05) / (dark + 0.05)
}

fn style_apply_suggestion(doc: &Document, id: NodeId, key: &str, property: StyleProperty) -> Value {
    json!({"expected_revision":doc.revision,"dry_run":true,"ops":[{"op":"apply_style","ids":[id],"key":key,"property":property}]})
}

fn style_property_equal(_doc: &Document, node: &desigma_doc::Node, prop: &str, style: &Style) -> Option<bool> {
    match (prop, style) {
        ("fill", Style::Fill(paints)) if node.kind.any_text().is_none() => Some(&node.fills == paints),
        ("stroke", Style::Fill(paints)) => Some(&node.stroke.paints == paints),
        ("text", Style::Text(style)) => node.kind.any_text().map(|text| if text.runs.is_empty() { text.style == *style } else { false }),
        ("effect", Style::Effect(effects)) => Some(&node.effects == effects),
        ("grid", Style::Grid(grids)) => node.kind.frame_props().map(|frame| &frame.grids == grids),
        ("fill", Style::Fill(paints)) => node.kind.any_text().map(|text| &text.style.fills == paints),
        _ => None,
    }
}

fn has_variable_binding(node: &desigma_doc::Node, prop: &str) -> bool {
    let prefix = match prop {
        "fill" => "fills/",
        "stroke" => "strokes/",
        "text" => "font",
        "effect" => "effects/",
        "grid" => "grids/",
        _ => return true,
    };
    node.variables.bindings.keys().any(|key| key == prefix || key.starts_with(prefix))
}

#[cfg(test)]
mod lint_tests {
    use super::*;
    use desigma_doc::Node;

    fn findings(doc: &Document) -> Vec<Value> {
        call(doc, "desigma_get_diagnostics", json!({})).unwrap()["findings"].as_array().unwrap().clone()
    }
    fn by_code<'a>(all: &'a [Value], code: &str) -> Vec<&'a Value> {
        all.iter().filter(|f| f["code"] == code).collect()
    }
    fn assert_apply_suggestion_is_schema_valid(doc: &Document, fix: &Value) {
        let apply = crate::contract::tools().into_iter().find(|tool| tool["name"] == "desigma_apply_ops").unwrap();
        crate::contract::validate(fix, &apply["inputSchema"]).unwrap();
        let request: desigma_engine::ApplyRequest = serde_json::from_value(fix.clone()).unwrap();
        assert!(request.dry_run);
        let mut trial = doc.clone();
        let before = trial.to_json().unwrap();
        desigma_engine::apply_ops(&mut trial, &mut desigma_doc::History::default(), request).unwrap();
        assert_eq!(trial.to_json().unwrap(), before);
    }

    #[test]
    fn missing_auto_layout_hint_requires_regular_children_and_has_dry_run_fix() {
        let mut doc = Document::new("lint");
        let page = doc.add_page("Page");
        let frame_id = doc.alloc_id();
        let frame = doc.insert(page, Node::frame(frame_id, desigma_doc::Rect::new(0., 0., 80., 30.))).unwrap();
        let mut child_ids = Vec::new();
        for x in [5., 20., 35.] {
            let id = doc.alloc_id();
            doc.insert(frame, Node::rectangle(id, desigma_doc::Rect::new(x, 5., 10., 10.))).unwrap();
            child_ids.push(id);
        }
        let all = findings(&doc);
        let hint = by_code(&all, "possible_missing_auto_layout");
        assert_eq!(hint.len(), 1);
        let fix = &hint[0]["suggested_fix"];
        assert_eq!(fix["expected_revision"], doc.revision);
        assert_eq!(fix["dry_run"], true);
        assert_eq!(fix["ops"][0]["op"], "set_auto_layout");
        assert_eq!(fix["ops"][0]["auto_layout"]["direction"], "Horizontal");
        assert_eq!(fix["ops"][0]["auto_layout"]["item_spacing"], 5.0);
        assert_apply_suggestion_is_schema_valid(&doc, fix);

        doc.get_mut(frame).unwrap().kind.frame_props_mut().unwrap().auto_layout = Some(AutoLayout::default());
        assert!(by_code(&findings(&doc), "possible_missing_auto_layout").is_empty());
        doc.get_mut(frame).unwrap().kind.frame_props_mut().unwrap().auto_layout = None;
        doc.get_mut(child_ids[2]).unwrap().transform.tx = 38.0;
        assert!(by_code(&findings(&doc), "possible_missing_auto_layout").is_empty());
    }

    #[test]
    fn shared_style_lints_use_native_style_data_and_only_suggest_dry_runs() {
        let mut doc = Document::new("styles");
        let page = doc.add_page("Page");
        let id = doc.alloc_id();
        let node = Node::rectangle(id, desigma_doc::Rect::new(0., 0., 20., 20.));
        let shared = node.fills.clone();
        doc.insert(page, node).unwrap();
        doc.styles.insert("brand.primary".into(), Style::Fill(shared));

        let all = findings(&doc);
        let linked = by_code(&all, "unlinked_shared_style");
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0]["style_id"], "brand.primary");
        assert_eq!(linked[0]["suggested_fix"]["expected_revision"], doc.revision);
        assert_eq!(linked[0]["suggested_fix"]["dry_run"], true);
        assert_eq!(linked[0]["suggested_fix"]["ops"][0]["op"], "apply_style");
        assert_apply_suggestion_is_schema_valid(&doc, &linked[0]["suggested_fix"]);

        doc.get_mut(id).unwrap().style_refs.insert("fill".into(), "brand.primary".into());
        doc.get_mut(id).unwrap().fills.clear();
        let all = findings(&doc);
        let diverged = by_code(&all, "shared_style_diverged");
        assert_eq!(diverged.len(), 1);
        assert_eq!(diverged[0]["suggested_fix"]["ops"][0]["op"], "apply_style");
        assert_eq!(diverged[0]["suggested_fix"]["dry_run"], true);
        assert_apply_suggestion_is_schema_valid(&doc, &diverged[0]["suggested_fix"]);
    }

    #[test]
    fn contrast_reports_aa_and_aaa_and_suggests_a_valid_dry_run_fix() {
        let mut doc = Document::new("contrast");
        let page = doc.add_page("Page");
        let frame_id = doc.alloc_id();
        let frame = doc.insert(page, Node::frame(frame_id, desigma_doc::Rect::new(0., 0., 100., 50.))).unwrap();
        let style =
            desigma_doc::TextStyle { font_size: 12.0, fills: vec![desigma_doc::Paint::solid(desigma_doc::Color::rgb(0x80, 0x80, 0x80))], ..Default::default() };
        let id = doc.alloc_id();
        doc.insert(frame, Node::text(id, desigma_doc::Rect::new(0., 0., 80., 20.), "Text", style)).unwrap();

        let all = findings(&doc);
        let low = by_code(&all, "low_text_contrast");
        assert_eq!(low.len(), 1);
        assert_eq!(low[0]["wcag_aa_pass"], false);
        assert_eq!(low[0]["wcag_aaa_pass"], false);
        assert_eq!(low[0]["wcag_aaa_required_ratio"], 7.0);
        assert_apply_suggestion_is_schema_valid(&doc, &low[0]["suggested_fix"]);

        // Mid-gray text passes AA on white, but remains an AAA advisory.
        doc.get_mut(id).unwrap().kind.any_text_mut().unwrap().style.fills = vec![desigma_doc::Paint::solid(desigma_doc::Color::rgb(0x66, 0x66, 0x66))];
        let all = findings(&doc);
        let aaa = by_code(&all, "low_text_contrast");
        assert_eq!(aaa.len(), 1);
        assert_eq!(aaa[0]["wcag_aa_pass"], true);
        assert_eq!(aaa[0]["wcag_aaa_pass"], false);
        assert_eq!(aaa[0]["severity"], "info");
        assert_apply_suggestion_is_schema_valid(&doc, &aaa[0]["suggested_fix"]);
    }

    #[test]
    fn sibling_text_color_and_spacing_outliers_are_reported_as_review_hints() {
        let mut doc = Document::new("consistency");
        let page = doc.add_page("Page");
        let frame_id = doc.alloc_id();
        let frame = doc.insert(page, Node::frame(frame_id, desigma_doc::Rect::new(0., 0., 150., 40.))).unwrap();
        for (x, color) in [(0., desigma_doc::Color::BLACK), (22., desigma_doc::Color::BLACK), (52., desigma_doc::Color::rgb(0xff, 0, 0))] {
            let style = desigma_doc::TextStyle { fills: vec![desigma_doc::Paint::solid(color)], ..Default::default() };
            let id = doc.alloc_id();
            doc.insert(frame, Node::text(id, desigma_doc::Rect::new(x, 0., 20., 20.), "Label", style)).unwrap();
        }
        let all = findings(&doc);
        assert_eq!(by_code(&all, "inconsistent_sibling_text_color").len(), 1);
        let spacing = by_code(&all, "inconsistent_sibling_spacing");
        assert_eq!(spacing.len(), 1);
        assert_eq!(spacing[0]["axis"], "horizontal");
        assert!(spacing[0]["suggested_fix"].is_null());
    }
}

fn infer_missing_auto_layout(doc: &Document, frame_id: NodeId) -> Option<AutoLayout> {
    let frame = doc.get(frame_id)?;
    if !matches!(frame.kind, NodeKind::Frame(_)) || frame.kind.frame_props()?.auto_layout.is_some() || frame.children.len() > 64 {
        return None;
    }
    let children: Vec<_> = frame.children.iter().filter_map(|id| doc.get(*id)).filter(|n| n.visible).collect();
    if !(3..=32).contains(&children.len())
        || children.iter().any(|n| {
            n.layout_child.absolute
                || n.layout_child.grow != 0.0
                || n.layout_child.horizontal != Sizing::Fixed
                || n.layout_child.vertical != Sizing::Fixed
                || n.transform.a != 1.0
                || n.transform.d != 1.0
                || n.transform.b != 0.0
                || n.transform.c != 0.0
                || n.variables.bindings.keys().any(|k| matches!(k.as_str(), "x" | "y" | "width" | "height"))
        })
        || frame.variables.bindings.keys().any(|k| matches!(k.as_str(), "itemSpacing" | "paddingTop" | "paddingRight" | "paddingBottom" | "paddingLeft"))
    {
        return None;
    }
    let close = |a: f32, b: f32| (a - b).abs() <= 0.35;
    let candidate = |direction: LayoutDirection| -> Option<AutoLayout> {
        let horizontal = direction == LayoutDirection::Horizontal;
        let cross_start = if horizontal { children[0].transform.ty } else { children[0].transform.tx };
        if children.iter().any(|n| !close(if horizontal { n.transform.ty } else { n.transform.tx }, cross_start)) {
            return None;
        }
        let positions: Vec<(f32, f32, f32)> =
            children.iter().map(|n| if horizontal { (n.transform.tx, n.size.x, n.size.y) } else { (n.transform.ty, n.size.y, n.size.x) }).collect();
        if positions.iter().any(|(p, size, cross)| !p.is_finite() || !size.is_finite() || !cross.is_finite() || *size <= 0.0 || *cross <= 0.0) {
            return None;
        }
        let mut gaps = Vec::with_capacity(positions.len() - 1);
        for pair in positions.windows(2) {
            let gap = pair[1].0 - (pair[0].0 + pair[0].1);
            if !gap.is_finite() || gap < -0.1 {
                return None;
            }
            gaps.push(gap.max(0.0));
        }
        let spacing = gaps.iter().sum::<f32>() / gaps.len() as f32;
        if gaps.iter().any(|gap| !close(*gap, spacing)) {
            return None;
        }
        let max_cross = cross_start + positions.iter().map(|(_, _, cross)| *cross).fold(0.0, f32::max);
        let first = positions[0].0;
        let end = positions.last().map(|(p, size, _)| p + size)?;
        let (top, right, bottom, left) = if horizontal {
            (cross_start, frame.size.x - end, frame.size.y - max_cross, first)
        } else {
            (first, frame.size.x - max_cross, frame.size.y - end, cross_start)
        };
        let padding = [top, right, bottom, left];
        if padding.iter().any(|p| !p.is_finite() || *p < -0.1) {
            return None;
        }
        Some(AutoLayout {
            direction,
            wrap: false,
            item_spacing: spacing,
            counter_spacing: 0.0,
            padding: padding.map(|p| p.max(0.0)),
            primary_align: LayoutAlign::Start,
            counter_align: LayoutAlign::Start,
            primary_sizing: Sizing::Fixed,
            counter_sizing: Sizing::Fixed,
            stroke_included: false,
            reverse_z: false,
        })
    };
    let horizontal = candidate(LayoutDirection::Horizontal);
    let vertical = candidate(LayoutDirection::Vertical);
    match (horizontal, vertical) {
        (Some(layout), None) | (None, Some(layout)) => Some(layout),
        _ => None,
    }
}

fn nearest_background(doc: &Document, mut parent: Option<NodeId>) -> Option<Color> {
    let mut visited = std::collections::BTreeSet::new();
    while let Some(id) = parent {
        if !visited.insert(id) || visited.len() > 256 {
            return None;
        }
        let node = doc.get(id)?;
        if let Some(color) = solid_color(&node.fills) {
            return Some(color);
        }
        parent = node.parent;
    }
    Some(Color::WHITE)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InspectArgs {
    node_id: Option<u64>,
    #[serde(default = "limit")]
    limit: usize,
}
fn limit() -> usize {
    500
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderArgs {
    node_id: u64,
    #[serde(default = "scale")]
    scale: f32,
    #[serde(default)]
    format: RenderFormat,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegionArgs {
    page_id: u64,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    #[serde(default = "scale")]
    scale: f32,
    #[serde(default)]
    format: RenderFormat,
}
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RenderFormat {
    #[default]
    Png,
    Jpeg,
}

impl RenderFormat {
    fn encode(self, png: &[u8]) -> Result<(Vec<u8>, &'static str), String> {
        match self {
            Self::Png => Ok((png.to_vec(), "image/png")),
            Self::Jpeg => {
                let rgba = image::load_from_memory(png).map_err(|e| format!("operation_conflict: cannot decode render for JPEG: {e}"))?.to_rgba8();
                // JPEG has no alpha channel; composite transparent pixels onto white.
                let rgb: Vec<u8> = rgba
                    .pixels()
                    .flat_map(|pixel| {
                        let alpha = u32::from(pixel[3]);
                        (0..3).map(move |channel| ((u32::from(pixel[channel]) * alpha + 255 * (255 - alpha) + 127) / 255) as u8)
                    })
                    .collect();
                let mut bytes = Vec::new();
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 82)
                    .encode(&rgb, rgba.width(), rgba.height(), image::ExtendedColorType::Rgb8)
                    .map_err(|e| format!("operation_conflict: cannot encode JPEG render: {e}"))?;
                Ok((bytes, "image/jpeg"))
            }
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiffArgs {
    node_a: u64,
    node_b: u64,
    #[serde(default = "scale")]
    scale: f32,
    #[serde(default)]
    tolerance: u8,
}
fn scale() -> f32 {
    1.0
}

pub fn call(doc: &Document, name: &str, args: Value) -> Result<Value, String> {
    if name == "desigma_list_fonts" {
        if !args.as_object().is_some_and(|object| object.is_empty()) {
            return Err("invalid_operation: list_fonts takes no arguments".into());
        }
        let faces: Vec<_> = desigma_render::available_faces()
            .into_iter()
            .map(|(family, weight, italic, bundled)| json!({"family":family,"weight":weight,"italic":italic,"bundled":bundled}))
            .collect();
        return Ok(json!({"revision":doc.revision,"faces":faces}));
    }
    if name == "desigma_render_diff" {
        let a: DiffArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        if !a.scale.is_finite() || a.scale <= 0.0 || a.scale > 4.0 {
            return Err("invalid_operation: scale must be in (0,4]".into());
        }
        for id in [a.node_a, a.node_b] {
            let node = doc.get(NodeId(id)).ok_or("missing_node: comparison node not found")?;
            let w = (node.size.x * a.scale).ceil();
            let h = (node.size.y * a.scale).ceil();
            if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 || w > 4096.0 || h > 4096.0 || w * h > 8_388_608.0 {
                return Err("invalid_operation: comparison node dimensions exceed render limits".into());
            }
        }
        let resolved = desigma_doc::apply_bound_variables(doc);
        let left = desigma_render::render_node(&resolved, NodeId(a.node_a), a.scale).ok_or("operation_conflict: cannot render first node")?;
        let right = desigma_render::render_node(&resolved, NodeId(a.node_b), a.scale).ok_or("operation_conflict: cannot render second node")?;
        let width = left.width().max(right.width());
        let height = left.height().max(right.height());
        if u64::from(width) * u64::from(height) > 8_388_608 {
            return Err("invalid_operation: comparison exceeds 8 megapixels".into());
        }
        let mut diff = tiny_skia::Pixmap::new(width, height).ok_or("operation_conflict: cannot allocate diff")?;
        let mut changed = 0u64;
        for y in 0..height {
            for x in 0..width {
                let lhs = left.pixel(x, y).map(|p| p.demultiply()).unwrap_or(tiny_skia::ColorU8::from_rgba(0, 0, 0, 0));
                let rhs = right.pixel(x, y).map(|p| p.demultiply()).unwrap_or(tiny_skia::ColorU8::from_rgba(0, 0, 0, 0));
                let channels = [(lhs.red(), rhs.red()), (lhs.green(), rhs.green()), (lhs.blue(), rhs.blue()), (lhs.alpha(), rhs.alpha())];
                if channels.iter().any(|(channel_a, channel_b)| channel_a.abs_diff(*channel_b) > a.tolerance) {
                    changed += 1;
                    diff.pixels_mut()[(y * width + x) as usize] = tiny_skia::PremultipliedColorU8::from_rgba(255, 0, 80, 255).unwrap();
                }
            }
        }
        let bytes = diff.encode_png().map_err(|e| format!("operation_conflict: cannot encode diff: {e}"))?;
        return Ok(
            json!({"revision":doc.revision,"node_a":a.node_a,"node_b":a.node_b,"changed_pixels":changed,"changed_pixel_ratio":changed as f64 / f64::from(width * height),"width":width,"height":height,"mime_type":"image/png","data":base64::engine::general_purpose::STANDARD.encode(bytes)}),
        );
    }
    if name == "desigma_render_region" {
        let a: RegionArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let page = NodeId(a.page_id);
        if !doc.pages.contains(&page) {
            return Err("missing_node: page not found".into());
        }
        if ![a.x, a.y, a.width, a.height, a.scale].iter().all(|v| v.is_finite()) || a.width <= 0.0 || a.height <= 0.0 || a.scale <= 0.0 || a.scale > 4.0 {
            return Err("invalid_operation: region and scale must be finite, positive, and scale at most 4".into());
        }
        let width = (a.width * a.scale).ceil();
        let height = (a.height * a.scale).ceil();
        if width > 4096.0 || height > 4096.0 || width * height > 8_388_608.0 {
            return Err("invalid_operation: render must be at most 4096 pixels per side and 8 megapixels".into());
        }
        let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32).ok_or("operation_conflict: cannot allocate render")?;
        let resolved = desigma_doc::apply_bound_variables(doc);
        desigma_render::render_page(&resolved, page, &desigma_render::Viewport { offset: Vec2::new(a.x, a.y), zoom: a.scale, dpr: 1.0 }, &mut pixmap);
        let png = pixmap.encode_png().map_err(|e| format!("operation_conflict: cannot encode render: {e}"))?;
        let (bytes, mime_type) = a.format.encode(&png)?;
        return Ok(
            json!({"page_id":page,"revision":doc.revision,"mime_type":mime_type,"data":base64::engine::general_purpose::STANDARD.encode(bytes),"width":width as u32,"height":height as u32}),
        );
    }
    if name == "desigma_render_node" {
        let a: RenderArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let id = NodeId(a.node_id);
        doc.get(id).ok_or("missing_node: render target not found")?;
        if !a.scale.is_finite() || a.scale <= 0.0 || a.scale > 4.0 {
            return Err("invalid_operation: scale must be in (0,4]".into());
        }
        let resolved = desigma_doc::apply_bound_variables(doc);
        let bounds = desigma_export::render_bounds(&resolved, id);
        let w = (bounds.w * a.scale).ceil();
        let h = (bounds.h * a.scale).ceil();
        if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 || w > 4096.0 || h > 4096.0 || w * h > 8_388_608.0 {
            return Err("invalid_operation: render must have positive dimensions, at most 4096 per side and 8 megapixels".into());
        }
        let png = desigma_export::export_png(&resolved, id, a.scale);
        if png.is_empty() {
            return Err("operation_conflict: renderer produced no image".into());
        }
        let (bytes, mime_type) = a.format.encode(&png)?;
        return Ok(
            json!({"node_id":id,"revision":doc.revision,"mime_type":mime_type,"data":base64::engine::general_purpose::STANDARD.encode(bytes),"width":w,"height":h}),
        );
    }
    let resolved;
    let doc = if name == "desigma_get_diagnostics" {
        resolved = desigma_doc::apply_bound_variables(doc);
        &resolved
    } else {
        doc
    };
    let a: InspectArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
    if a.limit == 0 || a.limit > 1000 {
        return Err("invalid_operation: limit must be between 1 and 1000".into());
    }
    let roots = if let Some(id) = a.node_id {
        doc.get(NodeId(id)).ok_or("missing_node: inspection target not found")?;
        vec![NodeId(id)]
    } else {
        doc.pages.clone()
    };
    let mut pending: Vec<_> = roots.into_iter().rev().map(|id| (id, 0usize)).collect();
    let mut nodes = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    while let Some((id, depth)) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(n) = doc.get(id) else { continue };
        nodes.push((n, depth));
        pending.extend(n.children.iter().rev().map(|id| (*id, depth + 1)));
        if nodes.len() >= a.limit {
            break;
        }
    }
    let truncated = !pending.is_empty();
    match name {
        "desigma_get_outline" => Ok(
            json!({"revision":doc.revision,"nodes":nodes.iter().map(|(n,depth)|json!({"node_id":n.id,"parent_id":n.parent,"name":n.name,"kind":n.kind.default_name(),"depth":depth,"child_count":n.children.len()})).collect::<Vec<_>>(),"truncated":truncated}),
        ),
        "desigma_get_layout" => Ok(json!({"revision":doc.revision,"nodes":nodes.iter().map(|(n,_)| {
            let m=doc.world_transform(n.id);
            let corners=[Vec2::ZERO,Vec2::new(n.size.x,0.0),n.size,Vec2::new(0.0,n.size.y)].map(|p|m.apply(p));
            let minx=corners.iter().map(|p|p.x).fold(f32::INFINITY,f32::min);
            let miny=corners.iter().map(|p|p.y).fold(f32::INFINITY,f32::min);
            let maxx=corners.iter().map(|p|p.x).fold(f32::NEG_INFINITY,f32::max);
            let maxy=corners.iter().map(|p|p.y).fold(f32::NEG_INFINITY,f32::max);
            json!({"node_id":n.id,"parent_id":n.parent,"local_bounds":n.local_bounds(),"world_bounds":{"x":minx,"y":miny,"w":maxx-minx,"h":maxy-miny},"transform":m,"layout":n.kind.frame_props().and_then(|f|f.auto_layout.as_ref())})
        }).collect::<Vec<_>>(),"truncated":truncated})),
        "desigma_get_diagnostics" => {
            let mut findings = Vec::new();
            let families = desigma_render::available_families();
            // Flag only substantial overlaps among visible siblings. This is a
            // review hint: layered designs commonly overlap intentionally.
            let mut overlap_count = 0usize;
            for (parent, _) in &nodes {
                let children: Vec<_> = parent.children.iter().filter_map(|id| doc.get(*id)).filter(|n| n.visible).collect();
                if children.len() > 128 {
                    continue;
                }
                'pairs: for i in 0..children.len() {
                    for j in (i + 1)..children.len() {
                        let (Some(a), Some(b)) = (world_bounds(doc, children[i].id), world_bounds(doc, children[j].id)) else {
                            continue;
                        };
                        let w = (a.2.min(b.2) - a.0.max(b.0)).max(0.0);
                        let h = (a.3.min(b.3) - a.1.max(b.1)).max(0.0);
                        let intersection = w * h;
                        let smaller = ((a.2 - a.0).max(0.0) * (a.3 - a.1).max(0.0)).min((b.2 - b.0).max(0.0) * (b.3 - b.1).max(0.0));
                        if smaller > 0.0 && intersection / smaller >= 0.25 {
                            findings.push(json!({"node_id":children[j].id,"other_node_id":children[i].id,"parent_id":parent.id,"code":"overlapping_siblings","severity":"info","overlap_ratio":intersection / smaller,"message":"Visible siblings overlap substantially; check whether the layering is intentional","suggested_fix":null}));
                            overlap_count += 1;
                            if overlap_count >= 200 {
                                break 'pairs;
                            }
                        }
                    }
                }
                if overlap_count >= 200 {
                    break;
                }

                // A small, deliberately conservative consistency pass: report
                // only obvious color outliers among sibling text layers with
                // matching typography, and only spacing drift in a clear row
                // or column of at least three siblings.
                let mut text_groups: std::collections::BTreeMap<(String, u16, u32), Vec<(NodeId, Color)>> = std::collections::BTreeMap::new();
                for child in &children {
                    if let Some(text) = child.kind.any_text().filter(|text| text.runs.is_empty()) {
                        if let Some(color) = solid_color(&text.style.fills) {
                            text_groups
                                .entry((text.style.font_family.to_ascii_lowercase(), text.style.font_weight, text.style.font_size.to_bits()))
                                .or_default()
                                .push((child.id, color));
                        }
                    }
                }
                for group in text_groups.values().filter(|group| group.len() >= 3) {
                    let mut colors: Vec<(Color, Vec<NodeId>)> = Vec::new();
                    for (id, color) in group {
                        if let Some((_, ids)) = colors.iter_mut().find(|(existing, _)| existing == color) {
                            ids.push(*id);
                        } else {
                            colors.push((*color, vec![*id]));
                        }
                    }
                    if let Some((_, majority)) = colors.iter().max_by_key(|(_, ids)| ids.len()) {
                        if majority.len() >= 2 && majority.len() + 1 == group.len() {
                            if let Some((id, _)) = group.iter().find(|(id, _)| !majority.contains(id)) {
                                findings.push(json!({"node_id":id,"parent_id":parent.id,"code":"inconsistent_sibling_text_color","severity":"info","message":"This text layer uses a different solid color from otherwise matching sibling text","suggested_fix":null}));
                            }
                        }
                    }
                }

                let aligned = |horizontal: bool| -> Option<Vec<(f32, f32, NodeId)>> {
                    let mut items: Vec<_> = children
                        .iter()
                        .filter_map(|n| {
                            n.kind.any_text()?;
                            let b = world_bounds(doc, n.id)?;
                            let (start, end, cross_start, cross_end) = if horizontal { (b.0, b.2, b.1, b.3) } else { (b.1, b.3, b.0, b.2) };
                            Some((start, end, cross_start, cross_end, n.id))
                        })
                        .collect();
                    if items.len() < 3 {
                        return None;
                    }
                    let cross = items[0].2;
                    if items.iter().any(|item| (item.2 - cross).abs() > 0.5 || (item.3 - items[0].3).abs() > 0.5) {
                        return None;
                    }
                    items.sort_by(|a, b| a.0.total_cmp(&b.0));
                    if items.windows(2).any(|pair| pair[1].0 < pair[0].1 - 0.5) {
                        return None;
                    }
                    Some(items.into_iter().map(|(start, end, _, _, id)| (start, end, id)).collect())
                };
                for horizontal in [true, false] {
                    if let Some(items) = aligned(horizontal) {
                        let gaps: Vec<_> = items.windows(2).map(|pair| pair[1].0 - pair[0].1).collect();
                        let min = gaps.iter().copied().fold(f32::INFINITY, f32::min);
                        let max = gaps.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                        if max - min > 2.0 && max <= min.max(0.0) * 3.0 + 8.0 {
                            findings.push(json!({"node_id":items[1].2,"parent_id":parent.id,"code":"inconsistent_sibling_spacing","severity":"info","axis":if horizontal { "horizontal" } else { "vertical" },"gap_range":{"min":min,"max":max},"message":"Aligned sibling text layers have noticeably different gaps","suggested_fix":null}));
                            break;
                        }
                    }
                }
            }
            for (n, _) in &nodes {
                let default = n.kind.default_name();
                let generated_name = n.name == default
                    || n.name
                        .strip_prefix(default)
                        .is_some_and(|suffix| suffix.strip_prefix(' ').is_some_and(|number| !number.is_empty() && number.parse::<u32>().is_ok()));
                if generated_name {
                    findings.push(
                        json!({"node_id":n.id,"code":"default_name","severity":"info","message":"Give this layer a descriptive name","suggested_fix":null}),
                    );
                }
                for (property, style_key) in &n.style_refs {
                    match doc.styles.get(style_key) {
                        None => findings.push(json!({"node_id":n.id,"code":"missing_shared_style","severity":"warning","property":property,"style_id":style_key,"message":"This layer references a shared style that is not present in the document","suggested_fix":null})),
                        Some(style) => match style_property_equal(doc, n, property, style) {
                            None => findings.push(json!({"node_id":n.id,"code":"incompatible_shared_style","severity":"warning","property":property,"style_id":style_key,"message":"The referenced shared style does not apply to this property","suggested_fix":null})),
                            Some(false) => {
                                let property_kind = match property.as_str() { "fill" => Some(StyleProperty::Fill), "stroke" => Some(StyleProperty::Stroke), "text" => Some(StyleProperty::Text), "effect" => Some(StyleProperty::Effect), "grid" => Some(StyleProperty::Grid), _ => None };
                                let suggestion = property_kind.filter(|_| !has_variable_binding(n, property)).map(|kind| style_apply_suggestion(doc, n.id, style_key, kind)).unwrap_or(Value::Null);
                                findings.push(json!({"node_id":n.id,"code":"shared_style_diverged","severity":"info","property":property,"style_id":style_key,"message":"The layer's current property differs from its referenced shared style; this may be an intentional local override","suggested_fix":suggestion}));
                            }
                            Some(true) => {}
                        },
                    }
                }
                for (property, kind) in [
                    ("fill", StyleProperty::Fill),
                    ("stroke", StyleProperty::Stroke),
                    ("text", StyleProperty::Text),
                    ("effect", StyleProperty::Effect),
                    ("grid", StyleProperty::Grid),
                ] {
                    if n.style_refs.contains_key(property) || has_variable_binding(n, property) {
                        continue;
                    }
                    let matching: Vec<_> = doc
                        .styles
                        .iter()
                        .filter_map(|(key, style)| style_property_equal(doc, n, property, style).filter(|equal| *equal).map(|_| key))
                        .collect();
                    if matching.len() == 1 {
                        let style_key = matching[0];
                        findings.push(json!({"node_id":n.id,"code":"unlinked_shared_style","severity":"info","property":property,"style_id":style_key,"message":"This property exactly matches one shared style but is not linked to it","suggested_fix":style_apply_suggestion(doc,n.id,style_key,kind)}));
                    }
                }
                if let Some(auto_layout) = infer_missing_auto_layout(doc, n.id) {
                    let direction = if auto_layout.direction == LayoutDirection::Horizontal { "Horizontal" } else { "Vertical" };
                    let fix = json!({"expected_revision":doc.revision,"dry_run":true,"ops":[{"op":"set_auto_layout","id":n.id,"auto_layout":auto_layout}]});
                    findings.push(json!({"node_id":n.id,"code":"possible_missing_auto_layout","severity":"info","direction":direction,"message":"Children form a regular, fixed-size row or column that may be intended as auto layout","suggested_fix":fix}));
                }
                if let Some(parent_id) = n.parent {
                    if let (Some(parent), Some(child_bounds), Some(parent_bounds)) = (doc.get(parent_id), world_bounds(doc, n.id), world_bounds(doc, parent_id))
                    {
                        if child_bounds.0 < parent_bounds.0 - 0.5
                            || child_bounds.1 < parent_bounds.1 - 0.5
                            || child_bounds.2 > parent_bounds.2 + 0.5
                            || child_bounds.3 > parent_bounds.3 + 0.5
                        {
                            let clipped = parent.kind.frame_props().is_some_and(|frame| frame.clips_content);
                            findings.push(json!({"node_id":n.id,"code":if clipped {"clipped_child"} else {"out_of_bounds_child"},"severity":"warning","parent_id":parent_id,"message":"Child extends beyond its parent bounds","suggested_fix":null}));
                        }
                    }
                }
                if let NodeKind::Instance { component, .. } = &n.kind {
                    if component.is_none_or(|id| doc.get(id).is_none()) {
                        findings
                            .push(json!({"node_id":n.id,"code":"detached_instance","severity":"warning","message":"Instance has no available main component"}));
                    }
                }
                if let Some(t) = n.kind.any_text() {
                    // Report contrast only when both foreground and nearest
                    // ancestor background are unambiguous opaque solid colors.
                    if t.runs.is_empty() {
                        if let (Some(fg), Some(bg)) = (solid_color(&t.style.fills), nearest_background(doc, n.parent)) {
                            let ratio = contrast(fg, bg);
                            let large = t.style.font_size >= 24.0 || (t.style.font_size >= 18.66 && t.style.font_weight >= 700);
                            let aa_threshold = if large { 3.0 } else { 4.5 };
                            let aaa_threshold = if large { 4.5 } else { 7.0 };
                            if ratio < aaa_threshold {
                                let black = Color::BLACK;
                                let white = Color::WHITE;
                                let best = if contrast(black, bg) >= contrast(white, bg) { black } else { white };
                                let mut suggestion = Value::Null;
                                // Offer a dry-run repair only when the best neutral
                                // foreground meets at least the failed AA criterion.
                                if contrast(best, bg) >= aa_threshold {
                                    if let Ok(mut style) = serde_json::to_value(&t.style) {
                                        if let Some(first) = style.get_mut("fills").and_then(Value::as_array_mut).and_then(|fills| fills.first_mut()) {
                                            first["kind"] = json!({"Solid":best});
                                            suggestion = json!({"expected_revision":doc.revision,"dry_run":true,"ops":[{"op":"set_text_patch","id":n.id,"patch":{"style":style}}]});
                                        }
                                    }
                                }
                                findings.push(json!({"node_id":n.id,"code":"low_text_contrast","severity":if ratio < aa_threshold { "warning" } else { "info" },"contrast_ratio":ratio,"required_ratio":aa_threshold,"wcag_aa_pass":ratio >= aa_threshold,"wcag_aaa_pass":ratio >= aaa_threshold,"wcag_aaa_required_ratio":aaa_threshold,"foreground":fg,"background":bg,"message":if ratio < aa_threshold { "Text contrast is below WCAG AA for the detected solid colors" } else { "Text contrast meets WCAG AA but is below WCAG AAA for the detected solid colors" },"suggested_fix":suggestion}));
                            }
                        }
                    }
                    for style in std::iter::once(&t.style).chain(t.runs.iter().map(|r| &r.style)) {
                        if !families.iter().any(|family| family.eq_ignore_ascii_case(&style.font_family)) {
                            let actual = desigma_render::resolve_font(&style.font_family, style.font_weight, style.italic);
                            findings.push(json!({"node_id":n.id,"code":"missing_font","severity":"warning","font_family":style.font_family,"resolved_family":actual.as_ref().map(|f|&f.family),"uses_bundled_substitute":actual.as_ref().is_some_and(|f|f.bundled)}));
                        }
                    }
                    let geom = desigma_render::text_geometry(t, n.size);
                    if geom.width > n.size.x + 0.5 || geom.height > n.size.y + 0.5 {
                        findings.push(json!({"node_id":n.id,"code":"text_overflow","severity":"warning","measured_width":geom.width,"measured_height":geom.height,"width":n.size.x,"height":n.size.y,"suggested_fix":{"expected_revision":doc.revision,"dry_run":true,"ops":[{"op":"set_properties","id":n.id,"properties":{"width":n.size.x.max(geom.width).ceil(),"height":n.size.y.max(geom.height).ceil()}}]}}));
                    }
                }
                for (property, variable) in &n.variables.bindings {
                    if doc.resolve_variable(n.id, variable).is_none() {
                        findings.push(json!({"node_id":n.id,"code":"unresolved_variable","severity":"warning","property":property,"variable_id":variable}));
                    }
                }
            }
            Ok(json!({"revision":doc.revision,"findings":findings,"checked_nodes":nodes.len(),"truncated":truncated}))
        }
        _ => Err(format!("unsupported_operation: unknown inspection tool {name}")),
    }
}
