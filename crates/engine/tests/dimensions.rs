//! Linked measurements, history, portable artwork and hostile command input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use serde_json::{Value, json};
use vectorcraft_engine::{
    Session, ViewInfo,
    doc::{Document, NodeId, NodeKind},
    tools::{Mods, PointerEvent, PointerKind, ToolKey},
};

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width":600,"height":500})).unwrap();
    s
}
fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap()
}
fn edge(s: &mut Session) -> u64 {
    run(s, "shape.line", json!({"x1":100,"y1":100,"x2":172,"y2":100}))["id"].as_u64().unwrap()
}
fn dim(s: &mut Session, node: u64) -> u64 {
    run(
        s,
        "dimension.create",
        json!({"source":{"kind":"segment","edge":{"node":node,"subpath":0,"segment":0}},"options":{"units":"in","precision":2,"scale":"1:10"}}),
    )["id"]
        .as_u64()
        .unwrap()
}
fn text(s: &Session, id: u64) -> String {
    let n = s.doc().unwrap().doc.node(NodeId(id)).unwrap();
    n.children().unwrap().iter().find_map(|n| if let NodeKind::Text(t) = &n.kind { Some(t.plain_text()) } else { None }).unwrap()
}
fn recipe(s: &Session, id: u64) -> vectorcraft_doc::dimension::Dimension {
    (**s.doc().unwrap().doc.node(NodeId(id)).unwrap().extras().dimension.as_ref().unwrap()).clone()
}
fn ev(s: &mut Session, kind: PointerKind, x: f64, y: f64) {
    s.pointer(&PointerEvent::new(kind, x, y), ViewInfo::default()).unwrap();
}

#[test]
fn dimension_length_scale_live_edit_undo_and_roundtrip() {
    let mut s = session();
    let line = edge(&mut s);
    let id = dim(&mut s, line);
    assert_eq!(text(&s, id), "10.00 in");
    run(&mut s, "select.set", json!({"ids":[line]}));
    run(&mut s, "object.scale", json!({"sx":200,"sy":100}));
    assert_eq!(text(&s, id), "20.00 in");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(text(&s, id), "10.00 in");
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(text(&s, id), "20.00 in");
    let json = serde_json::to_string(&*s.doc().unwrap().doc).unwrap();
    let doc: Document = serde_json::from_str(&json).unwrap();
    assert!(doc.dimensions, "saved with the document");
    let mut reopened = Session::new();
    reopened.add_document(doc, None);
    run(&mut reopened, "select.set", json!({"ids":[line]}));
    run(&mut reopened, "object.scale", json!({"sx":50,"sy":100}));
    assert_eq!(text(&reopened, id), "10.00 in");
    run(&mut reopened, "edit.clear", json!({}));
    assert!(reopened.doc().unwrap().doc.node(NodeId(id)).is_none());
    run(&mut reopened, "edit.undo", json!({}));
    assert_eq!(text(&reopened, id), "10.00 in");
}

/// Documents without dimensions skip the refresh, and lose the flag when the last goes.
#[test]
fn documents_without_dimensions_skip_the_refresh() {
    let mut s = session();
    let line = edge(&mut s);
    assert!(!s.doc().unwrap().doc.dimensions);
    let id = dim(&mut s, line);
    assert!(s.doc().unwrap().doc.dimensions);
    run(&mut s, "select.set", json!({"ids":[id]}));
    run(&mut s, "edit.clear", json!({}));
    run(&mut s, "select.set", json!({"ids":[line]}));
    run(&mut s, "object.move", json!({"dx":1,"dy":0}));
    assert!(!s.doc().unwrap().doc.dimensions);
}

#[test]
fn apply_to_all_restyles_every_dimension_in_one_undo_step() {
    let mut s = session();
    let line = edge(&mut s);
    let first = dim(&mut s, line);
    let second = run(&mut s, "dimension.create", json!({"a":[0,0],"b":[72,0],"options":{"units":"pt","precision":0}}))["id"].as_u64().unwrap();
    assert_eq!(text(&s, second), "72 pt");
    run(&mut s, "dimension.applyToAll", json!({"id":first}));
    assert_eq!(text(&s, second), "10.00 in", "the first dimension's style, its scale too");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(text(&s, second), "72 pt");
    assert!(s.execute("dimension.applyToAll", &json!({"id":line})).is_err(), "not a dimension");
}

