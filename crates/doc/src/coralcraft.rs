//! CoralCraft multi-page layer role and master/desktop layer metadata accessors on [`Document`].
//!
//! CorelDRAW layer roles (`Normal`, `MasterAllPages`, `MasterOddPages`, `MasterEvenPages`, `Desktop`,
//! `Guides`, `Grid`) and foreground/background master flags (`master_in_front`) live inside
//! `Document::unknown["coralcraft"]["layer_meta"]`, sharing the exact JSON schema produced by
//! `corel-import` (`CoralCraftLayerMeta`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{Document, NodeId};

/// Key in [`Document::unknown`] storing lossless CorelDRAW metadata and layer roles.
pub const CORALCRAFT_UNKNOWN_KEY: &str = "coralcraft";

/// Role of a layer in CorelDRAW's Object Manager (`Objects` Docker).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorelLayerRole {
    /// Standard per-page drawing layer.
    #[default]
    Normal,
    /// Master layer repeated on all pages (`1, 2, 3, ...`).
    MasterAllPages,
    /// Master layer repeated on even-numbered pages (`2, 4, 6, ...`).
    MasterEvenPages,
    /// Master layer repeated on odd-numbered pages (`1, 3, 5, ...`).
    MasterOddPages,
    /// Shared cross-page pasteboard (`Desktop`) layer where objects placed outside page bounds live.
    Desktop,
    /// Guidelines layer (non-printing by default).
    Guides,
    /// Document grid layer (non-printing).
    Grid,
    /// Forward-compatible fallback for unrecognized roles.
    #[serde(other)]
    Unknown,
}

impl CorelLayerRole {
    /// Parses a layer role string from engine commands or serialized JSON (`"normal"`,
    /// `"masterAll"` / `"master_all_pages"`, `"masterOdd"` / `"master_odd_pages"`,
    /// `"masterEven"` / `"master_even_pages"`, `"desktop"`, `"guides"`, `"grid"`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" | "page" => Some(Self::Normal),
            "master_all_pages" | "masterallpages" | "master_all" | "masterall" | "all" => Some(Self::MasterAllPages),
            "master_odd_pages" | "masteroddpages" | "master_odd" | "masterodd" | "odd" => Some(Self::MasterOddPages),
            "master_even_pages" | "masterevenpages" | "master_even" | "mastereven" | "even" => Some(Self::MasterEvenPages),
            "desktop" | "pasteboard" => Some(Self::Desktop),
            "guides" | "guide" => Some(Self::Guides),
            "grid" => Some(Self::Grid),
            _ => None,
        }
    }

    /// Canonical snake_case string used in `Document::unknown["coralcraft"]["layer_meta"]`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::MasterAllPages => "master_all_pages",
            Self::MasterEvenPages => "master_even_pages",
            Self::MasterOddPages => "master_odd_pages",
            Self::Desktop => "desktop",
            Self::Guides => "guides",
            Self::Grid => "grid",
            Self::Unknown => "unknown",
        }
    }

    /// Human-readable label for UI menus and Layer Options.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal (Page Layer)",
            Self::MasterAllPages => "Master (All Pages)",
            Self::MasterOddPages => "Master (Odd Pages)",
            Self::MasterEvenPages => "Master (Even Pages)",
            Self::Desktop => "Desktop (Shared Pasteboard)",
            Self::Guides => "Guides",
            Self::Grid => "Grid",
            Self::Unknown => "Unknown",
        }
    }

    /// Compact badge shown on a layer row in the Layers / Object Manager panel (`None` for `Normal`).
    #[must_use]
    pub const fn short_badge(self, master_in_front: bool) -> Option<&'static str> {
        match (self, master_in_front) {
            (Self::Normal | Self::Unknown, _) => None,
            (Self::MasterAllPages, false) => Some("M:All"),
            (Self::MasterAllPages, true) => Some("M:All\u{2191}"),
            (Self::MasterOddPages, false) => Some("M:Odd"),
            (Self::MasterOddPages, true) => Some("M:Odd\u{2191}"),
            (Self::MasterEvenPages, false) => Some("M:Even"),
            (Self::MasterEvenPages, true) => Some("M:Even\u{2191}"),
            (Self::Desktop, _) => Some("Desk"),
            (Self::Guides, _) => Some("Guides"),
            (Self::Grid, _) => Some("Grid"),
        }
    }

    /// Returns `true` if this role is a repeating Master layer (`All`, `Odd`, or `Even` pages).
    #[must_use]
    pub const fn is_master(self) -> bool {
        matches!(self, Self::MasterAllPages | Self::MasterOddPages | Self::MasterEvenPages)
    }

    /// Returns `true` if this role is a shared document-wide overlay (`Desktop`, `Guides`, or `Grid`).
    #[must_use]
    pub const fn is_desktop_or_overlay(self) -> bool {
        matches!(self, Self::Desktop | Self::Guides | Self::Grid)
    }

    /// Returns `true` if a Master layer with this role applies to 0-based `page_index`
    /// (`page_index == 0` is Page 1 = Odd; `page_index == 1` is Page 2 = Even).
    #[must_use]
    pub const fn applies_to_page(self, page_index: usize) -> bool {
        let page_number_1_based = page_index.saturating_add(1);
        match self {
            Self::MasterAllPages => true,
            Self::MasterOddPages => !page_number_1_based.is_multiple_of(2),
            Self::MasterEvenPages => page_number_1_based.is_multiple_of(2),
            Self::Normal | Self::Desktop | Self::Guides | Self::Grid | Self::Unknown => true,
        }
    }
}

