// CoralCraft: the CorelDRAW core-tool mapping (P1 Task 2 / V4.3).
//
// The CorelDRAW core tools — the public user guide's toolbox names and order
// (<https://product.corel.com/help/CorelDRAW/Documentation-Windows/index.html>, "Toolbox") — map
// onto the synced VectorCraft tool ids; the `"CorelDRAW"` shortcut preset (`vectorcraft-ui-egui`
// `shortcut_editor`, V4.2) binds the same public shortcuts. The mapping is data only: the tools
// themselves, their interaction and their option keys stay VectorCraft's, so the workspace and the
// Property Bar build on `tool.select` / `tool.setOption` and the live-shape commands unchanged.

/// A CorelDRAW core tool: CoralCraft's id for it, the public guide's label (with "Tool", as the
/// VectorCraft catalog labels its tools), the VectorCraft tool it maps onto, and the public
/// shortcut the CorelDRAW preset binds (if it binds one).
pub struct CorelTool {
    pub id: &'static str,
    pub label: &'static str,
    pub vc: &'static str,
    pub shortcut: Option<&'static str>,
}

/// The core-tool mapping, in the public guide's toolbox order: Pick, Shape, the curve tools
/// (Freehand with Bézier), Rectangle, Ellipse, the Polygon group (Polygon with Spiral), Text,
/// Zoom, Pan.
pub const COREL_TOOLS: &[CorelTool] = &[
    CorelTool { id: "pick", label: "Pick Tool", vc: "selection", shortcut: None },
    CorelTool { id: "shape", label: "Shape Tool", vc: "directSelection", shortcut: Some("F10") },
    CorelTool { id: "freehand", label: "Freehand Tool", vc: "pencil", shortcut: Some("F5") },
    CorelTool { id: "bezier", label: "Bézier Tool", vc: "pen", shortcut: None },
    CorelTool { id: "rectangle", label: "Rectangle Tool", vc: "rectangle", shortcut: Some("F6") },
    CorelTool { id: "ellipse", label: "Ellipse Tool", vc: "ellipse", shortcut: Some("F7") },
    CorelTool { id: "polygon", label: "Polygon Tool", vc: "polygon", shortcut: Some("Y") },
    CorelTool { id: "spiral", label: "Spiral Tool", vc: "spiral", shortcut: Some("A") },
    CorelTool { id: "text", label: "Text Tool", vc: "type", shortcut: Some("F8") },
    CorelTool { id: "zoom", label: "Zoom Tool", vc: "zoom", shortcut: Some("Z") },
    CorelTool { id: "pan", label: "Pan Tool", vc: "hand", shortcut: Some("H") },
];

/// The core tool with CoralCraft id `id`.
pub fn corel_tool(id: &str) -> Option<&'static CorelTool> {
    COREL_TOOLS.iter().find(|t| t.id == id)
}

/// The core tool that the VectorCraft tool `vc` maps onto.
pub fn corel_for_vc(vc: &str) -> Option<&'static CorelTool> {
    COREL_TOOLS.iter().find(|t| t.vc == vc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_info;

    /// Every mapping row names a tool the VectorCraft catalog knows, both ids are unique, and the
    /// shortcuts are unique where given.
    #[test]
    fn corel_tools_map_onto_known_vectorcraft_tools() {
        assert_eq!(COREL_TOOLS.len(), 11, "the public guide's core-tool set");
        for t in COREL_TOOLS {
            assert!(tool_info(t.vc).is_some_and(|i| i.id == t.vc), "unknown VectorCraft tool `{}`", t.vc);
        }
        let ids: Vec<_> = COREL_TOOLS.iter().map(|t| t.id).collect();
        let vcs: Vec<_> = COREL_TOOLS.iter().map(|t| t.vc).collect();
        let keys: Vec<_> = COREL_TOOLS.iter().filter_map(|t| t.shortcut).collect();
        assert_eq!(ids.len(), ids.iter().collect::<std::collections::HashSet<_>>().len(), "duplicate CorelDRAW id");
        assert_eq!(vcs.len(), vcs.iter().collect::<std::collections::HashSet<_>>().len(), "duplicate VectorCraft tool");
        assert_eq!(keys.len(), keys.iter().collect::<std::collections::HashSet<_>>().len(), "duplicate shortcut");
    }

    /// Both lookups find their row and refuse everything else.
    #[test]
    fn corel_tool_lookups_round_trip() {
        assert_eq!(corel_tool("pick").unwrap().vc, "selection");
        assert_eq!(corel_tool("shape").unwrap().vc, "directSelection");
        assert_eq!(corel_tool("spiral").unwrap().vc, "spiral");
        assert!(corel_tool("crop").is_none());
        assert_eq!(corel_for_vc("pencil").unwrap().id, "freehand");
        assert_eq!(corel_for_vc("type").unwrap().id, "text");
        assert!(corel_for_vc("shaper").is_none());
    }
}
