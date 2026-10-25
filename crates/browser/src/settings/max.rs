//! Installed Arc Max layout with local, user-owned provider configuration.
//! Marketing artwork is original vector UI; no Arc logo/assets are bundled.
use super::*;
use echo_ai::{
    client::{Cancellation, Client, Model},
    credentials::{CredentialStore, NativeCredentialStore, Secret},
    oauth, Authentication, MaxFeature, Provider, ProviderConfig,
};
use std::sync::Arc;

pub(crate) trait Authorization: Send {
    fn url(&self) -> &str;
    fn finish(
        self: Box<Self>,
        store: &dyn CredentialStore,
        cancel: &Cancellation,
    ) -> Result<oauth::Account, echo_ai::client::AiError>;
}
impl Authorization for oauth::PendingSignIn {
    fn url(&self) -> &str {
        self.authorization_url()
    }
    fn finish(
        self: Box<Self>,
        store: &dyn CredentialStore,
        cancel: &Cancellation,
    ) -> Result<oauth::Account, echo_ai::client::AiError> {
        (*self).finish(store, cancel)
    }
}
pub(crate) trait AuthorizationProvider: Send + Sync {
    fn start(
        &self,
        store: &dyn CredentialStore,
        host: &str,
        prior: Option<&str>,
    ) -> Result<Box<dyn Authorization>, echo_ai::client::AiError>;
}
struct PublicOpenAi;
impl AuthorizationProvider for PublicOpenAi {
    fn start(
        &self,
        store: &dyn CredentialStore,
        host: &str,
        prior: Option<&str>,
    ) -> Result<Box<dyn Authorization>, echo_ai::client::AiError> {
        Ok(Box::new(oauth::PendingSignIn::start(store, host, prior)?))
    }
}
#[derive(Clone)]
pub(crate) struct Services {
    pub store: Arc<dyn CredentialStore>,
    pub auth: Arc<dyn AuthorizationProvider>,
}
impl Default for Services {
    fn default() -> Self {
        Self {
            store: Arc::new(NativeCredentialStore),
            auth: Arc::new(PublicOpenAi),
        }
    }
}
impl gpui::Global for Services {}
pub(crate) fn services(cx: &App) -> Services {
    cx.try_global::<Services>().cloned().unwrap_or_default()
}
pub(crate) fn store(cx: &App) -> Arc<dyn CredentialStore> {
    services(cx).store
}
pub(super) struct State {
    pub key: Entity<crate::text_input::TextInput>,
    pub base_url: Entity<crate::text_input::TextInput>,
    pub workspace: Entity<crate::text_input::TextInput>,
    pub model: Entity<crate::text_input::TextInput>,
    status: String,
    busy: Option<Cancellation>,
    models: Vec<Model>,
    _changed: Vec<Subscription>,
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl State {
    pub fn new(profile: &ProfileHandle, cx: &mut Context<SettingsWindow>) -> Self {
        let config = profile.read_settings(|b| b.settings.max.provider.clone());
        let key = cx.new(|cx| crate::text_input::TextInput::secret("API key", cx));
        let base_url = cx.new(|cx| {
            let mut t = crate::text_input::TextInput::new("https://your-provider.example/v1", cx);
            t.set_text(config.base_url, cx);
            t
        });
        let workspace = cx.new(|cx| {
            let mut t = crate::text_input::TextInput::new("Optional for workspace-scoped keys", cx);
            t.set_text(config.anthropic_workspace_id, cx);
            t
        });
        let workspace_changed = cx.subscribe(
            &workspace,
            |this, field, _: &crate::text_input::Changed, cx| {
                let workspace = field.read(cx).text().trim().to_string();
                this.profile
                    .edit_settings(|b| b.settings.max.provider.anthropic_workspace_id = workspace);
                cx.notify();
            },
        );
        let model = cx.new(|cx| {
            let mut t = crate::text_input::TextInput::new("Model ID", cx);
            t.set_text(config.model, cx);
            t
        });
        let changed = cx.subscribe(&model, |this, field, _: &crate::text_input::Changed, cx| {
            let model = field.read(cx).text().trim().to_string();
            this.profile
                .edit_settings(|b| b.settings.max.provider.model = model);
            cx.notify();
        });
        Self {
            key,
            base_url,
            workspace,
            model,
            status: String::new(),
            busy: None,
            models: Vec::new(),
            _changed: vec![changed, workspace_changed],
        }
    }
    fn cancel(&mut self) {
        if let Some(cancel) = self.busy.take() {
            cancel.cancel();
        }
    }
    fn begin(&mut self) -> Cancellation {
        self.cancel();
        let cancel = Cancellation::default();
        self.busy = Some(cancel.clone());
        self.status.clear();
        cancel
    }
}
const FEATURES: [(MaxFeature, &str, &str, &str); 6] = [
    (MaxFeature::TidyTabs, "Tidy Tabs", "Press the Tidy icon to let your Sidebar organize itself whenever you have more than six Today Tabs.", "brush-outline"),
    (MaxFeature::InstantLinks, "Instant Links", "Press Shift + Enter for any web search to instantly open the top result. You can also search multiple queries!", "flash-outline"),
    (MaxFeature::FiveSecondPreviews, "5-Second Previews", "Instant summaries appear when you hover over links on certain websites. Or hold Shift while hovering to preview any link.", "search-outline"),
    (MaxFeature::TidyTabTitles, "Tidy Tab Titles", "Have your tabs automatically renamed with tidier, shorter titles when you Pin them.", "text-outline"),
    (MaxFeature::TidyDownloads, "Tidy Downloads", "Keep your many files more organized with smartly renamed downloads — and make them a little easier to find later.", "download-outline"),
    (MaxFeature::ChatGptCommandBar, "ChatGPT in the Command Bar", "Add ChatGPT to your Search suggestions, or start typing “ChatGPT”, hit Tab, to ask any question instantly.", "chatbubble-outline"),
];
fn button(
    colors: appearance::Colors,
    cx: &mut Context<SettingsWindow>,
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let id = id.into();
    let action = id.clone();
    div()
        .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space")
                && !event.keystroke.modifiers.platform
            {
                this.max_activate(action.as_ref(), cx);
                cx.stop_propagation();
            }
        }))
        .id(id.clone())
        .debug_selector(move || id.to_string())
        .focusable()
        .tab_stop(true)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .min_h(px(24.))
        .px(px(12.))
        .rounded(px(5.))
        .bg(colors.rgb(0x393b3d))
        .hover(move |s| s.bg(colors.rgb(0x484a4a)))
        .child(label.into())
}
fn line(colors: appearance::Colors) -> gpui::Div {
    super::rule().bg(colors.rgb(0x3e3f41))
}
fn field(
    colors: appearance::Colors,
    entity: &Entity<crate::text_input::TextInput>,
    id: &'static str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .h(px(28.))
        .w(px(238.))
        .px(px(7.))
        .flex()
        .items_center()
        .rounded(px(5.))
        .border_1()
        .border_color(colors.rgb(0x555759))
        .bg(colors.rgb(0x242628))
        .child(entity.clone())
}
// Original Echo artwork: folded ribbons, rather than the reference's brand marks.
fn banner_art() -> impl IntoElement {
    gpui::canvas(|_, _, _| (), |bounds, _, window, _| {
        let origin = bounds.origin;
        let w = f32::from(bounds.size.width);
        let mut field = gpui::PathBuilder::fill();
        field.move_to(origin);
        field.line_to(origin + point(px(w), px(0.)));
        for x in (0..=684).rev() {
            let y = 122.5 + 2.5 * (x as f32 * std::f32::consts::TAU / 22.).sin();
            field.line_to(origin + point(px(x as f32 * w / 684.), px(y)));
        }
        field.close();
        if let Ok(path) = field.build() { window.paint_path(path, rgb(0x5148dd)); }
        for (x,y,size) in [(18.,18.,22.), (94.,70.,28.), (208.,24.,12.), (452.,20.,25.), (570.,73.,17.), (663.,91.,29.)] {
            let mut ribbon = gpui::PathBuilder::stroke(px(2.5));
            for (i,(dx,dy)) in [(0.,0.),(size,0.),(0.,size),(size,size)].into_iter().enumerate() {
                let p = origin + point(px(x+dx),px(y+dy));
                if i==0 { ribbon.move_to(p); } else { ribbon.line_to(p); }
            }
            if let Ok(path) = ribbon.build() { window.paint_path(path, rgb(0xffbaa8)); }
        }
    }).absolute().inset_0()
}
fn preview(index: usize, _icon: &'static str) -> gpui::Div {
    let colors = [0xd8d1ef, 0x72bed4, 0xb9c9ef, 0xd2c5ef, 0x9cd3cf, 0x71a0d5];
    div()
        .w(px(94.))
        .h(px(62.))
        .flex_none()
        .relative()
        .rounded(px(5.))
        .border(px(3.))
        .border_color(rgb(0xffffff))
        .bg(rgb(colors[index]))
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(0x696488))
        .child(super::max_art::feature(index))
}
impl SettingsWindow {
    fn max_activate(&mut self, action: &str, cx: &mut Context<Self>) {
        match action {
            "max-save-key" => self.max_save_key(cx),
            "max-load-models" => self.max_models(cx),
            "max-chatgpt-sign-in" => self.max_sign_in(None, cx),
            "max-chatgpt-sign-out" => self.max_sign_out(cx),
            "max-cancel" => {
                self.max.cancel();
                self.max.status = "Cancelled".into();
                cx.notify();
            }
            "max-enable-all" => {
                self.profile.edit_settings(|b| {
                    for (feature, _, _, _) in FEATURES {
                        b.settings.max.set_enabled(feature, true);
                    }
                });
                cx.notify();
            }
            "max-provider-OpenAi" => self.max_select_provider(Provider::OpenAi, cx),
            "max-provider-Anthropic" => self.max_select_provider(Provider::Anthropic, cx),
            "max-provider-OpenAiCompatible" => {
                self.max_select_provider(Provider::OpenAiCompatible, cx)
            }
            id if id.starts_with("max-account-") => {
                self.max_sign_in(Some(id.trim_start_matches("max-account-").into()), cx)
            }
            id if id.starts_with("max-model-") => {
                if let Some(model) = id
                    .trim_start_matches("max-model-")
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| self.max.models.get(n))
                    .cloned()
                {
                    self.max
                        .model
                        .update(cx, |field, cx| field.set_text(model.id, cx));
                    self.max.models.clear();
                    cx.notify();
                }
            }
            _ => {}
        }
    }
    fn max_toggle(&mut self, feature: MaxFeature, cx: &mut Context<Self>) {
        self.profile.edit_settings(|b| {
            let enabled = b.settings.max.enabled(feature);
            b.settings.max.set_enabled(feature, !enabled);
        });
        cx.notify();
    }
    fn max_select_provider(&mut self, provider: Provider, cx: &mut Context<Self>) {
        self.max.cancel();
        self.max.models.clear();
        self.max.status.clear();
        let config = self.profile.edit_settings(|b| {
            let settings = &mut b.settings.max;
            settings
                .saved_providers
                .insert(settings.provider.provider, settings.provider.clone());
            settings.provider =
                settings
                    .saved_providers
                    .get(&provider)
                    .cloned()
                    .unwrap_or(ProviderConfig {
                        provider,
                        ..Default::default()
                    });
            settings.provider.clone()
        });
        self.max.key.update(cx, |field, cx| field.set_text("", cx));
        self.max
            .base_url
            .update(cx, |field, cx| field.set_text(config.base_url, cx));
        self.max.workspace.update(cx, |field, cx| {
            field.set_text(config.anthropic_workspace_id, cx)
        });
        self.max
            .model
            .update(cx, |field, cx| field.set_text(config.model, cx));
        cx.notify();
    }
    pub(super) fn max_save_key(&mut self, cx: &mut Context<Self>) {
        let mut config = self
            .profile
            .read_settings(|b| b.settings.max.provider.clone());
        config.base_url = self.max.base_url.read(cx).text().trim().to_string();
        config.model = self.max.model.read(cx).text().trim().to_string();
        config.anthropic_workspace_id = self.max.workspace.read(cx).text().trim().to_string();
        config.authentication = Authentication::ApiKey;
        if let Err(error) = config.endpoint("models") {
            self.max.status = error.into();
            cx.notify();
            return;
        }
        let key = self.max.key.read(cx).text().trim().to_string();
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_graphic()) {
            self.max.status = "Enter an API key".into();
            cx.notify();
            return;
        }
        let Ok(id) = oauth::new_credential_id() else {
            self.max.status = "Could not prepare credential storage".into();
            cx.notify();
            return;
        };
        config.api_key_id = id.clone();
        let secret = Secret::new(key);
        self.max.key.update(cx, |field, cx| field.set_text("", cx));
        let cancel = self.max.begin();
        let store = store(cx);
        let worker_store = store.clone();
        let worker_cancel = cancel.clone();
        self.max.status = "Saving to the system keychain…".into();
        cx.notify();
        let worker = cx.background_executor().spawn(async move {
            worker_cancel.check().map_err(|_| "Cancelled")?;
            worker_store
                .set(&id, &secret)
                .map_err(|_| "The system credential store is unavailable")?;
            if worker_cancel.check().is_err() {
                let _ = worker_store.delete(&id);
                return Err("Cancelled");
            }
            Ok(config)
        });
        cx.spawn(async move |this, cx| {
            let result = worker.await;
            let saved_id = result.as_ref().ok().map(|c| c.api_key_id.clone());
            let updated = this.update(cx, |this, cx| {
                if cancel.check().is_err() {
                    return;
                }
                this.max.busy = None;
                match result {
                    Ok(config) => {
                        this.profile.edit_settings(|b| {
                            b.settings
                                .max
                                .saved_providers
                                .insert(config.provider, config.clone());
                            b.settings.max.provider = config;
                        });
                        this.max.status = "API key saved in the system keychain".into();
                    }
                    Err(error) => this.max.status = error.into(),
                }
                cx.notify();
            });
            if updated.is_err() || cancel.check().is_err() {
                if let Some(id) = saved_id {
                    let _ = store.delete(&id);
                }
            }
        })
        .detach();
    }
    fn max_models(&mut self, cx: &mut Context<Self>) {
        let config = self
            .profile
            .read_settings(|b| b.settings.max.provider.clone());
        let cancel = self.max.begin();
        let store = store(cx);
        let worker_cancel = cancel.clone();
        self.max.status = "Loading your provider’s models…".into();
        cx.notify();
        let task = cx
            .background_executor()
            .spawn(async move { Client { store }.models(&config, &worker_cancel) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if cancel.check().is_err() {
                    return;
                }
                this.max.busy = None;
                match result {
                    Ok(models) => {
                        this.max.models = models;
                        this.max.status = "Choose a model below, or enter its ID".into();
                    }
                    Err(error) => this.max.status = error.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn max_sign_in(&mut self, prior: Option<String>, cx: &mut Context<Self>) {
        let host = self.profile.edit_settings(|b| {
            if b.settings.max.host_id.is_empty() {
                b.settings.max.host_id = oauth::new_host_id().unwrap_or_default();
            }
            b.settings.max.host_id.clone()
        });
        let cancel = self.max.begin();
        let services = services(cx);
        let store = services.store;
        let auth = services.auth;
        let starting_store = store.clone();
        let prepare = cx
            .background_executor()
            .spawn(async move { auth.start(starting_store.as_ref(), &host, prior.as_deref()) });
        self.max.status = "Preparing ChatGPT sign-in…".into();
        cx.notify();
        cx.spawn(async move |this,cx| {
            let pending=match prepare.await {
                Ok(pending)=>pending,
                Err(error)=>{let _=this.update(cx,|this,cx|{if cancel.check().is_ok(){this.max.busy=None;this.max.status=error.to_string();cx.notify();}});return;}
            };
            if cancel.check().is_err(){return;}
            if this.update(cx,|this,cx|{this.max.status="Complete sign-in in your browser".into();cx.open_url(pending.url());cx.notify();}).is_err(){return;}
            let worker_cancel=cancel.clone();let worker_store=store.clone();
            let finish=cx.background_executor().spawn(async move{pending.finish(worker_store.as_ref(),&worker_cancel)});
            let result=finish.await;
            let _=this.update(cx,|this,cx|{
                if cancel.check().is_err(){return;}this.max.busy=None;
                match result {
                    Ok(account)=>{
                        let label=format!("{} · {}",if account.email.is_empty(){"ChatGPT"}else{&account.email},account.client_id.chars().rev().take(6).collect::<String>().chars().rev().collect::<String>());
                        this.profile.edit_settings(|b| {
                            let settings=&mut b.settings.max;
                            if let Some(saved)=settings.accounts.iter_mut().find(|a|a.credential_id==account.credential_id){saved.label=label.clone();}
                            else{settings.accounts.push(echo_ai::AccountReference{credential_id:account.credential_id.clone(),label});}
                            settings.saved_providers.insert(settings.provider.provider,settings.provider.clone());
                            settings.provider=settings.saved_providers.get(&Provider::OpenAi).cloned().unwrap_or_default();
                            settings.provider.provider=Provider::OpenAi;settings.provider.authentication=Authentication::ChatGpt;settings.provider.chatgpt_account_id=account.credential_id;
                        });
                        this.max.status=if account.plan_enabled{"Connected to ChatGPT. Choose a model to continue."}else{"Signed in. ChatGPT plan permission was not granted; reconnect or use an API key."}.into();
                        let config=this.profile.read_settings(|b|b.settings.max.provider.clone());
                        this.max.model.update(cx,|field,cx|field.set_text(config.model,cx));
                    }
                    Err(error)=>this.max.status=error.to_string(),
                }cx.notify();
            });
        }).detach();
    }
    fn max_sign_out(&mut self, cx: &mut Context<Self>) {
        let id = self
            .profile
            .read_settings(|b| b.settings.max.provider.chatgpt_account_id.clone());
        // Disconnect immediately; signing out must never silently activate a
        // previously saved API key or start another billable request.
        self.profile.edit_settings(|b| {
            b.settings.max.provider.authentication = Authentication::Disconnected
        });
        let store = store(cx);
        let cancel = self.max.begin();
        self.max.status = "Signing out…".into();
        cx.notify();
        let task = cx
            .background_executor()
            .spawn(async move { oauth::sign_out(store.as_ref(), &id) });
        cx.spawn(async move|this,cx|{let result=task.await;let _=this.update(cx,|this,cx|{if cancel.check().is_err(){return;}this.max.busy=None;
            this.profile.edit_settings(|b|b.settings.max.provider.authentication=Authentication::Disconnected);
            this.max.status=match result{Ok(true)=>"Signed out".into(),Ok(false)=>"Signed out locally. Remote revocation was not confirmed; you can disconnect Echo in ChatGPT Settings.".into(),Err(e)=>e.to_string()};cx.notify();});}).detach();
    }
    pub(super) fn max(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let settings = self.profile.read_settings(|b| b.settings.max.clone());
        let all = FEATURES.iter().all(|(f, _, _, _)| settings.enabled(*f));
        let provider = settings.provider.provider;
        let banner = div()
            .id("max-banner")
            .debug_selector(|| "max-banner".into())
            .w_full()
            .h(px(126.))
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(44.))
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(colors.rgb(0xffffff))
            .child(banner_art())
            .child("Max");
        let enable = self.settings_group()
            .px(px(9.))
            .child(
                row()
                    .h(px(44.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().size(px(24.)).flex_none().rounded(px(4.))
                                .bg(rgb(0x5148dd)).text_color(rgb(0xffbaa8))
                                .flex().items_center().justify_center()
                                .child(icons::ionicon("color-wand-outline", 18., 1.)))
                            .child("Enable Max"),
                    )
                    .child(
                        button(colors, cx, "max-enable-all", "Turn On All")
                            .bg(colors.rgb(0x3532ff))
                            .text_color(colors.rgb(0xffffff))
                            .w(px(117.))
                            .h(px(28.))
                            .when(all, |s| s.opacity(0.5))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.profile.edit_settings(|b| {
                                    for (feature, _, _, _) in FEATURES {
                                        b.settings.max.set_enabled(feature, true);
                                    }
                                });
                                cx.notify();
                            })),
                    ),
            )
            .child(line(colors))
            .child(
                row()
                    .h(px(37.))
                    .pl(px(3.))
                    .id("max-future")
                    .debug_selector(|| "max-future".into())
                    .focusable()
                    .tab_stop(true)
                    .child("Automatically enable future Max features")
                    .child(self.settings_switch(settings.automatically_enable_future))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.profile.edit_settings(|b| {
                            b.settings.max.automatically_enable_future =
                                !b.settings.max.automatically_enable_future
                        });
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, _, cx| {
                        if matches!(e.keystroke.key.as_str(), "space" | "enter") {
                            this.profile.edit_settings(|b| {
                                b.settings.max.automatically_enable_future =
                                    !b.settings.max.automatically_enable_future
                            });
                            cx.notify();
                            cx.stop_propagation();
                        }
                    })),
            );
        let mut features = self.settings_group().px(px(9.));
        for (index, (feature, title, description, icon)) in FEATURES.into_iter().enumerate() {
            if index > 0 {
                features = features.child(line(colors));
            }
            let enabled = settings.enabled(feature);
            features = features.child(
                div()
                    .id(("max-feature", index))
                    .debug_selector(move || format!("max-feature-{feature:?}"))
                    .h(px(80.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .focusable()
                    .tab_stop(true)
                    .cursor_pointer()
                    .child(preview(index, icon))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(title)
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .line_height(px(16.))
                                    .text_color(colors.rgb(0x8d9091))
                                    .child(description),
                            ),
                    )
                    .child(self.settings_switch(enabled))
                    .on_click(cx.listener(move |this, _, _, cx| this.max_toggle(feature, cx)))
                    .on_key_down(cx.listener(move |this, e: &gpui::KeyDownEvent, _, cx| {
                        if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                            this.max_toggle(feature, cx);
                            cx.stop_propagation();
                        }
                    })),
            );
        }
        let mut providers = self.settings_group().child(
            row().child("AI provider").child(
                div().flex().gap(px(4.)).children(
                    [
                        (Provider::OpenAi, "OpenAI"),
                        (Provider::Anthropic, "Anthropic"),
                        (Provider::OpenAiCompatible, "Compatible"),
                    ]
                    .into_iter()
                    .map(|(p, label)| {
                        button(colors, cx, SharedString::from(format!("max-provider-{p:?}")), label)
                            .debug_selector(move || format!("max-provider-{p:?}"))
                            .when(provider == p, |s| s.bg(colors.rgb(0x45525e)))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.max_select_provider(p, cx)),
                            )
                    }),
                ),
            ),
        );
        if provider == Provider::OpenAiCompatible {
            providers = providers.child(line(colors)).child(
                row()
                    .child("Base URL")
                    .child(field(colors, &self.max.base_url, "max-base-url")),
            );
        }
        if provider == Provider::Anthropic {
            providers = providers.child(line(colors)).child(
                row()
                    .child("Workspace ID")
                    .child(field(colors, &self.max.workspace, "max-workspace-id")),
            );
        }
        providers = providers
            .child(line(colors))
            .child(
                row()
                    .child("API key")
                    .child(field(colors, &self.max.key, "max-api-key")),
            )
            .child(
                row().justify_end().gap(px(8.)).child(
                    button(colors, cx, "max-save-key", "Save API Key")
                        .on_click(cx.listener(|this, _, _, cx| this.max_save_key(cx))),
                ),
            );
        if provider == Provider::OpenAi {
            providers = providers.child(line(colors)).child(
                row().child("ChatGPT").child(
                    button(colors, cx, "max-chatgpt-sign-in", "Sign in with ChatGPT")
                        .on_click(cx.listener(|this, _, _, cx| this.max_sign_in(None, cx))),
                ),
            );
            for account in &settings.accounts {
                let id = account.credential_id.clone();
                let selected = settings.provider.authentication == Authentication::ChatGpt
                    && settings.provider.chatgpt_account_id == id;
                providers =
                    providers.child(
                        row().child(account.label.clone()).child(
                            button(colors,
                                cx,
                                SharedString::from(format!("max-account-{id}")),
                                if selected { "Reconnect" } else { "Use account" },
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.max_sign_in(Some(id.clone()), cx),
                            )),
                        ),
                    );
            }
            if settings.provider.authentication == Authentication::ChatGpt {
                providers = providers.child(
                    row().justify_end().child(
                        button(colors, cx, "max-chatgpt-sign-out", "Sign Out")
                            .on_click(cx.listener(|this, _, _, cx| this.max_sign_out(cx))),
                    ),
                );
            }
        }
        providers = providers
            .child(line(colors))
            .child(
                row()
                    .child("Model")
                    .child(field(colors, &self.max.model, "max-model")),
            )
            .child(
                row().justify_end().child(
                    button(colors, cx, "max-load-models", "Load Models")
                        .on_click(cx.listener(|this, _, _, cx| this.max_models(cx))),
                ),
            );
        for (index, model) in self.max.models.iter().take(24).enumerate() {
            let id = model.id.clone();
            providers = providers.child(
                row().child(model.name.clone()).child(
                    button(colors,
                        cx,
                        SharedString::from(format!("max-model-{index}")),
                        "Use Model",
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.max
                            .model
                            .update(cx, |field, cx| field.set_text(id.clone(), cx));
                        this.max.models.clear();
                        cx.notify();
                    })),
                ),
            );
        }
        if self.max.busy.is_some() {
            providers = providers.child(row().justify_end().child(
                button(colors, cx, "max-cancel", "Cancel").on_click(cx.listener(|this, _, _, cx| {
                    this.max.cancel();
                    this.max.status = "Cancelled".into();
                    cx.notify();
                })),
            ));
        }
        providers = providers.child(
            div()
                .py(px(8.))
                .text_size(px(11.))
                .text_color(colors.rgb(0xaaaeb0))
                .child(self.max.status.clone()),
        );
        div().w(px(684.)).flex_none().flex().flex_col().child(banner)
            .child(div().px(px(64.)).pt(px(24.)).pb(px(20.)).flex().flex_col().gap(px(24.))
                .child(div().h(px(34.)).px(px(5.)).child("Let Echo do more for you with Max, so you can do less. Power up your browsing with AI and spend time on the things that matter — on or off screen."))
                .child(enable).child(features)
                .child(div().text_size(px(11.)).text_color(colors.rgb(0x96999a)).child("Max features send data directly to your selected AI provider. Keys and ChatGPT credentials stay in your system keychain."))
                .child(providers)).into_any_element()
    }
}
