#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Defines the type of mapping from node ID to colors used by an
/// overlay script
pub enum OverlayKind {
    /// Overlay scripts that produce an RGB color for each node
    RGB,
    /// Overlay scripts that produce a single value for each node,
    /// that can then be mapped to a color, e.g. using a perceptual
    /// color scheme
    Value,
}

pub enum OverlayData {
    RGB(Vec<rgb::RGBA<f32>>),
    Value(Vec<f32>),
}

pub fn hash_node_color(hash: u64) -> (f32, f32, f32) {
    let r_u16 = ((hash >> 32) & 0xFFFFFFFF) as u16;
    let g_u16 = ((hash >> 16) & 0xFFFFFFFF) as u16;
    let b_u16 = (hash & 0xFFFFFFFF) as u16;

    let max = r_u16.max(g_u16).max(b_u16) as f32;
    let r = (r_u16 as f32) / max;
    let g = (g_u16 as f32) / max;
    let b = (b_u16 as f32) / max;
    (r, g, b)
}
