use serde::{Deserialize, Serialize};

use crate::layout_engine::LayoutId;
use crate::model::tree::{NodeId, NodeMap};

#[derive(Copy, Clone)]
pub enum TreeEvent {
    AddedToForest(NodeId),
    AddedToParent(NodeId),
    Copied {
        src: NodeId,
        dest: NodeId,
        dest_layout: LayoutId,
    },
    RemovingFromParent(NodeId),
    RemovedFromForest(NodeId),
}

#[derive(Default, Serialize, Deserialize)]
pub struct Selection {
    nodes: slotmap::SecondaryMap<NodeId, SelectionInfo>,
}

#[derive(Serialize, Deserialize)]
struct SelectionInfo {
    selected_child: NodeId,
    stop_here: bool,
    range_start: Option<NodeId>,
    range_end: Option<NodeId>,
}

impl Selection {
    pub fn current_selection(&self, root: NodeId) -> NodeId {
        let mut node = root;
        while let Some(info) = self.nodes.get(node) {
            if info.stop_here {
                break;
            }
            node = info.selected_child;
        }
        node
    }

    pub fn last_selection(&self, _map: &NodeMap, node: NodeId) -> Option<NodeId> {
        self.nodes.get(node).map(|info| info.selected_child)
    }

    pub fn local_selection(&self, map: &NodeMap, node: NodeId) -> Option<NodeId> {
        let result = self.nodes.get(node);
        if let Some(result) = result {
            debug_assert_eq!(result.selected_child.parent(map), Some(node));
        }
        result.filter(|info| !info.stop_here).map(|info| info.selected_child)
    }

    pub fn select_locally(&mut self, map: &NodeMap, node: NodeId) -> bool {
        if let Some(parent) = node.parent(map) {
            self.nodes
                .insert(parent, SelectionInfo {
                    selected_child: node,
                    stop_here: false,
                    range_start: None,
                    range_end: None,
                })
                .map(|info| info.selected_child != node)
                .unwrap_or(true)
        } else {
            false
        }
    }

    pub fn select(&mut self, map: &NodeMap, selection: NodeId) {
        if let Some(info) = self.nodes.get_mut(selection) {
            info.stop_here = true;
        }
        let mut node = selection;
        while let Some(parent) = node.parent(map) {
            self.nodes.insert(parent, SelectionInfo {
                selected_child: node,
                stop_here: false,
                range_start: None,
                range_end: None,
            });
            node = parent;
        }
    }

    pub fn handle_event(&mut self, map: &NodeMap, event: TreeEvent) {
        use TreeEvent::*;
        match event {
            AddedToForest(_node) => {}
            AddedToParent(_node) => {}
            Copied { src, dest, .. } => {
                let Some(info) = self.nodes.get(src) else {
                    return;
                };
                let selected_child = std::iter::zip(src.children(map), dest.children(map))
                    .filter(|(src_child, _)| *src_child == info.selected_child)
                    .map(|(_, dest_child)| dest_child)
                    .next()
                    .unwrap_or_else(|| panic!("Dest tree had different structure, or source node had nonexistent selection: {src:?}, {dest:?}"));
                self.nodes.insert(dest, SelectionInfo {
                    selected_child,
                    stop_here: self.nodes[src].stop_here,
                    range_start: None,
                    range_end: None,
                });
            }
            RemovingFromParent(node) => {
                let parent = node.parent(map).unwrap();
                if self.nodes.get(parent).map(|n| n.selected_child) == Some(node) {
                    if let Some(new_selection) = node.next_sibling(map).or(node.prev_sibling(map)) {
                        self.nodes[parent].selected_child = new_selection;
                    } else {
                        self.nodes.remove(parent);
                    }
                }
            }
            RemovedFromForest(node) => {
                self.nodes.remove(node);
            }
        }
    }

    pub fn set_range(&mut self, map: &NodeMap, node: NodeId, start: NodeId, end: NodeId) {
        if let Some(parent) = node.parent(map) {
            if let Some(info) = self.nodes.get_mut(parent) {
                info.range_start = Some(start);
                info.range_end = Some(end);
            }
        }
    }

    pub fn get_range(&self, map: &NodeMap, node: NodeId) -> Option<(NodeId, NodeId)> {
        if let Some(parent) = node.parent(map) {
            if let Some(info) = self.nodes.get(parent) {
                if let (Some(start), Some(end)) = (info.range_start, info.range_end) {
                    return Some((start, end));
                }
            }
        }
        None
    }

    pub fn clear_range(&mut self, map: &NodeMap, node: NodeId) {
        if let Some(parent) = node.parent(map) {
            if let Some(info) = self.nodes.get_mut(parent) {
                info.range_start = None;
                info.range_end = None;
            }
        }
    }
}
