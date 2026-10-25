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
    MotionSensors,
    Javascript,
    Images,
    IntrusiveAds,
    BackgroundSync,
    MidiDevices,
    UsbDevices,
    SerialPorts,
    FileEditing,
    HidDevices,
    PaymentHandlers,
    InsecureContent,
    JavascriptOptimization,
    AugmentedReality,
    VirtualReality,
    DeviceUse,
    WindowManagement,
    Fonts,
    AutomaticPictureInPicture,
    CapturedSurfaceControl,
    WebAppInstallation,
    LocalNetwork,
    LoopbackNetwork,
}
impl PermissionKind {
    pub const ALL: [Self; 31] = [
        Self::Camera,
        Self::Microphone,
        Self::Location,
        Self::Notifications,
        Self::Popups,
        Self::Clipboard,
        Self::AutomaticDownloads,
        Self::Sound,
        Self::MotionSensors,
        Self::Javascript,
        Self::Images,
        Self::IntrusiveAds,
        Self::BackgroundSync,
        Self::MidiDevices,
        Self::UsbDevices,
        Self::SerialPorts,
        Self::FileEditing,
        Self::HidDevices,
        Self::PaymentHandlers,
        Self::InsecureContent,
        Self::JavascriptOptimization,
        Self::AugmentedReality,
        Self::VirtualReality,
        Self::DeviceUse,
        Self::WindowManagement,
        Self::Fonts,
        Self::AutomaticPictureInPicture,
        Self::CapturedSurfaceControl,
        Self::WebAppInstallation,
        Self::LocalNetwork,
        Self::LoopbackNetwork,
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
            Self::MotionSensors => ContentSettingTypes::SENSORS,
            Self::Javascript => ContentSettingTypes::JAVASCRIPT,
            Self::Images => ContentSettingTypes::IMAGES,
            Self::IntrusiveAds => ContentSettingTypes::ADS,
            Self::BackgroundSync => ContentSettingTypes::BACKGROUND_SYNC,
            Self::MidiDevices => ContentSettingTypes::MIDI_SYSEX,
            Self::UsbDevices => ContentSettingTypes::USB_GUARD,
            Self::SerialPorts => ContentSettingTypes::SERIAL_GUARD,
            Self::FileEditing => ContentSettingTypes::FILE_SYSTEM_WRITE_GUARD,
            Self::HidDevices => ContentSettingTypes::HID_GUARD,
            Self::PaymentHandlers => ContentSettingTypes::PAYMENT_HANDLER,
            Self::InsecureContent => ContentSettingTypes::MIXEDSCRIPT,
            Self::JavascriptOptimization => ContentSettingTypes::JAVASCRIPT_OPTIMIZER,
            Self::AugmentedReality => ContentSettingTypes::AR,
            Self::VirtualReality => ContentSettingTypes::VR,
            Self::DeviceUse => ContentSettingTypes::IDLE_DETECTION,
            Self::WindowManagement => ContentSettingTypes::WINDOW_MANAGEMENT,
            Self::Fonts => ContentSettingTypes::LOCAL_FONTS,
            Self::AutomaticPictureInPicture => ContentSettingTypes::AUTO_PICTURE_IN_PICTURE,
            Self::CapturedSurfaceControl => ContentSettingTypes::CAPTURED_SURFACE_CONTROL,
            Self::WebAppInstallation => ContentSettingTypes::WEB_APP_INSTALLATION,
            Self::LocalNetwork => ContentSettingTypes::LOCAL_NETWORK,
            Self::LoopbackNetwork => ContentSettingTypes::LOOPBACK_NETWORK,
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
    let informational = CertStatus::IS_EV.get_raw() as u32
        | CertStatus::REV_CHECKING_ENABLED.get_raw() as u32
        | CertStatus::SHA1_SIGNATURE_PRESENT.get_raw() as u32;
    connection
        && status & !informational == 0
        && content == SslContentStatus::NORMAL_CONTENT.get_raw() as u32
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
                ssl.cert_status().get_raw() as u32,
                ssl.content_status().get_raw() as u32,
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
                status: ssl.cert_status().get_raw() as u32,
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
        if !kind.accepts(value) {
            return false;
        }
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
        assert!(connection_is_secure(true, CertStatus::IS_EV.get_raw() as u32, 0));
        for status in [
            CertStatus::DATE_INVALID.get_raw() as u32,
            CertStatus::AUTHORITY_INVALID.get_raw() as u32,
            1 << 31,
        ] {
            assert!(!connection_is_secure(true, status, 0));
        }
        assert!(!connection_is_secure(
            true,
            0,
            SslContentStatus::RAN_INSECURE_CONTENT.get_raw() as u32
        ));
    }
}

