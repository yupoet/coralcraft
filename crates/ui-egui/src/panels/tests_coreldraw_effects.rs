//! CoralCraft: signature-effect workflows over the shared panels and menu commands (P3).

use serde_json::json;
use vectorcraft_doc::{NodeId, NodeKind};

use super::tests_appearance::{app_with_rect, run};

#[test]
fn coreldraw_object_menu_runs_the_shared_powerclip_lifecycle() {
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let contents = app.session.active().unwrap().selection.objects[0];
    let frame = NodeId(run(&mut app, "shape.ellipse", json!({"x": 30, "y": 20, "width": 45, "height": 40}))["id"].as_u64().unwrap());
    run(&mut app, "select.set", json!({"ids": [contents.0, frame.0]}));

    let tree = crate::menus::menu_tree();
    let object = tree.iter().find(|(title, _)| *title == "Object").map(|(_, items)| items).unwrap();
    let clip = object
        .iter()
        .find_map(|item| match item {
            crate::menus::Item::Sub("Clipping Mask", items) => Some(items),
            _ => None,
        })
        .expect("Object menu has the clipping submenu");
    for command in ["object.clippingMask.make", "object.clippingMask.release", "object.clippingMask.editContents", "object.clippingMask.editMask"] {
        assert!(clip.iter().any(|item| matches!(item, crate::menus::Item::Cmd(_, id, _) if *id == command)), "{command} in {clip:?}");
    }

    crate::menus::invoke(&mut app, "object.clippingMask.make", json!({}));
    let group = app.session.active().unwrap().selection.objects[0];
    assert!(matches!(app.session.active().unwrap().doc.node(group).unwrap().kind, NodeKind::Group { clip: true, .. }));
    crate::menus::invoke(&mut app, "object.clippingMask.editContents", json!({}));
    assert_eq!(app.session.active().unwrap().selection.objects, vec![contents]);
    crate::menus::invoke(&mut app, "object.clippingMask.editMask", json!({}));
    assert_eq!(app.session.active().unwrap().selection.objects, vec![frame]);
    run(&mut app, "select.set", json!({"ids": [group.0]}));
    crate::menus::invoke(&mut app, "object.clippingMask.release", json!({}));
    assert!(matches!(app.session.active().unwrap().doc.node(group).unwrap().kind, NodeKind::Group { clip: false, .. }));
    assert!(app.session.active().unwrap().doc.node(contents).is_some());
    assert!(app.session.active().unwrap().doc.node(frame).is_some());
}