#[test]
fn dimension_angles_circles_arcs_and_curves() {
    let mut s = session();
    let a = run(
        &mut s,
        "dimension.create",
        json!({"mode":"angular","a":[100,0],"vertex":[0,0],"b":[0,100],"options":{"precision":1,"scale":"1:10"}}),
    )["id"]
        .as_u64()
        .unwrap();
    assert_eq!(text(&s, a), "90.0°");
    let circle = run(&mut s, "shape.ellipse", json!({"x":100,"y":100,"width":144,"height":144}))["id"].as_u64().unwrap();
    for (mode, want) in [("radius", "1.0 in"), ("diameter", "2.0 in")] {
        let id = run(
            &mut s,
            "dimension.create",
            json!({"mode":mode,"source":{"kind":"circle","node":circle},"options":{"units":"in","precision":1}}),
        )["id"]
            .as_u64()
            .unwrap();
        assert_eq!(text(&s, id), want);
    }
    // One quarter of the circle is a perfect arc.
    let id = run(
        &mut s,
        "dimension.create",
        json!({"mode":"radius","source":{"kind":"arc","edge":{"node":circle,"subpath":0,"segment":0}},"options":{"units":"in","precision":1}}),
    )["id"]
        .as_u64()
        .unwrap();
    assert_eq!(text(&s, id), "1.0 in");
    // A curve that isn't an arc: no radius, and its length is measured end to end.
    let curve = run(&mut s, "path.create", json!({"d":"M 0 0 C 0 100 100 100 100 0"}))["id"].as_u64().unwrap();
    let arc = json!({"mode":"radius","source":{"kind":"arc","edge":{"node":curve,"subpath":0,"segment":0}}});
    assert!(s.execute("dimension.create", &arc).is_err());
    let id = run(
        &mut s,
        "dimension.create",
        json!({"source":{"kind":"segment","edge":{"node":curve,"subpath":0,"segment":0}},"options":{"units":"pt","precision":0}}),
    )["id"]
        .as_u64()
        .unwrap();
    assert_eq!(text(&s, id), "100 pt");
}

#[test]
fn dimension_rejects_bad_input_without_mutation() {
    let mut s = session();
    let line = edge(&mut s);
    let id = dim(&mut s, line);
    let original = serde_json::to_value(&*s.doc().unwrap().doc).unwrap();
    for p in [
        json!({}),
        json!({"a":[0,0],"b":[0,0]}),
        json!({"a":[0,0],"b":[1e100,0]}),
        json!({"a":[0,0],"b":[10,0],"options":{"precision":99999}}),
        json!({"a":[0,0],"b":[10,0],"options":{"scale":"0:10"}}),
        json!({"a":[0,0],"b":[10,0],"options":{"scale":"1:NaN"}}),
        json!({"a":[0,0],"b":[10,0],"options":{"units":"furlongs"}}),
        json!({"a":[0,0],"b":[10,0],"options":{"textPosition":"sideways"}}),
        json!({"a":[0,0],"b":[10,0],"mode":"spiral"}),
        json!({"source":{"kind":"segment","edge":{"node":line,"subpath":0,"segment":18446744073709551615u64}}}),
        json!({"mode":"angular","source":{"kind":"segment","edge":{"node":line,"subpath":0,"segment":0}}}),
    ] {
        assert!(s.execute("dimension.create", &p).is_err(), "{p}");
        assert_eq!(serde_json::to_value(&*s.doc().unwrap().doc).unwrap(), original);
    }
    assert!(s.execute("dimension.update", &json!({"ids":[id],"options":{"fontSize":-1}})).is_err());
    assert_eq!(serde_json::to_value(&*s.doc().unwrap().doc).unwrap(), original);
}

