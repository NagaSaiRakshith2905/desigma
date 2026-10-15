//! Safe workspace file handoff using the existing CPU/vector exporters.
use desigma_doc::{Document, DocumentKind, NodeId, NodeKind};
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Component, Path, PathBuf},
};

const MAX_OUTPUT: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeExportFormat {
    Png,
    Svg,
    Pdf,
}

impl NodeExportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Svg => "svg",
            Self::Pdf => "pdf",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportReceipt {
    /// Normalized workspace-relative path created by this operation.
    pub path: String,
    pub format: &'static str,
    pub bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("invalid export path: paths must be relative, stay inside the workspace, and use the matching extension")]
    InvalidPath,
    #[error("node does not exist")]
    MissingNode,
    #[error("unsupported export target: {0}")]
    Unsupported(String),
    #[error("export failed: {0}")]
    Export(String),
    #[error("output already exists; exports never overwrite files")]
    Exists,
    #[error("file operation failed: {0}")]
    Io(String),
}

fn output_path(workspace: &Path, relative: &Path, format: &str) -> Result<(PathBuf, String), ExportError> {
    if relative.as_os_str().is_empty() || relative.is_absolute() || relative.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(ExportError::InvalidPath);
    }
    let ext = relative.extension().and_then(|s| s.to_str()).ok_or(ExportError::InvalidPath)?;
    if !ext.eq_ignore_ascii_case(format) {
        return Err(ExportError::InvalidPath);
    }
    let root = workspace.canonicalize().map_err(|e| ExportError::Io(e.to_string()))?;
    let parent = relative.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let canonical_parent = root.join(parent).canonicalize().map_err(|_| ExportError::InvalidPath)?;
    if !canonical_parent.starts_with(&root) {
        return Err(ExportError::InvalidPath);
    }
    let name = relative.file_name().ok_or(ExportError::InvalidPath)?;
    let path = canonical_parent.join(name);
    let display = relative
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    Ok((path, display))
}

fn write_new(workspace: &Path, relative: &Path, format: &'static str, bytes: &[u8]) -> Result<ExportReceipt, ExportError> {
    if bytes.is_empty() || bytes.len() > MAX_OUTPUT {
        return Err(ExportError::Export("output is empty or exceeds 128 MiB".into()));
    }
    let (path, display) = output_path(workspace, relative, format)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            ExportError::Exists
        } else {
            ExportError::Io(e.to_string())
        }
    })?;
    if let Err(e) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(ExportError::Io(e.to_string()));
    }
    Ok(ExportReceipt { path: display, format, bytes: bytes.len() })
}

/// Render one existing node as PNG, SVG or a one-page raster PDF into a new
/// workspace file. `scale` applies to PNG; SVG and PDF ignore it.
pub fn export_node_to_workspace(
    doc: &Document,
    workspace: &Path,
    relative: &Path,
    node_id: NodeId,
    format: NodeExportFormat,
    scale: f32,
) -> Result<ExportReceipt, ExportError> {
    let node = doc.get(node_id).ok_or(ExportError::MissingNode)?;
    if !scale.is_finite() || scale <= 0.0 || scale > 4.0 {
        return Err(ExportError::Export("scale must be in (0,4]".into()));
    }
    let resolved = desigma_doc::apply_bound_variables(doc);
    let bytes = match format {
        NodeExportFormat::Png => {
            let bounds = desigma_export::render_bounds(&resolved, node_id);
            let w = (bounds.w * scale).ceil();
            let h = (bounds.h * scale).ceil();
            if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 || w > 4096.0 || h > 4096.0 || w * h > 8_388_608.0 {
                return Err(ExportError::Export("PNG dimensions exceed 4096 per side or 8 megapixels".into()));
            }
            desigma_export::export_png(&resolved, node_id, scale)
        }
        NodeExportFormat::Svg => desigma_export::export_svg(&resolved, node_id, &Default::default()).into_bytes(),
        NodeExportFormat::Pdf => {
            if !matches!(node.kind, NodeKind::Frame(_)) {
                return Err(ExportError::Unsupported("PDF export requires a frame; use a deck page to export slides".into()));
            }
            desigma_export::export_pdf(&resolved, &[node_id]).map_err(ExportError::Export)?
        }
    };
    write_new(workspace, relative, format.extension(), &bytes)
}

