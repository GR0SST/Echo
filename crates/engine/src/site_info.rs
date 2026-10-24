//! Owned page settings; native CEF objects never cross the engine boundary.
use crate::{Engine, ViewId, ViewRole};
use cef::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionKind {
    Camera,
    Microphone,
    Location,
    Notifications,
    Popups,
    Clipboard,
    AutomaticDownloads,
    Sound,
}
impl PermissionKind {
    pub const ALL: [Self; 8] = [
        Self::Camera,
        Self::Microphone,
        Self::Location,
        Self::Notifications,
        Self::Popups,
        Self::Clipboard,
        Self::AutomaticDownloads,
        Self::Sound,
    ];
    pub(crate) fn cef_type(self) -> ContentSettingTypes {
        match self {
            Self::Camera => ContentSettingTypes::MEDIASTREAM_CAMERA,
            Self::Microphone => ContentSettingTypes::MEDIASTREAM_MIC,
            Self::Location => ContentSettingTypes::GEOLOCATION,
            Self::Notifications => ContentSettingTypes::NOTIFICATIONS,
            Self::Popups => ContentSettingTypes::POPUPS,
            Self::Clipboard => ContentSettingTypes::CLIPBOARD_READ_WRITE,
            Self::AutomaticDownloads => ContentSettingTypes::AUTOMATIC_DOWNLOADS,
            Self::Sound => ContentSettingTypes::SOUND,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionValue {
    Default,
    Ask,
    Allow,
    Block,
}
impl PermissionValue {
    fn cef_value(self) -> ContentSettingValues {
        match self {
            Self::Default => ContentSettingValues::DEFAULT,
            Self::Ask => ContentSettingValues::ASK,
            Self::Allow => ContentSettingValues::ALLOW,
            Self::Block => ContentSettingValues::BLOCK,
        }
    }
    fn from_cef(value: ContentSettingValues) -> Self {
        if value == ContentSettingValues::ALLOW {
            Self::Allow
        } else if value == ContentSettingValues::BLOCK {
            Self::Block
        } else if value == ContentSettingValues::ASK {
            Self::Ask
        } else {
            Self::Default
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SitePermission {
    pub kind: PermissionKind,
    pub value: PermissionValue,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteCertificate {
    pub subject: String,
    pub issuer: String,
    pub der_chain: Vec<Vec<u8>>,
    pub status: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteInfo {
    pub origin: String,
    pub host: String,
    pub secure: bool,
    pub certificate: Option<SiteCertificate>,
    pub permissions: Vec<SitePermission>,
    pub zoom_percent: u16,
}

pub(crate) fn web_origin(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    Some(url.origin().ascii_serialization())
}
fn binary_bytes(value: BinaryValue) -> Option<Vec<u8>> {
    let mut bytes = vec![0; value.size()];
    (value.data(Some(&mut bytes), 0) == bytes.len()).then_some(bytes)
}
fn connection_is_secure(connection: bool, status: u32, content: u32) -> bool {
    let informational = CertStatus::IS_EV.get_raw()
        | CertStatus::REV_CHECKING_ENABLED.get_raw()
        | CertStatus::SHA1_SIGNATURE_PRESENT.get_raw();
    connection && status & !informational == 0 && content == SslContentStatus::NORMAL_CONTENT.get_raw()
}
impl Engine {
    pub fn zoom_percent(&self, id: ViewId) -> u16 {
        self.views
            .get(&id)
            .filter(|view| view.role == ViewRole::Page)
            .and_then(|view| view.browser.host())
            .map(|host| {
                (1.2_f64.powf(host.zoom_level()) * 100.)
                    .round()
                    .clamp(25., 500.) as u16
            })
            .unwrap_or(100)
    }
    pub fn site_info(&self, id: ViewId) -> Option<SiteInfo> {
        let view = self
            .views
            .get(&id)
            .filter(|view| view.role == ViewRole::Page)?;
        let host = view.browser.host()?;
        let url = CefString::from(&view.browser.main_frame()?.url()).to_string();
        let origin = web_origin(&url)?;
        let host_name = url::Url::parse(&origin).ok()?.host_str()?.to_owned();
        let cef_origin = CefString::from(origin.as_str());
        let permissions = PermissionKind::ALL
            .into_iter()
            .map(|kind| SitePermission {
                kind,
                value: PermissionValue::from_cef(view.context.content_setting(
                    Some(&cef_origin),
                    Some(&cef_origin),
                    kind.cef_type(),
                )),
            })
            .collect();
        let ssl = host
            .visible_navigation_entry()
            .and_then(|entry| entry.sslstatus());
        let secure = ssl.as_ref().is_some_and(|ssl| {
            connection_is_secure(
                ssl.is_secure_connection() != 0,
                ssl.cert_status().get_raw(),
                ssl.content_status().get_raw(),
            )
        });
        let certificate = ssl.as_ref().and_then(|ssl| {
            let cert = ssl.x509_certificate()?;
            let mut der_chain = Vec::new();
            der_chain.push(binary_bytes(cert.derencoded()?)?);
            let mut issuers = vec![None; cert.issuer_chain_size()];
            cert.derencoded_issuer_chain(Some(&mut issuers));
            der_chain.extend(issuers.into_iter().flatten().filter_map(binary_bytes));
            Some(SiteCertificate {
                subject: cert
                    .subject()
                    .map(|p| CefString::from(&p.display_name()).to_string())
                    .unwrap_or_default(),
                issuer: cert
                    .issuer()
                    .map(|p| CefString::from(&p.display_name()).to_string())
                    .unwrap_or_default(),
                der_chain,
                status: ssl.cert_status().get_raw(),
            })
        });
        Some(SiteInfo {
            origin,
            host: host_name,
            secure,
            certificate,
            permissions,
            zoom_percent: self.zoom_percent(id),
        })
    }
    pub fn set_site_permission(
        &mut self,
        id: ViewId,
        expected_origin: &str,
        kind: PermissionKind,
        value: PermissionValue,
    ) -> bool {
        let Some(view) = self
            .views
            .get(&id)
            .filter(|view| view.role == ViewRole::Page)
        else {
            return false;
        };
        let Some(frame) = view.browser.main_frame() else {
            return false;
        };
        let current = CefString::from(&frame.url()).to_string();
        if web_origin(&current).as_deref() != Some(expected_origin) {
            return false;
        }
        let origin = CefString::from(expected_origin);
        view.context.set_content_setting(
            Some(&origin),
            Some(&origin),
            kind.cef_type(),
            value.cef_value(),
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn origin_controls_do_not_cross_scheme_port_or_opaque_boundaries() {
        assert_eq!(
            web_origin("https://user:password@bücher.example:443/path").as_deref(),
            Some("https://xn--bcher-kva.example")
        );
        assert_ne!(
            web_origin("http://site.example"),
            web_origin("https://site.example")
        );
        assert_ne!(
            web_origin("https://site.example:444"),
            web_origin("https://site.example")
        );
        for url in [
            "file:///tmp/test",
            "data:text/html,test",
            "about:blank",
            "echo://settings",
        ] {
            assert!(web_origin(url).is_none());
        }
    }
    #[test]
    fn https_alone_and_certificate_or_mixed_content_errors_are_not_secure() {
        assert!(!connection_is_secure(false, 0, 0));
        assert!(connection_is_secure(true, CertStatus::IS_EV.get_raw(), 0));
        for status in [
            CertStatus::DATE_INVALID.get_raw(),
            CertStatus::AUTHORITY_INVALID.get_raw(),
            1 << 31,
        ] {
            assert!(!connection_is_secure(true, status, 0));
        }
        assert!(!connection_is_secure(
            true,
            0,
            SslContentStatus::RAN_INSECURE_CONTENT.get_raw()
        ));
    }
}
