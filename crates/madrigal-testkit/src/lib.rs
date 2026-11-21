slint::include_modules!();

use std::ops::ControlFlow;
use std::path::Path;
use std::time::Duration;

pub use i_slint_backend_testing::ElementHandle;
use i_slint_backend_testing::ElementRoot;

pub enum Error {
    ElementNotFound(String),
    Snapshot(String),
    Image(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ElementNotFound(id) => write!(f, "element not found: {id}"),
            Self::Snapshot(detail) => write!(f, "snapshot failed: {detail}"),
            Self::Image(detail) => write!(f, "image failed: {detail}"),
        }
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for Error {}

pub fn init_headless() {
    std::env::set_var("SLINT_BACKEND", "headless-software");
    std::env::set_var("SLINT_EMIT_DEBUG_INFO", "1");
    let backend = i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: true,
            renderer_name: Some(slint::SharedString::from("software")),
        },
    );
    slint::platform::set_platform(Box::new(backend)).expect("backend already initialized");
}

pub fn find_by_id(root: &impl ElementRoot, id: &str) -> Result<ElementHandle, Error> {
    let mut found = ElementHandle::find_by_element_id(root, id);
    found.next().ok_or_else(|| Error::ElementNotFound(id.to_string()))
}

pub fn click(handle: &ElementHandle) {
    handle.mock_single_click(slint::platform::PointerEventButton::Left);
}

pub fn click_by_id(root: &impl ElementRoot, id: &str) -> Result<(), Error> {
    let handle = find_by_id(root, id)?;
    click(&handle);
    Ok(())
}

pub fn mock_advance(total: Duration) {
    const STEP: Duration = Duration::from_millis(1);
    let mut rest = total;
    while rest > Duration::ZERO {
        let chunk = rest.min(STEP);
        i_slint_backend_testing::mock_elapsed_time(chunk);
        rest -= chunk;
    }
}

pub fn snapshot_dir(fallback: &Path) -> std::path::PathBuf {
    match std::env::var_os("MADRIGAL_SNAPSHOT_DIR") {
        Some(value) if !value.is_empty() => {
            let dir = std::path::PathBuf::from(value);
            std::fs::create_dir_all(&dir).expect("create MADRIGAL_SNAPSHOT_DIR");
            dir
        }
        _ => fallback.to_path_buf(),
    }
}

pub fn snapshot_to(window: &slint::Window, path: &Path) -> Result<(), Error> {
    let buffer = window.take_snapshot().map_err(|e| Error::Snapshot(e.to_string()))?;
    let pixels = buffer.as_bytes().to_vec();
    let width = buffer.width();
    let height = buffer.height();
    let picture = image::RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| Error::Snapshot("empty pixel buffer".to_string()))?;
    picture.save(path).map_err(|e| Error::Image(e.to_string()))?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    fn right(&self) -> f32 {
        self.x + self.width
    }

    fn bottom(&self) -> f32 {
        self.y + self.height
    }

    fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    fn overlap_exceeds(&self, other: &Rect, tolerance: f32) -> bool {
        let horizontal = self.right().min(other.right()) - self.x.max(other.x);
        let vertical = self.bottom().min(other.bottom()) - self.y.max(other.y);
        horizontal > tolerance && vertical > tolerance
    }
}

impl std::fmt::Display for Rect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({:.0}, {:.0} {:.0}x{:.0})", self.x, self.y, self.width, self.height)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ViolationKind {
    OutOfWindow,
    Overlap(String),
}

impl std::fmt::Display for ViolationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfWindow => write!(f, "out of window"),
            Self::Overlap(other) => write!(f, "overlaps {other}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Violation {
    pub id: String,
    pub kind: ViolationKind,
    pub rect: Rect,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} at {}", self.id, self.kind, self.rect)
    }
}

struct Node {
    handle: ElementHandle,
    label: String,
    text: bool,
    scroll: bool,
    rect: Rect,
    span: usize,
    parent: Option<usize>,
}