/// Export all slide frames on a deck page, in their existing layer order, as a
/// single raster PDF. This uses the PDF exporter's own slide and size limits.
pub fn export_deck_pdf_to_workspace(doc: &Document, workspace: &Path, relative: &Path, page_id: NodeId) -> Result<ExportReceipt, ExportError> {
    if doc.kind != DocumentKind::Deck {
        return Err(ExportError::Unsupported("document is not a deck".into()));
    }
    let page = doc.get(page_id).ok_or(ExportError::MissingNode)?;
    if !matches!(page.kind, NodeKind::Page { .. }) {
        return Err(ExportError::Unsupported("deck PDF target must be a page".into()));
    }
    let slides: Vec<_> =
        page.children.iter().copied().filter(|id| doc.get(*id).is_some_and(|n| n.slide.is_some() && matches!(n.kind, NodeKind::Frame(_)))).collect();
    let resolved = desigma_doc::apply_bound_variables(doc);
    let bytes = desigma_export::export_pdf(&resolved, &slides).map_err(ExportError::Export)?;
    write_new(workspace, relative, "pdf", &bytes)
}

impl ExportReceipt {
    pub fn format_label(&self) -> &'static str {
        self.format
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desigma_doc::{Node, Rect};

    fn workspace() -> (PathBuf, NodeId, Document) {
        let root = std::env::temp_dir().join(format!(
            "desigma-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut doc = Document::new("export");
        let page = doc.add_page("Page");
        let id = doc.alloc_id();
        doc.insert(page, Node::frame(id, Rect::new(0.0, 0.0, 30.0, 20.0))).unwrap();
        (root, id, doc)
    }

    #[test]
    fn exports_create_new_workspace_files_without_overwriting() {
        let (root, id, doc) = workspace();
        let receipt = export_node_to_workspace(&doc, &root, Path::new("frame.svg"), id, NodeExportFormat::Svg, 1.0).unwrap();
        assert_eq!(receipt.path, "frame.svg");
        assert!(std::fs::read(root.join("frame.svg")).unwrap().windows(4).any(|w| w == b"<svg"));
        assert!(matches!(export_node_to_workspace(&doc, &root, Path::new("frame.svg"), id, NodeExportFormat::Svg, 1.0), Err(ExportError::Exists)));
        assert!(matches!(export_node_to_workspace(&doc, &root, Path::new("../escape.svg"), id, NodeExportFormat::Svg, 1.0), Err(ExportError::InvalidPath)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn png_and_single_frame_pdf_use_existing_exporters() {
        let (root, id, doc) = workspace();
        let png = export_node_to_workspace(&doc, &root, Path::new("frame.png"), id, NodeExportFormat::Png, 1.0).unwrap();
        assert!(std::fs::read(root.join(png.path)).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
        let pdf = export_node_to_workspace(&doc, &root, Path::new("frame.pdf"), id, NodeExportFormat::Pdf, 1.0).unwrap();
        assert!(std::fs::read(root.join(pdf.path)).unwrap().starts_with(b"%PDF-1.4"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn deck_pdf_exports_slide_frames_in_page_order() {
        let (root, _, mut doc) = workspace();
        doc.kind = DocumentKind::Deck;
        let page = doc.pages[0];
        let mut tx = crate::edit::Tx::new();
        let slide = crate::decks_ops::add_slide(&mut doc, &mut tx, page, None, Some("First".into())).unwrap();
        tx.commit(&mut doc, &mut desigma_doc::History::default()).unwrap();
        let receipt = export_deck_pdf_to_workspace(&doc, &root, Path::new("deck.pdf"), page).unwrap();
        assert!(std::fs::read(root.join(receipt.path)).unwrap().starts_with(b"%PDF-1.4"));
        assert_eq!(doc.get(slide).unwrap().slide.as_ref().unwrap().notes, "");
        let _ = std::fs::remove_dir_all(root);
    }
}
