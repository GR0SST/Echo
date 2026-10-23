//! Connection/cookie-bound checked native requests. No global error handlers.

/// Production seam shared by XCB dispatch and server-free ordering tests. Only
/// this exact checked request may reject its returned resource/ownership value.
pub(crate) fn check_request<R>(
    connection: usize,
    sequence: u32,
    value: R,
    check: impl FnOnce(usize, u32) -> Result<(), String>,
) -> Result<R, String> {
    check(connection, sequence)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::{Cell, RefCell}, collections::HashMap, rc::Rc};
    struct Parent(Rc<Cell<usize>>);
    impl Drop for Parent { fn drop(&mut self) { self.0.set(self.0.get() + 1); } }

    #[test]
    fn interleaved_foreign_requests_and_handler_restores_cannot_reject_or_lose_parent() {
        let errors = RefCell::new(HashMap::from([
            ((22usize, 42u32), "foreign connection".to_owned()),
            ((11, 41), "earlier request".to_owned()),
            ((11, 43), "later request".to_owned()),
        ]));
        let global_handler = Cell::new("original");
        // Model the exact adversarial interleaving from R2. The production seam
        // receives no handler install/restore capability and cannot capture one.
        let foreign_saved = global_handler.replace("foreign");
        let drops = Rc::new(Cell::new(0));
        let owner = check_request(11, 42, Parent(drops.clone()), |connection, cookie| {
            assert_eq!(global_handler.get(), "foreign");
            assert_eq!((connection, cookie), (11, 42));
            match errors.borrow_mut().remove(&(connection, cookie)) {
                Some(error) => Err(error), None => Ok(()),
            }
        }).unwrap();
        global_handler.set(foreign_saved);
        assert_eq!(global_handler.get(), "original");
        // A second generation cannot save our handler as predecessor: there
        // never was an engine handler. It checks just its own XCB cookie again.
        check_request(11, 44, (), |connection, cookie| {
            assert_eq!(global_handler.get(), "original");
            assert_eq!((connection, cookie), (11, 44));
            Ok(())
        }).unwrap();
        assert_eq!(errors.borrow().len(), 3); // No foreign errors consumed.
        assert_eq!(drops.get(), 0);
        drop(owner);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn only_exact_checked_request_failure_rejects_and_releases_its_owned_value() {
        let drops = Rc::new(Cell::new(0));
        let result = check_request(11, u32::MAX, Parent(drops.clone()), |connection, cookie| {
            assert_eq!((connection, cookie), (11, u32::MAX));
            Err("own checked CreateWindow failed".into())
        });
        assert!(result.is_err());
        assert_eq!(drops.get(), 1);
    }
}