/// Hover a segment and drag outwards: releasing places the dimension; Escape cancels the drag.
#[test]
fn dragging_out_from_a_segment_places_its_dimension() {
    let mut s = session();
    edge(&mut s);
    s.select_tool("dimension", ViewInfo::default()).unwrap();
    let before = s.doc().unwrap().doc.node_count();
    ev(&mut s, PointerKind::Move, 130.0, 100.0);
    ev(&mut s, PointerKind::Down, 130.0, 100.0);
    ev(&mut s, PointerKind::Drag, 130.0, 70.0);
    assert!(s.in_interaction());
    s.tool_key(ToolKey::Escape, Mods::default(), ViewInfo::default()).unwrap();
    assert_eq!(s.doc().unwrap().doc.node_count(), before);
    assert!(!s.in_interaction());
    ev(&mut s, PointerKind::Up, 130.0, 70.0);
    ev(&mut s, PointerKind::Down, 130.0, 100.0);
    ev(&mut s, PointerKind::Drag, 130.0, 80.0);
    ev(&mut s, PointerKind::Drag, 130.0, 60.0);
    ev(&mut s, PointerKind::Up, 130.0, 60.0);
    assert!(!s.in_interaction());
    assert!(s.doc().unwrap().doc.node_count() > before);
    let id = s.doc().unwrap().selection.objects[0].0;
    assert_eq!(text(&s, id), "72.00 pt");
    assert!((recipe(&s, id).offset.abs() - 40.0).abs() < 1e-6, "placed where it was released");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc().unwrap().doc.node_count(), before);
}

/// Plain clicks pick two points, and a third click places the dimension.
#[test]
fn clicking_two_points_then_placing_measures_between_them() {
    let mut s = session();
    s.select_tool("dimension", ViewInfo::default()).unwrap();
    for (x, y) in [(300.0, 300.0), (372.0, 300.0)] {
        ev(&mut s, PointerKind::Down, x, y);
        ev(&mut s, PointerKind::Up, x, y);
    }
    ev(&mut s, PointerKind::Move, 336.0, 330.0);
    assert!(s.in_interaction());
    ev(&mut s, PointerKind::Down, 336.0, 330.0);
    ev(&mut s, PointerKind::Up, 336.0, 330.0);
    assert!(!s.in_interaction());
    let id = s.doc().unwrap().selection.objects[0].0;
    assert_eq!(text(&s, id), "72.00 pt");
}

/// Dragging out from a corner of two straight segments measures the angle between them.
#[test]
fn dragging_out_from_a_corner_measures_its_angle() {
    let mut s = session();
    run(&mut s, "shape.rectangle", json!({"x":100,"y":100,"width":100,"height":50}));
    s.select_tool("dimension", ViewInfo::default()).unwrap();
    s.set_tool_option_cmd(&json!({"tool":"dimension","values":{"mode":"angular"}})).unwrap();
    ev(&mut s, PointerKind::Move, 100.0, 100.0);
    ev(&mut s, PointerKind::Down, 100.0, 100.0);
    ev(&mut s, PointerKind::Drag, 120.0, 120.0);
    ev(&mut s, PointerKind::Drag, 130.0, 130.0);
    ev(&mut s, PointerKind::Up, 130.0, 130.0);
    let id = s.doc().unwrap().selection.objects[0].0;
    assert_eq!(text(&s, id), "90.00°");
}

/// Dragging out from a circle gives its radius; dragging inwards, its diameter.
#[test]
fn dragging_from_a_circle_gives_its_radius_or_diameter() {
    let mut s = session();
    run(&mut s, "shape.ellipse", json!({"x":100,"y":100,"width":144,"height":144}));
    s.select_tool("dimension", ViewInfo::default()).unwrap();
    s.set_tool_option_cmd(&json!({"tool":"dimension","values":{"mode":"radial","style":{"units":"in","precision":0}}})).unwrap();
    for ((x0, y0), (x1, y1), want) in [((244.0, 172.0), (280.0, 172.0), "1 in"), ((244.0, 172.0), (200.0, 172.0), "2 in")] {
        ev(&mut s, PointerKind::Move, x0, y0);
        ev(&mut s, PointerKind::Down, x0, y0);
        ev(&mut s, PointerKind::Drag, (x0 + x1) / 2.0, y0);
        ev(&mut s, PointerKind::Drag, x1, y1);
        ev(&mut s, PointerKind::Up, x1, y1);
        let id = s.doc().unwrap().selection.objects[0].0;
        assert_eq!(text(&s, id), want);
    }
}

