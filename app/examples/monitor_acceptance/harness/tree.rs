//! Tree reads over one snapshot: the drawer root, panel geometry, rows, status.

use bevy::ecs::entity::Entity;
use bevy::prelude::*;

use super::{NodeSnap, PanelGeometry, RowSnap, Snap};

impl Snap {
    pub fn node(&self, entity: Entity) -> Option<&NodeSnap> {
        self.nodes.iter().find(|node| node.entity == entity)
    }

    pub fn text(&self, entity: Entity) -> Option<&str> {
        self.texts.get(&entity).map(String::as_str)
    }

    pub fn color(&self, entity: Entity) -> Option<Color> {
        self.colors.get(&entity).copied()
    }

    pub fn child_list(&self, parent: Entity) -> Vec<Entity> {
        self.children.get(&parent).cloned().unwrap_or_default()
    }

    /// The drawer's stacked root: a `GlobalZIndex(100)` root node.
    pub fn drawer_root(&self) -> Option<Entity> {
        self.nodes
            .iter()
            .find(|node| node.zindex == 100 && !self.parents.contains_key(&node.entity))
            .map(|node| node.entity)
    }

    pub fn drawer_open(&self) -> bool {
        self.drawer_root().is_some()
    }

    /// Whether `entity` is the drawer root or one of its descendants.
    pub fn under_drawer(&self, entity: Entity) -> bool {
        let Some(root) = self.drawer_root() else {
            return false;
        };
        let mut current = entity;
        loop {
            if current == root {
                return true;
            }
            match self.parents.get(&current) {
                Some(parent) => current = *parent,
                None => return false,
            }
        }
    }

    /// The drawer panel node: the root's only child.
    pub fn panel(&self) -> Option<Entity> {
        self.child_list(self.drawer_root()?).first().copied()
    }

    pub fn geometry(&self) -> Option<PanelGeometry> {
        let panel = self.panel()?;
        let panel_node = self.node(panel)?;
        let children = self.child_list(panel);
        let header = children
            .iter()
            .find(|entity| {
                self.node(**entity)
                    .is_some_and(|node| node.shape.contains("h=Px(52.0)"))
            })
            .copied()?;
        let body = children
            .iter()
            .find(|entity| {
                self.node(**entity)
                    .is_some_and(|node| node.shape.contains("grow=1"))
            })
            .copied()?;
        let footer = children
            .iter()
            .find(|entity| {
                self.node(**entity)
                    .is_some_and(|node| node.shape.contains("h=Px(13.0)"))
            })
            .copied()?;
        let body_node = self.node(body)?;
        Some(PanelGeometry {
            x: panel_node.position.x,
            y: panel_node.position.y,
            width: panel_node.size.x,
            height: panel_node.size.y,
            header: self.node(header)?.size.y,
            body: body_node.size.y,
            footer: self.node(footer)?.size.y,
            body_top: body_node.position.y,
        })
    }

    /// Every materialized log row, top edge first.
    pub fn rows(&self) -> Vec<RowSnap> {
        let mut rows: Vec<RowSnap> = self
            .nodes
            .iter()
            .filter(|node| {
                node.shape.contains("bd=UiRect { left: Px(2.0)")
                    && node.shape.contains("p=UiRect { left: Px(12.0)")
            })
            .filter_map(|node| {
                let parts = self.child_list(node.entity);
                if !(parts.len() == 4 || parts.len() == 1) {
                    return None;
                }
                Some(RowSnap {
                    entity: node.entity,
                    top: node.position.y,
                    height: node.size.y,
                    truncated: parts.len() == 1,
                    parts,
                })
            })
            .collect();
        rows.sort_by(|a, b| {
            a.top
                .partial_cmp(&b.top)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        rows
    }

    /// The status footer's rendered text.
    pub fn status(&self) -> Option<String> {
        let panel = self.panel()?;
        let footer = self.child_list(panel).into_iter().find(|entity| {
            self.node(*entity)
                .is_some_and(|node| node.shape.contains("h=Px(13.0)"))
        })?;
        let mut text = String::new();
        self.collect_text(footer, &mut text);
        Some(text)
    }

    /// The empty-projection state line, if it is drawn.
    pub fn empty_label(&self) -> Option<String> {
        self.nodes.iter().find_map(|node| {
            let text = self.text(node.entity)?;
            (text.starts_with("no unmei::") || text.starts_with("no diagnostics"))
                .then(|| text.to_string())
        })
    }

    /// The header's rendered labels, left to right.
    pub fn header_controls(&self) -> Vec<(String, f32)> {
        let Some(panel) = self.panel() else {
            return Vec::new();
        };
        let Some(header) = self.child_list(panel).into_iter().find(|entity| {
            self.node(*entity)
                .is_some_and(|node| node.shape.contains("h=Px(52.0)"))
        }) else {
            return Vec::new();
        };
        let mut labels: Vec<(String, f32)> = Vec::new();
        self.collect_leaves(header, &mut |entity| {
            if let Some(text) = self.text(entity) {
                if !text.is_empty() {
                    let x = self.node(entity).map_or(0.0, |node| node.position.x);
                    labels.push((text.to_string(), x));
                }
            }
        });
        labels.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        labels
    }

    /// The scrollbar thumb's height and top, as fractions of the track.
    pub fn thumb(&self) -> Option<(f32, f32, f32)> {
        let track = self.scrollbar_track()?;
        let thumb = self.child_list(track).into_iter().next()?;
        let node = self.node(thumb)?;
        Some((node.size.y, node.position.y, self.node(track)?.size.y))
    }

    /// The rendered scrollbar track's laid-out rect, so a drag can address the
    /// real hit-test target instead of a guessed coordinate.
    pub fn scrollbar_track(&self) -> Option<Entity> {
        let body = self.log_body()?;
        self.child_list(body).into_iter().find(|entity| {
            self.node(*entity)
                .is_some_and(|node| node.shape.contains("w=Px(10.0)"))
        })
    }

    /// The log body node.
    pub fn log_body(&self) -> Option<Entity> {
        let panel = self.panel()?;
        self.child_list(panel).into_iter().find(|entity| {
            self.node(*entity)
                .is_some_and(|node| node.shape.contains("grow=1"))
        })
    }

    /// The top visible row: the first row whose bottom edge is below the body's
    /// top edge.
    pub fn top_visible_row(&self) -> Option<RowSnap> {
        let geometry = self.geometry()?;
        self.rows()
            .into_iter()
            .find(|row| row.top + row.height > geometry.body_top + 0.5)
    }

    /// The top visible row that is *fully* inside the body.
    pub fn top_full_row(&self) -> Option<RowSnap> {
        let geometry = self.geometry()?;
        self.rows()
            .into_iter()
            .find(|row| row.top >= geometry.body_top - 0.5)
    }

    fn collect_text(&self, entity: Entity, out: &mut String) {
        if let Some(text) = self.text(entity) {
            out.push_str(text);
        }
        for child in self.child_list(entity) {
            self.collect_text(child, out);
        }
    }

    fn collect_leaves(&self, entity: Entity, visit: &mut impl FnMut(Entity)) {
        let children = self.child_list(entity);
        if children.is_empty() {
            visit(entity);
            return;
        }
        for child in children {
            self.collect_leaves(child, visit);
        }
    }
}
