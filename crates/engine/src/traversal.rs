//! Recursive node traversal, undo/redo snapshot management, and layer resolution for `EditorEngine` (`R8.1`).

use coralcraft_doc::{Document, Layer, LayerId, LayerRole, ObjectId, ObjectKind, ObjectNode};
use coralcraft_geom::{Point, Rect, artistic_text_to_path};
use coralcraft_tools::{PageOrientation, object_local_bounds};

use crate::EditorEngine;
use crate::commands::{ActivePageTarget, UndoEntry};

pub(crate) fn flatten_group_recursive(obj: ObjectNode, out: &mut Vec<ObjectNode>) {
    if let ObjectKind::Group { children } = obj.kind {
        let group_tf = obj.transform;
        for mut child in children {
            if let Ok(composed) = child.transform.then(group_tf) {
                child.transform = composed;
            }
            flatten_group_recursive(child, out);
        }
    } else {
        out.push(obj);
    }
}

impl EditorEngine {
    /// Returns `true` if at least one command can be undone.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Returns `true` if at least one command can be redone.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Undoes the most recent committed [`EngineCommand`].
    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo_stack.pop() else {
            return false;
        };
        let current = self.capture_snapshot();
        self.redo_stack.push(current);
        self.restore_snapshot(prev);
        true
    }

    /// Redoes the most recently undone [`EngineCommand`].
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo_stack.pop() else {
            return false;
        };
        let current = self.capture_snapshot();
        self.undo_stack.push(current);
        self.restore_snapshot(next);
        true
    }

    /// Convenience helper returning orientation of the active page.
    #[must_use]
    pub fn current_page_orientation(&self) -> PageOrientation {
        let p = self.current_page();
        if p.size.width.get() >= p.size.height.get() { PageOrientation::Landscape } else { PageOrientation::Portrait }
    }

    /// Computes local bounds of an object kind (`coralcraft_tools::object_local_bounds` re-export).
    #[must_use]
    pub fn local_bounds(kind: &ObjectKind) -> Rect {
        object_local_bounds(kind)
    }

    pub(crate) fn capture_snapshot(&self) -> UndoEntry {
        UndoEntry {
            document: self.document.clone(),
            active_target: self.active_target,
            selection: self.selection.clone(),
            tool_state: self.tool_state.clone(),
            default_fill: self.default_fill.clone(),
            default_outline: self.default_outline.clone(),
        }
    }

    pub(crate) fn restore_snapshot(&mut self, entry: UndoEntry) {
        let high_water_next_id = self.document.next_id.max(entry.document.next_id);
        self.document = entry.document;
        self.document.next_id = high_water_next_id;
        self.active_target = entry.active_target;
        self.selection = entry.selection;
        self.tool_state = entry.tool_state;
        self.default_fill = entry.default_fill;
        self.default_outline = entry.default_outline;
    }

    #[must_use]
    pub fn current_target_objects(&self) -> &[ObjectNode] {
        match self.active_target {
            ActivePageTarget::Page(idx) => {
                let page_idx = idx.min(self.document.pages.len().saturating_sub(1));
                self.document.pages[page_idx]
                    .layers
                    .iter()
                    .rev()
                    .find(|l| l.role == LayerRole::Normal)
                    .or_else(|| self.document.pages[page_idx].layers.last())
                    .map_or(&[], |l| l.objects.as_slice())
            }
            ActivePageTarget::MasterPage => {
                self.document.master_layers.iter().find(|l| l.role == LayerRole::MasterAllPages).map_or(&[], |l| l.objects.as_slice())
            }
            ActivePageTarget::Desktop => {
                self.document.master_layers.iter().find(|l| l.role == LayerRole::Desktop).map_or(&[], |l| l.objects.as_slice())
            }
        }
    }

    /// Looks up an [`ObjectNode`] by `id` across all numbered pages and master/desktop layers.
    #[must_use]
    pub fn inspect_object(&self, id: ObjectId) -> Option<&ObjectNode> {
        self.selected_or_all_node(id)
    }

    pub(crate) fn selected_or_all_node(&self, id: ObjectId) -> Option<&ObjectNode> {
        for page in &self.document.pages {
            for layer in &page.layers {
                if let Some(found) = find_node_recursive(&layer.objects, id) {
                    return Some(found);
                }
            }
        }
        for layer in &self.document.master_layers {
            if let Some(found) = find_node_recursive(&layer.objects, id) {
                return Some(found);
            }
        }
        None
    }

    pub(crate) fn target_layer_mut(&mut self) -> &mut Layer {
        match self.active_target {
            ActivePageTarget::Page(idx) => {
                if self.document.pages.is_empty() {
                    Self::ensure_minimum_structure(&mut self.document);
                }
                let page_idx = idx.min(self.document.pages.len().saturating_sub(1));
                let normal_pos = self.document.pages.get(page_idx).and_then(|p| p.layers.iter().rposition(|l| l.role == LayerRole::Normal));
                let pos = match normal_pos {
                    Some(pos) => pos,
                    None => {
                        let layer_id = LayerId(self.document.allocate_object_id().0);
                        let page = &mut self.document.pages[page_idx];
                        page.layers.push(Layer::new_normal(layer_id, "Layer 1"));
                        page.layers.len() - 1
                    }
                };
                &mut self.document.pages[page_idx].layers[pos]
            }
            ActivePageTarget::MasterPage => {
                let pos = match self.document.master_layers.iter().position(|l| l.role == LayerRole::MasterAllPages) {
                    Some(pos) => pos,
                    None => {
                        let id = LayerId(self.document.allocate_object_id().0);
                        let mut master = Layer::new_normal(id, "Master Page");
                        master.role = LayerRole::MasterAllPages;
                        self.document.master_layers.push(master);
                        self.document.master_layers.len() - 1
                    }
                };
                &mut self.document.master_layers[pos]
            }
            ActivePageTarget::Desktop => {
                let pos = match self.document.master_layers.iter().position(|l| l.role == LayerRole::Desktop) {
                    Some(pos) => pos,
                    None => {
                        let id = LayerId(self.document.allocate_object_id().0);
                        self.document.master_layers.push(Layer::new_desktop(id));
                        self.document.master_layers.len() - 1
                    }
                };
                &mut self.document.master_layers[pos]
            }
        }
    }

    pub(crate) fn for_each_selected_mut(&mut self, mut f: impl FnMut(&mut ObjectNode)) {
        let sel = self.selection.clone();
        self.for_each_node_mut(|obj| {
            if sel.contains(&obj.id) {
                f(obj);
            }
        });
    }

    pub(crate) fn for_each_node_mut(&mut self, mut f: impl FnMut(&mut ObjectNode)) {
        for page in &mut self.document.pages {
            for layer in &mut page.layers {
                visit_nodes_mut_recursive(&mut layer.objects, &mut f);
            }
        }
        for layer in &mut self.document.master_layers {
            visit_nodes_mut_recursive(&mut layer.objects, &mut f);
        }
    }
}