#[test]
fn dimension_anchor_links_and_native_file_survive_source_moves() {
    let mut s = session();
    let first = edge(&mut s);
    let second = edge(&mut s);
    let id=run(&mut s,"dimension.create",json!({"a":[100,100],"b":[172,100],"source":{"kind":"points","a":{"node":first,"subpath":0,"anchor":0},"b":{"node":second,"subpath":0,"anchor":1},"vertex":null},"options":{"units":"pt","precision":0}}))["id"].as_u64().unwrap();
    assert_eq!(text(&s, id), "72 pt");
    let bytes = vectorcraft_format::save(&s.doc().unwrap().doc, false);
    let doc = vectorcraft_format::load(&bytes).unwrap();
    let mut s = Session::new();
    s.add_document(doc, None);
    run(&mut s, "select.set", json!({"ids":[second]}));
    run(&mut s, "object.move", json!({"dx":28,"dy":0}));
    assert_eq!(text(&s, id), "100 pt");
}

/// A linked dimension moved on its own keeps its new place when its art changes later; moved with
/// its art, it keeps its distance from it.
#[test]
fn a_moved_linked_dimension_stays_where_it_was_dropped() {
    let mut s = session();
    let line = edge(&mut s);
    let id = dim(&mut s, line);
    let offset = recipe(&s, id).offset;
    run(&mut s, "select.set", json!({"ids":[id]}));
    run(&mut s, "object.move", json!({"dx":0,"dy":-20}));
    let moved = recipe(&s, id).offset;
    assert!((moved - offset).abs() - 20.0 < 1e-6 && (moved - offset).abs() > 19.0, "{offset} → {moved}");
    run(&mut s, "select.set", json!({"ids":[line]}));
    run(&mut s, "object.scale", json!({"sx":200,"sy":100}));
    assert!((recipe(&s, id).offset - moved).abs() < 1e-6, "no jump back");
    run(&mut s, "select.set", json!({"ids":[line, id]}));
    run(&mut s, "object.move", json!({"dx":0,"dy":50}));
    assert!((recipe(&s, id).offset - moved).abs() < 1e-6, "moved together");
}

#[test]
fn dimension_options_are_persistent_and_invalid_updates_report_errors() {
    let mut s = session();
    s.set_tool_option_cmd(&json!({"tool":"dimension","values":{"style":{"scale":"10:1","units":"mm","precision":3},"mode":"radial"}})).unwrap();
    assert_eq!(s.tool_options_of("dimension")["style"]["scale"], "10:1");
    assert!(s.set_tool_option_cmd(&json!({"tool":"dimension","values":{"style":{"scale":"1:0"}}})).is_err());
    let o: vectorcraft_doc::dimension::DimensionOptions = serde_json::from_value(s.tool_options_of("dimension")["style"].clone()).unwrap();
    assert_eq!(o.label(72.0, false).unwrap(), "2.540 mm");
    let picas: vectorcraft_doc::dimension::DimensionOptions = serde_json::from_value(json!({"units":"pc","precision":1})).unwrap();
    assert_eq!(picas.label(72.0, false).unwrap(), "6.0 pc");
    let prefs = serde_json::to_value(&s.prefs).unwrap();
    let mut fresh = Session::new();
    fresh.prefs = serde_json::from_value(prefs).unwrap();
    assert_eq!(fresh.tool_options_of("dimension")["style"], s.tool_options_of("dimension")["style"]);
}

#[test]
fn dimension_unlinked_geometry_scales_and_updates_its_label() {
    let mut s = session();
    let id = run(&mut s, "dimension.create", json!({"a":[100,100],"b":[172,100],"options":{"units":"in","precision":0}}))["id"].as_u64().unwrap();
    run(&mut s, "object.scale", json!({"sx":200,"sy":200}));
    assert_eq!(text(&s, id), "2 in");
}

