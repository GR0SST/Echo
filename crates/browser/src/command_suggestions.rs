//! Address-field suggestions are separate from website cookies and history.
use std::time::Duration;
use browser_core::omnibox::{self, Resolved, SearchEngine, SiteSearch};
use gpui::Context;
use crate::window::{BrowserWindow, Target};

pub(crate) fn parse_suggestions(body: &str, query: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else { return Vec::new(); };
    if value[0].as_str() != Some(query) { return Vec::new(); }
    let Some(rows) = value[1].as_array() else { return Vec::new(); };
    let mut results = Vec::new();
    for row in rows {
        let Some(text) = row.as_str() else { continue; };
        if text.len() > 512 || text.trim().is_empty() || text.contains(char::is_control)
            || text == query || text.starts_with('=') || results.iter().any(|item| item == text) { continue; }
        results.push(text.to_string());
        if results.len() == 3 { break; }
    }
    results
}

fn fetch(engine: SearchEngine, query: &str) -> Vec<String> {
    // GPUI tests use synthetic rows below; never depend on live services.
    if cfg!(test) { return Vec::new(); }
    // A fresh cookie-free HTTP agent: no browsing cookies, referrer, page URL,
    // account identifier, persistent cache or profile storage is attached.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3))).build().into();
    let Ok(mut response) = agent.get(engine.suggest_url(query)).call() else { return Vec::new(); };
    let Ok(body) = response.body_mut().with_config().limit(64 * 1024).read_to_string() else { return Vec::new(); };
    parse_suggestions(&body, query)
}

impl BrowserWindow {
    pub(crate) fn scope_site_search(&mut self, cx: &mut Context<Self>) -> bool {
        let chatgpt_enabled = self.profile.handle.read_settings(|b| b.settings.max.enabled(echo_ai::MaxFeature::ChatGptCommandBar));
        let Some(bar) = &mut self.command_bar else { return false; };
        if !matches!(bar.target, Target::NewTab | Target::Tab(_)) || bar.site_search.is_some() { return false; }
        let Some(site) = SiteSearch::from_keyword(bar.input.read(cx).text()) else { return false; };
        if site == SiteSearch::ChatGpt && !chatgpt_enabled { return false; }
        bar.site_search = Some(site);
        bar.remote.clear();
        bar.remote_task.take();
        bar.remote_generation += 1;
        bar.selected = 0;
        let input = bar.input.clone();
        input.update(cx, |input, cx| {
            input.set_placeholder(if site == SiteSearch::ChatGpt { "Send a Message…" } else { "Search…" }, cx);
            input.set_caret_text("", cx);
        });
        cx.notify();
        true
    }

    pub(crate) fn update_command_suggestions(&mut self, cx: &mut Context<Self>) {
        let profile = self.browser.active_user_profile_id();
        let enabled = self.browser.active_user_profile().settings.search.suggestions && !self.browser.is_private();
        let engine = self.browser.active_user_profile().settings.search.engine;
        let enabled = enabled && engine.supports_suggestions();
        let Some(bar) = &mut self.command_bar else { return; };
        if !matches!(bar.target, Target::NewTab | Target::Tab(_)) { return; }
        let text = bar.input.read(cx).text().to_string();
        if bar.site_search.is_none() && text.ends_with(' ') && SiteSearch::from_keyword(&text) == Some(SiteSearch::Wikipedia) {
            self.scope_site_search(cx);
            return;
        }
        if bar.site_search.is_none() {
            if let Some((domain, _)) = omnibox::top_domain(&text) {
                bar.input.update(cx, |input, cx| input.complete_suffix(domain, cx));
            }
        }
        bar.remote.clear();
        bar.remote_task.take();
        bar.remote_generation += 1;
        let generation = bar.remote_generation;
        let query = bar.input.read(cx).text().to_string();
        if !enabled || bar.site_search.is_some() || query.len() < 2 || query.len() > 512
            || !matches!(omnibox::resolve(&query), Some(Resolved::Search { .. }))
            || query.contains(['/', '@', ':', '\\']) { cx.notify(); return; }
        let input = bar.input.clone();
        bar.remote_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(180)).await;
            // Recheck after debounce before transmitting anything.
            let allowed = this.read_with(cx, |this, cx| {
                !this.browser.is_private() && this.browser.active_user_profile_id() == profile
                    && this.browser.active_user_profile().settings.search.suggestions
                    && this.browser.active_user_profile().settings.search.engine == engine
                    && this.command_bar.as_ref().is_some_and(|bar| bar.input == input
                        && bar.remote_generation == generation && bar.query(cx) == query)
            }).unwrap_or(false);
            if !allowed { return; }
            let requested = query.clone();
            let rows = cx.background_executor().spawn(async move { fetch(engine, &requested) }).await;
            let _ = this.update(cx, |this, cx| {
                if this.browser.active_user_profile_id() != profile || this.browser.is_private()
                    || !this.browser.active_user_profile().settings.search.suggestions
                    || this.browser.active_user_profile().settings.search.engine != engine { return; }
                let Some(bar) = &mut this.command_bar else { return; };
                if bar.input != input || bar.remote_generation != generation || bar.query(cx) != query { return; }
                bar.remote = rows;
                cx.notify();
            });
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_payload_is_bounded_deduplicated_and_query_bound() {
        assert_eq!(parse_suggestions(r#"["rust",["rust","rust book","rust book",null,"rust docs","rust tutorial","rust more"]]"#, "rust"), ["rust book", "rust docs", "rust tutorial"]);
        assert!(parse_suggestions(r#"["old",["stale"]]"#, "new").is_empty());
        assert!(parse_suggestions("invalid", "rust").is_empty());
        assert!(parse_suggestions(r#"["2+2",["= 4"]]"#, "2+2").is_empty());
    }
}
