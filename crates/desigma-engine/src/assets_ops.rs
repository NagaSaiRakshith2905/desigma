//! Transactional image and SVG asset import helpers.
use crate::edit::Tx;
use desigma_doc::*;
use std::path::Path;

pub const MAX_ASSET_BYTES: usize = 50 * 1024 * 1024;
pub const MAX_SVG_NODES: usize = 10_000;
pub const MAX_IMAGE_PIXELS: u64 = 50_000_000;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ImageAssetInfo {
    pub reference: String,
    pub bytes: usize,
    pub format: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum AssetError {
    #[error("image or SVG data is invalid: {0}")]
    Invalid(&'static str),
    #[error("workspace path is invalid or escapes the workspace")]
    InvalidPath,
    #[error("node or parent not found")]
    MissingNode,
    #[error("document operation failed: {0}")]
    Document(String),
    #[error("asset conversion failed: {0}")]
    Conversion(String),
}
impl From<DocError> for AssetError {
    fn from(e: DocError) -> Self {
        Self::Document(e.to_string())
    }
}

fn image_format(bytes: &[u8]) -> Option<image::ImageFormat> {
    let format = image::guess_format(bytes).ok()?;
    matches!(
        format,
        image::ImageFormat::Png
            | image::ImageFormat::Jpeg
            | image::ImageFormat::Gif
            | image::ImageFormat::WebP
            | image::ImageFormat::Bmp
            | image::ImageFormat::Tiff
            | image::ImageFormat::Ico
            | image::ImageFormat::Pnm
            | image::ImageFormat::Tga
            | image::ImageFormat::Dds
            | image::ImageFormat::OpenExr
            | image::ImageFormat::Farbfeld
    )
    .then_some(format)
}

/// Lists embedded raster assets that decode successfully; reports format and dimensions.
pub fn list_images(doc: &Document) -> Vec<ImageAssetInfo> {
    doc.images
        .iter()
        .filter_map(|(reference, bytes)| {
            let format = image_format(bytes)?;
            let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(bytes), format).into_dimensions().ok()?;
            if width == 0 || height == 0 || (width as u64).saturating_mul(height as u64) > MAX_IMAGE_PIXELS {
                return None;
            }
            Some(ImageAssetInfo { reference: reference.clone(), bytes: bytes.len(), format: format!("{format:?}").to_ascii_lowercase(), width, height })
        })
        .collect()
}

fn next_image_key(doc: &Document) -> String {
    let mut n = 1u64;
    loop {
        let k = format!("image:{n}");
        if !doc.images.contains_key(&k) {
            return k;
        }
        n += 1;
    }
}

/// Imports raster bytes as one undoable embedded asset. The key is stable within this document.
pub fn import_image(doc: &mut Document, tx: &mut Tx, bytes: Vec<u8>) -> Result<String, AssetError> {
    if bytes.is_empty() || bytes.len() > MAX_ASSET_BYTES {
        return Err(AssetError::Invalid("image size"));
    }
    let format = image_format(&bytes).ok_or(AssetError::Invalid("unsupported image format"))?;
    let (width, height) =
        image::ImageReader::with_format(std::io::Cursor::new(&bytes), format).into_dimensions().map_err(|_| AssetError::Invalid("image header"))?;
    if width == 0 || height == 0 || width > 100_000 || height > 100_000 || (width as u64).saturating_mul(height as u64) > MAX_IMAGE_PIXELS {
        return Err(AssetError::Invalid("image dimensions"));
    }
    let decoded = image::load_from_memory_with_format(&bytes, format).map_err(|_| AssetError::Invalid("image decode"))?;
    if decoded.width() != width || decoded.height() != height {
        return Err(AssetError::Invalid("image dimensions"));
    }
    let key = next_image_key(doc);
    tx.cmd(doc, Command::SetImage { key: key.clone(), bytes: Some(bytes) })?;
    Ok(key)
}

/// Reads a relative file beneath the workspace; canonicalization prevents traversal and symlink escapes.
pub fn read_workspace_asset(workspace: &Path, relative: &Path) -> Result<Vec<u8>, AssetError> {
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        return Err(AssetError::InvalidPath);
    }
    let root = workspace.canonicalize().map_err(|_| AssetError::InvalidPath)?;
    let target = root.join(relative).canonicalize().map_err(|_| AssetError::InvalidPath)?;
    if !target.starts_with(&root) || !target.is_file() {
        return Err(AssetError::InvalidPath);
    }
    let metadata = std::fs::metadata(&target).map_err(|_| AssetError::InvalidPath)?;
    if metadata.len() == 0 || metadata.len() as usize > MAX_ASSET_BYTES {
        return Err(AssetError::Invalid("asset file size"));
    }
    std::fs::read(target).map_err(|_| AssetError::InvalidPath)
}