#[test]
fn dimension_linked_angle_and_circle_update_and_reject_non_circles() {
    let mut s = session();
    let first = edge(&mut s);
    let second = run(&mut s, "shape.line", json!({"x1":100,"y1":100,"x2":100,"y2":172}))["id"].as_u64().unwrap();
    let id=run(&mut s,"dimension.create",json!({"mode":"angular","source":{"kind":"angle","first":{"node":first,"subpath":0,"segment":0},"second":{"node":second,"subpath":0,"segment":0}},"options":{"precision":1}}))["id"].as_u64().unwrap();
    assert_eq!(text(&s, id), "90.0°");
    run(&mut s, "select.set", json!({"ids":[second]}));
    run(&mut s, "object.rotate", json!({"angle":45,"origin":[100,100]}));
    assert!(["45.0°", "135.0°"].contains(&text(&s, id).as_str()));
    let circle = run(&mut s, "shape.ellipse", json!({"x":100,"y":100,"width":144,"height":144}))["id"].as_u64().unwrap();
    let id = run(
        &mut s,
        "dimension.create",
        json!({"mode":"diameter","source":{"kind":"circle","node":circle},"options":{"units":"in","precision":0}}),
    )["id"]
        .as_u64()
        .unwrap();
    run(&mut s, "select.set", json!({"ids":[circle]}));
    run(&mut s, "object.scale", json!({"sx":200,"sy":200}));
    assert_eq!(text(&s, id), "4 in");
    run(&mut s, "object.scale", json!({"sx":200,"sy":100}));
    assert!(s.doc().unwrap().doc.node(NodeId(id)).is_none());
    assert!(s.execute("dimension.create", &json!({"mode":"radius","source":{"kind":"circle","node":circle}})).is_err());
}

#[test]
fn dimension_style_update_keeps_links_and_can_be_undone() {
    let mut s = session();
    let line = edge(&mut s);
    let id = dim(&mut s, line);
    run(
        &mut s,
        "dimension.update",
        json!({"ids":[id],"offset":-42,"options":{"units":"mm","precision":2,"hideUnits":true,"textPosition":"between","textAlignment":"right","lineColor":[10,20,30]}}),
    );
    assert_eq!(text(&s, id), "25.40");
    let d = recipe(&s, id);
    assert!(d.source.is_some());
    assert_eq!(d.offset, -42.0);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(text(&s, id), "10.00 in");
}

#[test]
fn dimension_cannot_measure_its_own_generated_art() {
    let mut s = session();
    let line = edge(&mut s);
    let id = dim(&mut s, line);
    let child = s.doc().unwrap().doc.node(NodeId(id)).unwrap().children().unwrap()[0].id;
    assert!(s.execute("dimension.create", &json!({"source":{"kind":"segment","edge":{"node":child,"subpath":0,"segment":0}}})).is_err());
}

