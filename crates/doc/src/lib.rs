//! coralcraft-doc：多页文档模型（借鉴 refs/designcraft 页模型；区别于 vectorcraft 的画板模型）

#[cfg(test)]
mod tests {
    #[test]
    fn crate_links() {
        assert_eq!(env!("CARGO_PKG_NAME"), "coralcraft-doc");
    }
}
