//! CoralCraft desktop application entry point (M0 stub).
//! Wires up `coralcraft-ui-egui` + `coralcraft-render` starting in M4; currently prints version for smoke testing.

fn main() {
    println!("CoralCraft {} (pre-alpha skeleton; UI lands in M4)", env!("CARGO_PKG_VERSION"));
}