/// Resolved CorelDRAW role and placement metadata for a layer node in [`Document`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorelLayerInfo {
    /// Original CorelDRAW `LayerId` (`u64`), defaulting to the `NodeId`.
    pub layer_id: u64,
    /// `Some(page_idx)` for a per-page normal layer, or `None` for a master/desktop/guide/grid layer.
    pub page_index: Option<usize>,
    /// CorelDRAW layer role.
    pub role: CorelLayerRole,
    /// When `true` on a master layer, renders in front of normal page layers instead of behind them.
    pub master_in_front: bool,
}

impl CorelLayerInfo {
    /// Default normal layer metadata for `id`.
    #[must_use]
    pub const fn default_for(id: NodeId) -> Self {
        Self { layer_id: id.0, page_index: None, role: CorelLayerRole::Normal, master_in_front: false }
    }
}

impl Document {
    /// Returns `true` if `self.unknown["coralcraft"]["layer_meta"]` is present and non-empty.
    #[must_use]
    pub fn has_corel_layer_roles(&self) -> bool {
        self.unknown.get(CORALCRAFT_UNKNOWN_KEY).and_then(|cc| cc.get("layer_meta")).and_then(Value::as_object).is_some_and(|m| !m.is_empty())
    }

    /// Returns the [`CorelLayerInfo`] for layer `id` (defaults to `CorelLayerRole::Normal` when unset).
    #[must_use]
    pub fn corel_layer_info(&self, id: NodeId) -> CorelLayerInfo {
        let mut info = CorelLayerInfo::default_for(id);
        let Some(entry) = self
            .unknown
            .get(CORALCRAFT_UNKNOWN_KEY)
            .and_then(|cc| cc.get("layer_meta"))
            .and_then(Value::as_object)
            .and_then(|m| m.get(&id.0.to_string()))
            .and_then(Value::as_object)
        else {
            return info;
        };
        if let Some(lid) = entry.get("layer_id").and_then(Value::as_u64) {
            info.layer_id = lid;
        }
        info.page_index = entry.get("page_index").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok());
        if let Some(role_str) = entry.get("role").and_then(Value::as_str) {
            info.role = CorelLayerRole::parse(role_str).unwrap_or(CorelLayerRole::Unknown);
        }
        if let Some(mif) = entry.get("master_in_front").and_then(Value::as_bool) {
            info.master_in_front = mif;
        }
        info
    }

    /// Updates the CorelDRAW layer metadata (`role`, `master_in_front`, `page_index`) for layer `id`
    /// inside `self.unknown["coralcraft"]["layer_meta"]`, preserving any unknown/residual fields.
    pub fn set_corel_layer_info(
        &mut self,
        id: NodeId,
        role: Option<CorelLayerRole>,
        master_in_front: Option<bool>,
        page_index: Option<Option<usize>>,
    ) {
        let cc_val = self.unknown.entry(CORALCRAFT_UNKNOWN_KEY.to_owned()).or_insert_with(|| json!({}));
        if !cc_val.is_object() {
            *cc_val = json!({});
        }
        let Some(cc_obj) = cc_val.as_object_mut() else { return };
        let lm_val = cc_obj.entry("layer_meta".to_owned()).or_insert_with(|| json!({}));
        if !lm_val.is_object() {
            *lm_val = json!({});
        }
        let Some(lm_obj) = lm_val.as_object_mut() else { return };
        let key = id.0.to_string();
        let entry_val = lm_obj.entry(key).or_insert_with(|| {
            json!({
                "layer_id": id.0,
                "page_index": null,
                "role": "normal",
                "master_in_front": false,
            })
        });
        if !entry_val.is_object() {
            *entry_val = json!({
                "layer_id": id.0,
                "page_index": null,
                "role": "normal",
                "master_in_front": false,
            });
        }
        let Some(entry_obj) = entry_val.as_object_mut() else { return };
        entry_obj.entry("layer_id".to_owned()).or_insert_with(|| Value::from(id.0));
        if let Some(r) = role {
            entry_obj.insert("role".to_owned(), Value::String(r.as_str().to_owned()));
            if (r.is_master() || r.is_desktop_or_overlay()) && page_index.is_none() {
                entry_obj.insert("page_index".to_owned(), Value::Null);
            }
        }
        if let Some(mif) = master_in_front {
            entry_obj.insert("master_in_front".to_owned(), Value::Bool(mif));
        }
        if let Some(pi) = page_index {
            entry_obj.insert("page_index".to_owned(), pi.map_or(Value::Null, Value::from));
        }
    }

    /// Copies CorelDRAW layer role metadata from layer `from` to duplicated layer `to` if present.
    pub fn copy_corel_layer_info(&mut self, from: NodeId, to: NodeId) {
        let Some(lm_obj) = self
            .unknown
            .get_mut(CORALCRAFT_UNKNOWN_KEY)
            .and_then(Value::as_object_mut)
            .and_then(|cc| cc.get_mut("layer_meta"))
            .and_then(Value::as_object_mut)
        else {
            return;
        };
        if let Some(mut cloned) = lm_obj.get(&from.0.to_string()).cloned() {
            if let Some(obj) = cloned.as_object_mut() {
                obj.insert("layer_id".to_owned(), Value::from(to.0));
            }
            lm_obj.insert(to.0.to_string(), cloned);
        }
    }

    /// Removes CorelDRAW layer metadata for deleted layer `ids`.
    pub fn remove_corel_layer_info(&mut self, ids: &[NodeId]) {
        let Some(lm_obj) = self
            .unknown
            .get_mut(CORALCRAFT_UNKNOWN_KEY)
            .and_then(Value::as_object_mut)
            .and_then(|cc| cc.get_mut("layer_meta"))
            .and_then(Value::as_object_mut)
        else {
            return;
        };
        for id in ids {
            lm_obj.remove(&id.0.to_string());
        }
        let _ = Map::len(lm_obj);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corel_layer_role_page_matching_and_roundtrip() {
        assert!(CorelLayerRole::MasterAllPages.applies_to_page(0));
        assert!(CorelLayerRole::MasterAllPages.applies_to_page(1));
        assert!(CorelLayerRole::MasterAllPages.applies_to_page(2));

        // 0-based page 0 is Page 1 (Odd), page 1 is Page 2 (Even), page 2 is Page 3 (Odd).
        assert!(CorelLayerRole::MasterOddPages.applies_to_page(0));
        assert!(!CorelLayerRole::MasterOddPages.applies_to_page(1));
        assert!(CorelLayerRole::MasterOddPages.applies_to_page(2));

        assert!(!CorelLayerRole::MasterEvenPages.applies_to_page(0));
        assert!(CorelLayerRole::MasterEvenPages.applies_to_page(1));
        assert!(!CorelLayerRole::MasterEvenPages.applies_to_page(2));

        let mut doc = Document::new(200.0, 200.0);
        let lid = doc.layers[0].id;
        assert!(!doc.has_corel_layer_roles());
        assert_eq!(doc.corel_layer_info(lid).role, CorelLayerRole::Normal);

        doc.set_corel_layer_info(lid, Some(CorelLayerRole::MasterOddPages), Some(true), None);
        assert!(doc.has_corel_layer_roles());
        let info = doc.corel_layer_info(lid);
        assert_eq!(info.role, CorelLayerRole::MasterOddPages);
        assert!(info.master_in_front);
        assert_eq!(info.page_index, None);
    }
}
