//! Help → community and project links (the upstream ArtCraft Discord, this app's GitHub
//! repository). The commands return the URL; the frontend opens it.

use serde_json::{Value, json};

use super::*;

// CoralCraft: a fork of VectorCraft (storytold/vectorcraft); the app's home is its own
// repository. The Discord stays the upstream community, labeled as such.
pub const APP_ID: &str = "coralcraft";
pub const DISCORD_URL: &str = "https://discord.gg/artcraft";
pub const WEBSITE_URL: &str = "https://github.com/yupoet/coralcraft";

/// This app's home page (its repository; CoralCraft has no separate product site).
pub fn app_page_url() -> String {
    WEBSITE_URL.to_string()
}

/// This app's source repository.
pub fn github_url() -> String {
    format!("https://github.com/yupoet/{APP_ID}")
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "help.discord", "Join Our Discord", ["Help"], None, "{} → {url} the upstream ArtCraft community Discord", always, |_, _| url(DISCORD_URL.into())),
        cmd!(query "help.website", "CoralCraft Website", ["Help"], None, "{} → {url}", always, |_, _| url(WEBSITE_URL.into())),
        cmd!(query "help.appPage", "CoralCraft Website", ["Help"], None, "{} → {url} this app's page", always, |_, _| url(app_page_url())),
        cmd!(query "help.github", "CoralCraft on GitHub", ["Help"], None, "{} → {url} source code, issues and releases", always, |_, _| url(github_url())),
        cmd!(query "help.links", "Links", [], None, "{} → {discord, website, appPage, github}", always, |_, _| {
            Ok(json!({ "discord": DISCORD_URL, "website": WEBSITE_URL, "appPage": app_page_url(), "github": github_url() }))
        }),
    ]
}

fn url(u: String) -> Result<Value> {
    Ok(json!({ "url": u }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links() {
        let mut s = Session::new();
        assert_eq!(s.execute("help.discord", &json!({})).unwrap()["url"], "https://discord.gg/artcraft");
        assert_eq!(s.execute("help.appPage", &json!({})).unwrap()["url"], "https://github.com/yupoet/coralcraft");
        assert_eq!(s.execute("help.github", &json!({})).unwrap()["url"], "https://github.com/yupoet/coralcraft");
        assert_eq!(s.execute("help.links", &json!({})).unwrap()["website"], "https://github.com/yupoet/coralcraft");
    }
}
