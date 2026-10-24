//! Install-local Air Traffic Control. Rules are evaluated in their stored order.
use crate::{Browser, SpaceId, UserProfileId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkMatch {
    Contains(String),
    IsEqualTo(String),
    Domain(String),
    SourceApplication(String),
}
impl LinkMatch {
    pub fn matches(&self, url: &url::Url, source: Option<&str>) -> bool {
        match self {
            Self::Contains(value) => !value.trim().is_empty() && url.as_str().contains(value),
            Self::IsEqualTo(value) => {
                if value.trim().is_empty() {
                    return false;
                }
                // Arc accepts a bare domain in its URL "Is equal to" field.
                if value.contains("://") {
                    url::Url::parse(value).is_ok_and(|expected| expected == *url)
                } else {
                    domain_matches(url, value, false)
                }
            }
            Self::Domain(value) => domain_matches(url, value, true),
            Self::SourceApplication(value) => {
                !value.is_empty() && source.is_some_and(|source| source.eq_ignore_ascii_case(value))
            }
        }
    }
}
fn domain_matches(url: &url::Url, value: &str, subdomains: bool) -> bool {
    let value = value.trim().trim_end_matches('.').to_ascii_lowercase();
    if value.is_empty() || value.contains(['/', '@', ':', '?', '#']) {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.');
    host.eq_ignore_ascii_case(&value)
        || subdomains
            && host
                .to_ascii_lowercase()
                .strip_suffix(&value)
                .is_some_and(|prefix| prefix.ends_with('.'))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkDestination {
    Space(SpaceId),
    #[default]
    MostRecentSpace,
    LittleWindow,
    Profile(UserProfileId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRule {
    pub id: u64,
    #[serde(default = "crate::default_enabled")]
    pub enabled: bool,
    pub condition: LinkMatch,
    pub destination: LinkDestination,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AirTrafficControl {
    pub rules: Vec<LinkRule>,
    pub default_destination: LinkDestination,
    next_id: u64,
}
impl AirTrafficControl {
    pub fn add(&mut self, condition: LinkMatch, destination: LinkDestination) -> u64 {
        self.next_id = self
            .next_id
            .max(self.rules.iter().map(|rule| rule.id).max().unwrap_or(0))
            .checked_add(1)
            .expect("route id exhausted");
        let id = self.next_id;
        self.rules.push(LinkRule {
            id,
            enabled: true,
            condition,
            destination,
        });
        id
    }
    pub fn remove(&mut self, id: u64) {
        self.rules.retain(|rule| rule.id != id);
    }
    pub fn move_rule(&mut self, id: u64, index: usize) -> bool {
        let Some(from) = self.rules.iter().position(|rule| rule.id == id) else {
            return false;
        };
        let rule = self.rules.remove(from);
        self.rules.insert(index.min(self.rules.len()), rule);
        true
    }
    pub fn resolve(
        &self,
        browser: &Browser,
        input: &str,
        source: Option<&str>,
    ) -> Option<LinkDestination> {
        let url = url::Url::parse(input).ok()?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return None;
        }
        let exists = |destination: LinkDestination| match destination {
            LinkDestination::Space(id) => browser.spaces.iter().any(|space| space.id == id),
            LinkDestination::Profile(id) => browser
                .spaces
                .iter()
                .any(|space| space.user_profile_id == id),
            _ => true,
        };
        self.rules
            .iter()
            .find(|rule| {
                rule.enabled && exists(rule.destination) && rule.condition.matches(&url, source)
            })
            .map(|rule| rule.destination)
            .or(Some(if exists(self.default_destination) {
                self.default_destination
            } else {
                LinkDestination::MostRecentSpace
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_enabled_rules_are_install_local_and_stale_targets_fall_through() {
        let mut browser = Browser::new();
        let space = browser.add_space("Work", 20.);
        let profile = browser.create_user_profile("Other").unwrap();
        browser.assign_space_user_profile(space, profile).unwrap();
        let mut routes = AirTrafficControl::default();
        let a = routes.add(
            LinkMatch::Contains("example.com".into()),
            LinkDestination::Space(space),
        );
        let b = routes.add(
            LinkMatch::Domain("example.com".into()),
            LinkDestination::LittleWindow,
        );
        assert_eq!(
            routes.resolve(&browser, "https://example.com/a", None),
            Some(LinkDestination::Space(space))
        );
        routes.move_rule(b, 0);
        assert_eq!(
            routes.resolve(&browser, "https://docs.example.com/a", None),
            Some(LinkDestination::LittleWindow)
        );
        routes.rules[0].enabled = false;
        routes.remove(a);
        routes.add(
            LinkMatch::SourceApplication("com.example.mail".into()),
            LinkDestination::Profile(profile),
        );
        assert_eq!(
            routes.resolve(
                &browser,
                "https://outside.example",
                Some("com.example.mail")
            ),
            Some(LinkDestination::Profile(profile))
        );
        assert_eq!(
            routes.resolve(&browser, "https://outside.example", None),
            Some(LinkDestination::MostRecentSpace)
        );
        assert_eq!(routes.resolve(&browser, "file:///private/test", None), None);
        assert!(
            !LinkMatch::Domain("example.com".into())
                .matches(&url::Url::parse("https://badexample.com").unwrap(), None)
        );
        assert!(
            !LinkMatch::Contains("".into())
                .matches(&url::Url::parse("https://example.com").unwrap(), None)
        );
        browser.settings.air_traffic_control = Some(routes.clone());
        let loaded = crate::session::from_json(&crate::session::to_json(&browser)).unwrap();
        assert_eq!(loaded.settings.routing(), routes);
        let next = loaded.settings.routing().add(
            LinkMatch::IsEqualTo("example.com".into()),
            LinkDestination::MostRecentSpace,
        );
        assert!(next > b);
    }
}