pub(crate) fn find_node_recursive(nodes: &[ObjectNode], id: ObjectId) -> Option<&ObjectNode> {
    for obj in nodes {
        if obj.id == id {
            return Some(obj);
        }
        if let Some(found) = find_node_recursive(&obj.powerclip_children, id) {
            return Some(found);
        }
        match &obj.kind {
            ObjectKind::Group { children } => {
                if let Some(found) = find_node_recursive(children, id) {
                    return Some(found);
                }
            }
            ObjectKind::Unsupported(u) => {
                if let Some(found) = find_node_recursive(&u.children, id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn collect_selected_recursive<'a>(nodes: &'a [ObjectNode], selection: &[ObjectId], out: &mut Vec<&'a ObjectNode>) {
    for obj in nodes {
        if selection.contains(&obj.id) {
            out.push(obj);
        }
        collect_selected_recursive(&obj.powerclip_children, selection, out);
        match &obj.kind {
            ObjectKind::Group { children } => collect_selected_recursive(children, selection, out),
            ObjectKind::Unsupported(u) => collect_selected_recursive(&u.children, selection, out),
            _ => {}
        }
    }
}

pub(crate) fn visit_nodes_mut_recursive(nodes: &mut [ObjectNode], f: &mut impl FnMut(&mut ObjectNode)) {
    for obj in nodes {
        f(obj);
        visit_nodes_mut_recursive(&mut obj.powerclip_children, f);
        match &mut obj.kind {
            ObjectKind::Group { children } => visit_nodes_mut_recursive(children, f),
            ObjectKind::Unsupported(u) => visit_nodes_mut_recursive(&mut u.children, f),
            _ => {}
        }
    }
}

pub(crate) fn count_nodes_recursive(nodes: &[ObjectNode]) -> usize {
    let mut count = 0usize;
    for obj in nodes {
        count = count.saturating_add(1);
        count = count.saturating_add(count_nodes_recursive(&obj.powerclip_children));
        match &obj.kind {
            ObjectKind::Group { children } => {
                count = count.saturating_add(count_nodes_recursive(children));
            }
            ObjectKind::Unsupported(u) => {
                count = count.saturating_add(count_nodes_recursive(&u.children));
            }
            _ => {}
        }
    }
    count
}

pub(crate) fn powerclip_subtree_depth(node: &ObjectNode) -> usize {
    node.powerclip_children.iter().map(|c| 1 + powerclip_subtree_depth(c)).max().unwrap_or(0)
}

pub(crate) fn object_to_path(obj: &ObjectNode) -> Option<coralcraft_geom::Path> {
    match &obj.kind {
        ObjectKind::Rect(r) => r.to_path().ok(),
        ObjectKind::Ellipse(e) => e.to_path().ok(),
        ObjectKind::Polygon(p) => p.to_path().ok(),
        ObjectKind::Path(p) => Some(p.clone()),
        ObjectKind::ArtisticText { text, font_family, size_pt, position } => Some(artistic_text_to_path(text, font_family, size_pt.get(), *position)),
        ObjectKind::ParagraphText(pt) => {
            let layout = coralcraft_doc::layout_paragraph_text(pt);
            let mut combined = Vec::new();
            for line in &layout.lines {
                if let Ok(origin) = Point::new(line.origin_x, line.origin_y) {
                    let line_path = artistic_text_to_path(&line.text, &pt.font_family, pt.font_size_pt.get(), origin);
                    combined.extend(line_path.elements);
                }
            }
            coralcraft_geom::Path::from_elements(combined, coralcraft_geom::FillRule::NonZero).ok()
        }
        _ => None,
    }
}

pub(crate) fn reallocate_object_ids_recursive(node: &mut ObjectNode, doc: &mut Document) {
    node.id = doc.allocate_object_id();
    for child in &mut node.powerclip_children {
        reallocate_object_ids_recursive(child, doc);
    }
    match &mut node.kind {
        ObjectKind::Group { children } => {
            for child in children {
                reallocate_object_ids_recursive(child, doc);
            }
        }
        ObjectKind::Unsupported(u) => {
            for child in &mut u.children {
                reallocate_object_ids_recursive(child, doc);
            }
        }
        _ => {}
    }
}
