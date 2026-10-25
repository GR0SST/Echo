//! Native checked-state adapter for the shared Appearance commands.
use objc2::{msg_send, runtime::AnyObject};
use objc2_foundation::NSString;

pub(super) fn set_selected(selected: usize) {
    unsafe {
        let app: *mut AnyObject = msg_send![objc2::class!(NSApplication), sharedApplication];
        let root: *mut AnyObject = msg_send![app, mainMenu];
        if root.is_null() { return; }
        let view: *mut AnyObject = msg_send![root, itemWithTitle: &*NSString::from_str("View")];
        if view.is_null() { return; }
        let view: *mut AnyObject = msg_send![view, submenu];
        if view.is_null() { return; }
        let appearance: *mut AnyObject = msg_send![view, itemWithTitle: &*NSString::from_str("Appearance")];
        if appearance.is_null() { return; }
        let submenu: *mut AnyObject = msg_send![appearance, submenu];
        if submenu.is_null() { return; }
        for (index, title) in ["Automatic", "Light", "Dark"].into_iter().enumerate() {
            let item: *mut AnyObject = msg_send![submenu, itemWithTitle: &*NSString::from_str(title)];
            if !item.is_null() {
                let _: () = msg_send![item, setState: if selected == index { 1isize } else { 0isize }];
            }
        }
    }
}