#[test]
fn unknown_dimension_options_are_refused_by_commands_but_a_newer_file_still_opens() {
    let mut s = session();
    let line = edge(&mut s);
    let source = json!({"kind":"segment","edge":{"node":line,"subpath":0,"segment":0}});
    let bad = s.execute("dimension.create", &json!({"source":source,"options":{"units":"in","color":"red"}}));
    assert!(bad.unwrap_err().to_string().contains("Unknown dimension option color"));
    assert!(s.set_tool_option_cmd(&json!({"tool":"dimension","values":{"style":{"precison":2}}})).is_err());
    let id = dim(&mut s, line);
    let saved = vectorcraft_format::save(&s.doc().unwrap().doc, false);
    let mut v: Value = serde_json::from_slice(&saved).unwrap();
    // A later version's dimension carries fields this one doesn't know.
    fn add_unknown(v: &mut Value) {
        match v {
            Value::Object(m) => {
                if let Some(Value::Object(d)) = m.get_mut("dimension") {
                    d.insert("later".into(), json!(1));
                    if let Some(Value::Object(o)) = d.get_mut("options") {
                        o.insert("later".into(), json!(true));
                    }
                }
                m.values_mut().for_each(add_unknown);
            }
            Value::Array(a) => a.iter_mut().for_each(add_unknown),
            _ => {}
        }
    }
    add_unknown(&mut v);
    let doc = vectorcraft_format::load(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(doc.node(NodeId(id)).unwrap().extras().dimension.is_some());
}

/// With Shift down, the dashed line from the first picked point follows the 45-degree step the
/// second click will pick (#2432), not the raw pointer.
#[test]
fn shift_shows_the_constrained_line_while_picking_the_second_point() {
    use vectorcraft_engine::tools::Overlay;
    let mut s = session();
    s.select_tool("dimension", ViewInfo::default()).unwrap();
    ev(&mut s, PointerKind::Down, 300.0, 300.0);
    ev(&mut s, PointerKind::Up, 300.0, 300.0);
    let end = |s: &mut Session, shift: bool| {
        let e = PointerEvent::new(PointerKind::Move, 372.0, 310.0).with_mods(Mods { shift, ..Mods::default() });
        s.pointer(&e, ViewInfo::default()).unwrap();
        s.overlays(ViewInfo::default()).into_iter().find_map(|o| if let Overlay::Line { b, .. } = o { Some(b) } else { None }).unwrap()
    };
    assert_eq!(end(&mut s, false).y, 310.0);
    assert!((end(&mut s, true).y - 300.0).abs() < 1e-9);
}

/// A click near a line, away from its anchors, snaps to the closest point of the line (#2432).
#[test]
fn clicking_near_a_line_snaps_the_measured_point_onto_it() {
    let mut s = session();
    edge(&mut s);
    s.select_tool("dimension", ViewInfo::default()).unwrap();
    for (x, y) in [(130.0, 103.0), (150.0, 97.0), (140.0, 130.0)] {
        ev(&mut s, PointerKind::Down, x, y);
        ev(&mut s, PointerKind::Up, x, y);
    }
    let id = s.doc().unwrap().selection.objects[0].0;
    assert_eq!(text(&s, id), "20.00 pt");
}

/// Both picked points snap onto anchors a few pixels away, the rubber band and a ring show the
/// snap before the click, and the dimension stays linked to the anchors (#2516).
#[test]
fn both_points_snap_onto_anchors_with_a_visible_indicator() {
    use vectorcraft_engine::tools::Overlay;
    let mut s = session();
    let line = edge(&mut s);
    s.select_tool("dimension", ViewInfo::default()).unwrap();
    ev(&mut s, PointerKind::Move, 103.0, 102.0);
    let ring = |s: &mut Session| {
        s.overlays(ViewInfo::default())
            .into_iter()
            .any(|o| matches!(o, Overlay::Anchor { p, size, .. } if size > 8.0 && p.x == 100.0 && p.y == 100.0))
    };
    assert!(ring(&mut s), "ring on the anchor before the first click");
    ev(&mut s, PointerKind::Down, 103.0, 102.0);
    ev(&mut s, PointerKind::Up, 103.0, 102.0);
    ev(&mut s, PointerKind::Move, 169.0, 98.0);
    let end = s.overlays(ViewInfo::default()).into_iter().find_map(|o| if let Overlay::Line { b, .. } = o { Some(b) } else { None }).unwrap();
    assert_eq!((end.x, end.y), (172.0, 100.0), "rubber band ends on the anchor");
    ev(&mut s, PointerKind::Down, 169.0, 98.0);
    ev(&mut s, PointerKind::Up, 169.0, 98.0);
    ev(&mut s, PointerKind::Down, 136.0, 130.0);
    ev(&mut s, PointerKind::Up, 136.0, 130.0);
    let id = s.doc().unwrap().selection.objects[0].0;
    assert_eq!(text(&s, id), "72.00 pt");
    run(&mut s, "select.set", json!({"ids":[line]}));
    run(&mut s, "object.scale", json!({"sx":200,"sy":100}));
    assert_eq!(text(&s, id), "144.00 pt", "linked to both anchors");
}
