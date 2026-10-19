//! coralcraft-ui-egui：egui 前端（独立 crate，可整体替换，M4 实装）

#[cfg(test)]
mod tests {
    #[test]
    fn crate_links() {
        assert_eq!(env!("CARGO_PKG_NAME"), "coralcraft-ui-egui");
    }
}