impl Node {
    fn from_handle(handle: ElementHandle) -> Self {
        let position = handle.absolute_position();
        let size = handle.size();
        let kind = handle.type_name().map(|value| value.to_string()).unwrap_or_default();
        Self {
            label: handle.id().map(|value| value.to_string()).unwrap_or(kind),
            text: is_kind(&handle, "Text"),
            scroll: is_kind(&handle, "Flickable"),
            rect: Rect { x: position.x, y: position.y, width: size.width, height: size.height },
            span: 0,
            parent: None,
            handle,
        }
    }
}

fn inherits(handle: &ElementHandle, base: &str) -> bool {
    handle.bases().is_some_and(|mut bases| bases.any(|name| name == base))
}

fn is_kind(handle: &ElementHandle, name: &str) -> bool {
    handle.type_name().is_some_and(|kind| kind == name) || inherits(handle, name)
}

fn subtree_len(handle: &ElementHandle) -> usize {
    let mut count = 0;
    handle.visit_descendants(|_| {
        count += 1;
        ControlFlow::<()>::Continue(())
    });
    count
}

fn build_tree(root: &impl ElementRoot) -> Vec<Node> {
    let mut nodes = Vec::new();
    root.root_element().visit_descendants(|handle| {
        nodes.push(Node::from_handle(handle));
        ControlFlow::<()>::Continue(())
    });
    for node in &mut nodes {
        node.span = subtree_len(&node.handle);
    }
    // The testing API exposes no parent link; a node's descendants are the contiguous pre-order run after it.
    for index in 1..nodes.len() {
        let mut parent = None;
        let mut smallest = usize::MAX;
        for candidate in 0..index {
            let span = nodes[candidate].span;
            if span == 0 || candidate + span < index || span >= smallest {
                continue;
            }
            smallest = span;
            parent = Some(candidate);
        }
        nodes[index].parent = parent;
    }
    nodes
}

pub fn lint_layout(root: &impl ElementRoot, window_size: (f32, f32)) -> Vec<Violation> {
    let nodes = build_tree(root);
    let mut violations = Vec::new();
    lint_window_bounds(&nodes, window_size, &mut violations);
    lint_text_pairs(&nodes, PairScope::Siblings, &mut violations);
    lint_text_pairs(&nodes, PairScope::DistinctParents, &mut violations);
    violations
}

fn clipped(nodes: &[Node], index: usize) -> bool {
    let mut current = nodes[index].parent;
    while let Some(parent) = current {
        if nodes[parent].scroll {
            return true;
        }
        current = nodes[parent].parent;
    }
    false
}

fn lint_window_bounds(nodes: &[Node], window_size: (f32, f32), out: &mut Vec<Violation>) {
    let (width, height) = window_size;
    let tolerance = 1.0;
    for (index, node) in nodes.iter().enumerate() {
        if node.rect.is_empty() || clipped(nodes, index) {
            continue;
        }
        let outside = node.rect.x < -tolerance
            || node.rect.y < -tolerance
            || node.rect.right() > width + tolerance
            || node.rect.bottom() > height + tolerance;
        if outside {
            out.push(Violation {
                id: node.label.clone(),
                kind: ViolationKind::OutOfWindow,
                rect: node.rect,
            });
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum PairScope {
    Siblings,
    DistinctParents,
}

fn lint_text_pairs(nodes: &[Node], scope: PairScope, out: &mut Vec<Violation>) {
    let texts: Vec<usize> = (0..nodes.len())
        .filter(|index| nodes[*index].text && !nodes[*index].rect.is_empty())
        .collect();
    let tolerance = 2.0;
    for left in 0..texts.len() {
        for right in (left + 1)..texts.len() {
            let same_parent = nodes[texts[left]].parent == nodes[texts[right]].parent;
            if same_parent != (scope == PairScope::Siblings) {
                continue;
            }
            if nodes[texts[left]].rect.overlap_exceeds(&nodes[texts[right]].rect, tolerance) {
                out.push(Violation {
                    id: nodes[texts[right]].label.clone(),
                    kind: ViolationKind::Overlap(nodes[texts[left]].label.clone()),
                    rect: nodes[texts[right]].rect,
                });
            }
        }
    }
}