/// Profile-context controls for browser-owned settings pages. They deliberately
/// do not depend on a currently loaded document (unlike page popover toggles).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteSetting {
    pub kind: PermissionKind,
    pub value: PermissionValue,
    pub default_value: PermissionValue,
    /// None means the public API cannot qualify the setting's source.
    pub uses_default: Option<bool>,
}
impl PermissionKind {
    pub fn accepts(self, value: PermissionValue) -> bool {
        match value {
            PermissionValue::Default | PermissionValue::Block => true,
            PermissionValue::Allow => !matches!(
                self,
                Self::UsbDevices | Self::SerialPorts | Self::FileEditing | Self::HidDevices
            ),
            PermissionValue::Ask => !matches!(
                self,
                Self::Javascript
                    | Self::Images
                    | Self::IntrusiveAds
                    | Self::MotionSensors
                    | Self::Sound
                    | Self::Popups
                    | Self::BackgroundSync
                    | Self::PaymentHandlers
                    | Self::InsecureContent
                    | Self::JavascriptOptimization
            ),
        }
    }

    fn preference_name(self) -> &'static str {
        match self {
            Self::Camera => "media_stream_camera",
            Self::Microphone => "media_stream_mic",
            Self::Location => "geolocation",
            Self::Notifications => "notifications",
            Self::Popups => "popups",
            Self::Clipboard => "clipboard",
            Self::AutomaticDownloads => "automatic_downloads",
            Self::Sound => "sound",
            Self::MotionSensors => "sensors",
            Self::Javascript => "javascript",
            Self::Images => "images",
            Self::IntrusiveAds => "ads",
            Self::BackgroundSync => "background_sync",
            Self::MidiDevices => "midi_sysex",
            Self::UsbDevices => "usb_guard",
            Self::SerialPorts => "serial_guard",
            Self::FileEditing => "file_system_write_guard",
            Self::HidDevices => "hid_guard",
            Self::PaymentHandlers => "payment_handler",
            Self::InsecureContent => "mixed_script",
            Self::JavascriptOptimization => "javascript_optimizer",
            Self::AugmentedReality => "ar",
            Self::VirtualReality => "vr",
            Self::DeviceUse => "idle_detection",
            Self::WindowManagement => "window_placement",
            Self::Fonts => "local_fonts",
            Self::AutomaticPictureInPicture => "auto_picture_in_picture",
            Self::CapturedSurfaceControl => "captured_surface_control",
            Self::WebAppInstallation => "web_app_installation",
            Self::LocalNetwork => "local_network",
            Self::LoopbackNetwork => "loopback_network",
        }
    }
}
fn exact_default_source(exceptions: &serde_json::Value, origin: &str) -> Option<bool> {
    let entries = exceptions.as_object()?;
    let mut unknown = false;
    for key in entries.keys() {
        let (primary, secondary) = key.split_once(',')?;
        let primary_origin = web_origin(primary);
        let secondary_origin = web_origin(secondary);
        if primary_origin.as_deref() == Some(origin)
            && (secondary == "*" || secondary_origin.as_deref() == Some(origin))
        {
            return Some(false);
        }
        if primary_origin.is_none() || (secondary != "*" && secondary_origin.is_none()) {
            unknown = true;
        }
    }
    (!unknown).then_some(true)
}
impl crate::StorageContext {
    pub fn site_settings(&self, origin: &str) -> Option<Vec<SiteSetting>> {
        if cef::currently_on(cef::ThreadId::UI) == 0 || !self.is_ready() {
            return None;
        }
        let origin = web_origin(origin)?;
        let url = CefString::from(origin.as_str());
        Some(
            PermissionKind::ALL
                .into_iter()
                .map(|kind| {
                    let value = PermissionValue::from_cef(self.context.content_setting(
                        Some(&url),
                        Some(&url),
                        kind.cef_type(),
                    ));
                    let default_value = PermissionValue::from_cef(self.context.content_setting(
                        None,
                        None,
                        kind.cef_type(),
                    ));
                    let pref = CefString::from(
                        format!(
                            "profile.content_settings.exceptions.{}",
                            kind.preference_name()
                        )
                        .as_str(),
                    );
                    let uses_default = if value != default_value {
                        Some(false)
                    } else {
                        self.context.preference(Some(&pref)).and_then(|mut value| {
                            let json = CefString::from(&cef::write_json(
                                Some(&mut value),
                                cef::JsonWriterOptions::DEFAULT,
                            ))
                            .to_string();
                            exact_default_source(
                                &serde_json::from_str::<serde_json::Value>(&json).ok()?,
                                &origin,
                            )
                        })
                    };
                    SiteSetting {
                        kind,
                        value,
                        default_value,
                        uses_default,
                    }
                })
                .collect(),
        )
    }
    pub fn set_site_setting(
        &self,
        origin: &str,
        kind: PermissionKind,
        value: PermissionValue,
    ) -> bool {
        if cef::currently_on(cef::ThreadId::UI) == 0 || !self.is_ready() || !kind.accepts(value) {
            return false;
        }
        let Some(origin) = web_origin(origin) else {
            return false;
        };
        let url = CefString::from(origin.as_str());
        self.context.set_content_setting(
            Some(&url),
            Some(&url),
            kind.cef_type(),
            value.cef_value(),
        );
        true
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    #[test]
    fn permission_choices_and_default_labels_do_not_guess_from_equal_values() {
        assert!(!PermissionKind::Javascript.accepts(PermissionValue::Ask));
        assert!(PermissionKind::Camera.accepts(PermissionValue::Ask));
        assert!(!PermissionKind::UsbDevices.accepts(PermissionValue::Allow));
        assert!(PermissionKind::LocalNetwork.accepts(PermissionValue::Allow));
        for kind in PermissionKind::ALL {
            assert!(kind.accepts(PermissionValue::Default));
        }
        let origin = "https://example.com";
        assert_eq!(
            exact_default_source(
                &serde_json::json!({"https://example.com:443,*":{"setting":1}}),
                origin
            ),
            Some(false)
        );
        assert_eq!(
            exact_default_source(&serde_json::json!({}), origin),
            Some(true)
        );
        assert_eq!(
            exact_default_source(
                &serde_json::json!({"https://example.com,*":{"setting":1}}),
                origin
            ),
            Some(false)
        );
        assert_eq!(
            exact_default_source(
                &serde_json::json!({"https://other.example,*":{"setting":1}}),
                origin
            ),
            Some(true)
        );
        assert_eq!(
            exact_default_source(
                &serde_json::json!({"https://[*.]example.com,*":{"setting":1}}),
                origin
            ),
            None
        );
    }
}

/// cef-rs represents a default CefString as a null optional argument. CEF's
/// preference setter requires a non-null error output even on success. Borrow
/// an actual UTF-16 struct for the synchronous call and release CEF's allocation.
fn preference_call(call: impl FnOnce(&mut CefString) -> i32) -> Result<(), String> {
    struct Output(sys::cef_string_utf16_t);
    impl Drop for Output {
        fn drop(&mut self) {
            if !self.0.str_.is_null() {
                if let Some(dtor) = self.0.dtor {
                    unsafe { dtor(self.0.str_) };
                }
            }
        }
    }
    let mut output = Output(unsafe { std::mem::zeroed() });
    let success = {
        let mut error = CefString::from(&mut output.0 as *mut sys::cef_string_utf16_t);
        call(&mut error) != 0
    };
    if success {
        return Ok(());
    }
    let message = if output.0.str_.is_null() || output.0.length == 0 {
        String::new()
    } else {
        String::from_utf16_lossy(unsafe {
            std::slice::from_raw_parts(output.0.str_, output.0.length)
        })
    };
    Err(message)
}

impl crate::StorageContext {
    /// Reset automatic/never-translate decisions in this retained profile only.
    /// Null CEF values restore registered defaults, including blocked languages.
    pub fn reset_translation_prompts(&self) -> bool {
        if cef::currently_on(cef::ThreadId::UI) == 0 || !self.is_ready() {
            return false;
        }
        let required = [
            "translate_site_blocklist_with_time",
            "translate_allowlists",
            "translate_blocked_languages",
        ];
        if required
            .iter()
            .any(|name| self.context.has_preference(Some(&CefString::from(*name))) == 0)
        {
            return false;
        }
        let names = [
            "translate_site_blocklist",
            "translate_site_blocklist_with_time",
            "translate_allowlists",
            "translate_blocked_languages",
            "translate_denied_count_for_language",
            "translate_ignored_count_for_language",
            "translate_accepted_count",
            "translate_recent_target",
        ];
        let registered: Vec<_> = names
            .into_iter()
            .map(CefString::from)
            .filter(|name| self.context.has_preference(Some(name)) != 0)
            .collect();
        if registered
            .iter()
            .any(|name| self.context.can_set_preference(Some(name)) == 0)
        {
            return false;
        }
        for name in registered {
            if let Err(error) =
                preference_call(|error| self.context.set_preference(Some(&name), None, Some(error)))
            {
                eprintln!("[translation-reset] {name}: {error}");
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod preference_error_tests {
    use super::*;
    #[test]
    fn preference_calls_supply_a_real_output_buffer_and_preserve_utf16_errors() {
        let mut missing = CefString::default();
        let pointer: *mut sys::cef_string_utf16_t = (&mut missing).into();
        assert!(
            pointer.is_null(),
            "A default CefString is an absent API argument"
        );
        assert_eq!(
            preference_call(|error| {
                let pointer: *mut sys::cef_string_utf16_t = error.into();
                assert!(!pointer.is_null());
                1
            }),
            Ok(())
        );
        let mut message: Vec<u16> = "Managed preference 💜".encode_utf16().collect();
        assert_eq!(
            preference_call(|error| {
                let pointer: *mut sys::cef_string_utf16_t = error.into();
                unsafe {
                    (*pointer).str_ = message.as_mut_ptr();
                    (*pointer).length = message.len();
                }
                0
            }),
            Err("Managed preference 💜".into())
        );
    }
    #[test]
    fn preference_error_storage_is_released_after_the_call() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static DROPS: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "C" fn release(pointer: *mut u16) {
            drop(unsafe { Box::from_raw(pointer.cast::<[u16; 2]>()) });
            DROPS.fetch_add(1, Ordering::SeqCst);
        }
        let result = preference_call(|error| {
            let pointer: *mut sys::cef_string_utf16_t = error.into();
            unsafe {
                (*pointer).str_ = Box::into_raw(Box::new([b'x' as u16, b'y' as u16])).cast();
                (*pointer).length = 2;
                (*pointer).dtor = Some(release);
            }
            0
        });
        assert_eq!(result, Err("xy".into()));
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    }
}
