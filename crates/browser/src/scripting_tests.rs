use super::*;
use browser_core::scripting::{code, Command, Object, Query, Record, Value};
fn dispatch(command: Command, cx: &mut VisualTestContext) -> Value {
    cx.cx
        .update(|cx| crate::scripting::dispatch(command, cx))
        .unwrap()
}
#[gpui::test]
fn scripting_creation_url_selection_and_closing_edit_the_canonical_profile(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let state = cx.cx.update(crate::scripting::snapshot);
    let window = state.windows[0].id.clone();
    let initial = view.read_with(cx, |v, _| v.browser.all_tabs().count());
    let made = dispatch(
        Command::Make {
            class: code(b"tAbB"),
            container: Object::Window(window.clone()).query(),
            properties: Record::from([(
                code(b"URL "),
                Value::Text("https://script.example/first?x=1#part".into()),
            )]),
        },
        cx,
    );
    let Value::Object(tab) = made else {
        panic!("make must return a stable tab reference")
    };
    assert_eq!(
        view.read_with(cx, |v, _| v
            .profile
            .handle
            .read_settings(|b| b.all_tabs().count())),
        initial + 1
    );
    let url = Query::Property {
        property: code(b"URL "),
        container: Box::new(tab.query()),
    };
    assert_eq!(
        dispatch(Command::Get(url.clone()), cx),
        Value::Text("https://script.example/first?x=1#part".into())
    );
    dispatch(
        Command::Set(
            url.clone(),
            Value::Text("https://script.example/second".into()),
        ),
        cx,
    );
    cx.run_until_parked();
    assert_eq!(
        dispatch(Command::Get(url), cx),
        Value::Text("https://script.example/second".into())
    );
    dispatch(Command::Select(tab.query()), cx);
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |v, _| v.browser.active_tab().unwrap().url.clone()),
        "https://script.example/second"
    );
    dispatch(Command::Close(tab.query()), cx);
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |v, _| v
            .profile
            .handle
            .read_settings(|b| b.all_tabs().count())),
        initial
    );
    assert!(cx
        .cx
        .update(|cx| crate::scripting::dispatch(Command::Get(tab.query()), cx))
        .is_err());
}
#[gpui::test]
fn scripting_focus_and_location_preserve_unrelated_spaces_and_reject_unknown_values(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let state = cx.cx.update(crate::scripting::snapshot);
    let w = &state.windows[0];
    let other = w.spaces[0].clone();
    dispatch(
        Command::Focus(
            Object::Space {
                window: w.id.clone(),
                id: other.id.clone(),
            }
            .query(),
        ),
        cx,
    );
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |v, _| v.browser.space().name.clone()),
        other.title
    );
    let made = dispatch(
        Command::Make {
            class: code(b"tAbB"),
            container: Object::Window(w.id.clone()).query(),
            properties: Record::new(),
        },
        cx,
    );
    let Value::Object(tab) = made else { panic!() };
    let location = Query::Property {
        property: code(b"loca"),
        container: Box::new(tab.query()),
    };
    dispatch(
        Command::Set(location.clone(), Value::Text("pinned".into())),
        cx,
    );
    assert_eq!(
        dispatch(Command::Get(location.clone()), cx),
        Value::Text("pinned".into())
    );
    assert!(cx
        .cx
        .update(|cx| crate::scripting::dispatch(
            Command::Set(location.clone(), Value::Text("invented".into())),
            cx
        ))
        .is_err());
    assert_eq!(
        dispatch(Command::Get(location), cx),
        Value::Text("pinned".into())
    );
}
#[gpui::test]
fn scripting_javascript_requires_the_explicit_menu_opt_in_and_targets_the_live_tab(
    cx: &mut TestAppContext,
) {
    let fake = FakeEngine::default();
    let (view, cx) = open_with(cx, Some(Box::new(fake.clone())));
    view.update(cx, |v, cx| v.retain_normal_source(cx));
    cx.run_until_parked();
    let state = cx.cx.update(crate::scripting::snapshot);
    let w = &state.windows[0];
    let q = Object::Tab {
        window: w.id.clone(),
        id: w.active_tab.clone().unwrap(),
    }
    .query();
    let result = cx
        .cx
        .update(|cx| crate::scripting::start(Command::Execute(q.clone(), "1+2".into()), cx));
    assert!(matches!(result,crate::scripting::Dispatch::Done(Err(e)) if e.number==-10000));
    assert!(fake.0.borrow().script_calls.is_empty());
    cx.cx
        .update(|cx| cx.dispatch_action(&crate::menus::AllowJavaScriptFromAppleEvents));
    cx.run_until_parked();
    assert!(cx.cx.update(|cx| crate::scripting::javascript_allowed(cx)));
    let result = cx
        .cx
        .update(|cx| crate::scripting::start(Command::Execute(q, "1+2".into()), cx));
    let crate::scripting::Dispatch::Script(request) = result else {
        panic!("expected live script request")
    };
    assert_eq!(
        cx.cx.update(|cx| request.poll(cx)),
        Some(Ok(Value::Text("3".into())))
    );
    assert_eq!(fake.0.borrow().script_calls.last().unwrap().1, "1+2");
}
#[gpui::test]
fn scripting_rejects_a_completed_reply_after_the_model_url_changes_before_render(
    cx: &mut TestAppContext,
) {
    let fake = FakeEngine::default();
    let (view, cx) = open_with(cx, Some(Box::new(fake)));
    view.update(cx, |v, cx| v.retain_normal_source(cx));
    cx.cx.update(crate::scripting::toggle_javascript);
    cx.run_until_parked();
    let state = cx.cx.update(crate::scripting::snapshot);
    let w = &state.windows[0];
    let q = Object::Tab {
        window: w.id.clone(),
        id: w.active_tab.clone().unwrap(),
    }
    .query();
    let result = cx
        .cx
        .update(|cx| crate::scripting::start(Command::Execute(q, "document.title".into()), cx));
    let crate::scripting::Dispatch::Script(request) = result else {
        panic!("expected script request")
    };
    view.update(cx, |v, _| {
        let id = v.browser.active_tab().unwrap().id;
        v.edit_browser(|b| b.navigate(id, "https://replacement.example/"));
    });
    assert!(cx.cx.update(|cx| request.poll(cx)).unwrap().is_err());
}