pub fn import_image_path(doc: &mut Document, tx: &mut Tx, workspace: &Path, relative: &Path) -> Result<String, AssetError> {
    let bytes = read_workspace_asset(workspace, relative)?;
    import_image(doc, tx, bytes)
}

/// Parses SVG into a root-first native subtree, embeds its image dependencies,
/// and inserts everything as a single undoable transaction.
pub fn import_svg(doc: &mut Document, tx: &mut Tx, parent: NodeId, name: &str, svg: &str, at: Vec2) -> Result<NodeId, AssetError> {
    if svg.len() > MAX_ASSET_BYTES || !at.x.is_finite() || !at.y.is_finite() || at.x.abs() > 1_000_000.0 || at.y.abs() > 1_000_000.0 {
        return Err(AssetError::Invalid("SVG size or placement"));
    }
    if !doc.get(parent).is_some_and(|n| n.kind.is_container()) {
        return Err(AssetError::MissingNode);
    }
    if doc.remaining_node_ids() < MAX_SVG_NODES as u64 {
        return Err(AssetError::Invalid("insufficient node ID capacity"));
    }
    let mut parsed = desigma_svg::svg_to_nodes(svg, name, &mut || doc.alloc_id()).map_err(|e| AssetError::Conversion(e.to_string()))?;
    if parsed.nodes.is_empty() || parsed.nodes.len() > MAX_SVG_NODES {
        return Err(AssetError::Invalid("SVG node count"));
    }
    parsed.nodes[0].transform = Affine::translate(at.x, at.y);
    let root = parsed.nodes[0].id;
    let mut commands = Vec::with_capacity(parsed.images.len() + 1);
    for (key, bytes) in parsed.images {
        if bytes.len() > MAX_ASSET_BYTES {
            return Err(AssetError::Invalid("embedded SVG image size"));
        }
        commands.push(Command::SetImage { key, bytes: Some(bytes) });
    }
    commands.push(Command::Insert { parent, index: doc.get(parent).unwrap().children.len(), nodes: parsed.nodes });
    tx.cmd(doc, Command::Batch(commands))?;
    Ok(root)
}

/// SVG import from a workspace-relative path with traversal and symlink protection.
pub fn import_svg_path(doc: &mut Document, tx: &mut Tx, workspace: &Path, relative: &Path, parent: NodeId) -> Result<NodeId, AssetError> {
    let bytes = read_workspace_asset(workspace, relative)?;
    let svg = std::str::from_utf8(&bytes).map_err(|_| AssetError::Invalid("SVG must be UTF-8"))?;
    if !desigma_svg::looks_like_svg(svg) {
        return Err(AssetError::Invalid("file is not SVG"));
    }
    let name = relative.file_stem().and_then(|s| s.to_str()).unwrap_or("svg");
    import_svg(doc, tx, parent, name, svg, Vec2::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgba8(2, 3).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        bytes
    }
    #[test]
    fn image_import_list_and_undo() {
        let mut d = Document::new("assets");
        let mut tx = Tx::new();
        let key = import_image(&mut d, &mut tx, png()).unwrap();
        let mut h = History::default();
        tx.commit(&mut d, &mut h).unwrap();
        let listed = list_images(&d);
        assert_eq!(listed[0].reference, key);
        assert_eq!((listed[0].width, listed[0].height), (2, 3));
        h.undo(&mut d).unwrap();
        assert!(d.images.is_empty());
    }
    #[test]
    fn svg_import_is_one_undo_step() {
        let mut d = Document::new("assets");
        let p = d.add_page("P");
        let mut tx = Tx::new();
        let id = import_svg(
            &mut d,
            &mut tx,
            p,
            "mark",
            "<svg xmlns='http://www.w3.org/2000/svg' width='20' height='10'><rect width='20' height='10' fill='red'/></svg>",
            Vec2::new(4.0, 5.0),
        )
        .unwrap();
        let mut h = History::default();
        tx.commit(&mut d, &mut h).unwrap();
        assert!(d.get(id).is_some());
        h.undo(&mut d).unwrap();
        assert!(d.get(id).is_none());
    }
    #[test]
    fn workspace_path_rejects_parent_escape() {
        let root = std::env::temp_dir().join(format!("desigma-assets-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(read_workspace_asset(&root, Path::new("../escape.png")), Err(AssetError::InvalidPath));
        let _ = std::fs::remove_dir_all(root);
    }
}
