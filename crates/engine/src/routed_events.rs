//! Callback queues can follow a live browser between independent engine pools.
//! The callback keeps its original router; only its destination buffer changes.
use std::{collections::HashMap, sync::{Arc, Mutex, Weak}};

type Buffer<T> = Mutex<Vec<(i32, T)>>;
pub(crate) struct RoutedEvents<T> {
    buffer: Arc<Buffer<T>>,
    routes: Arc<Mutex<HashMap<i32, Weak<Buffer<T>>>>>,
}
impl<T> Default for RoutedEvents<T> {
    fn default() -> Self { Self { buffer: Arc::default(), routes: Arc::default() } }
}
impl<T> Clone for RoutedEvents<T> {
    fn clone(&self) -> Self { Self { buffer: self.buffer.clone(), routes: self.routes.clone() } }
}
impl<T> RoutedEvents<T> {
    pub(crate) fn push(&self, event: (i32, T)) {
        // Hold the route while enqueuing. A handoff cannot leave a callback in
        // the old buffer after it has moved the pending metadata to its target.
        let routes = self.routes.lock().unwrap();
        let target = match routes.get(&event.0) {
            Some(target) => { let Some(target) = target.upgrade() else { return; }; target },
            None => self.buffer.clone(),
        };
        target.lock().unwrap().push(event);
    }
    pub(crate) fn take(&self) -> Vec<(i32, T)> {
        std::mem::take(&mut *self.buffer.lock().unwrap())
    }
    pub(crate) fn redirect(&self, browser: i32, current: &Self, target: &Self, mut metadata: impl FnMut(&T) -> bool) {
        let mut routes = self.routes.lock().unwrap();
        // Independent pools always have distinct buffers. The original router
        // may itself be the destination when a browser returns to its first pool.
        assert!(!Arc::ptr_eq(&current.buffer, &target.buffer));
        let mut current = current.buffer.lock().unwrap();
        let mut destination = target.buffer.lock().unwrap();
        let mut retained = Vec::with_capacity(current.len());
        for (id, event) in current.drain(..) {
            if id != browser { retained.push((id, event)); }
            else if metadata(&event) { destination.push((id, event)); }
            // Input queued for the old attachment must not activate the new one.
        }
        *current = retained;
        if Arc::ptr_eq(&self.buffer, &target.buffer) { routes.remove(&browser); }
        else { routes.insert(browser, Arc::downgrade(&target.buffer)); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, PartialEq)] enum Event { Metadata(u8), Input }
    #[test]
    fn a_live_browser_keeps_its_callback_router_across_repeated_handoffs() {
        let a = RoutedEvents::default(); let b = RoutedEvents::default(); let c = RoutedEvents::default();
        let metadata = |event: &Event| matches!(event, Event::Metadata(_));
        a.push((1, Event::Metadata(1))); a.push((1, Event::Input)); a.push((2, Event::Metadata(2)));
        a.redirect(1, &a, &b, metadata);
        assert_eq!(a.take(), vec![(2, Event::Metadata(2))]);
        assert_eq!(b.take(), vec![(1, Event::Metadata(1))]);
        a.push((1, Event::Metadata(3)));
        a.redirect(1, &b, &c, metadata);
        a.push((1, Event::Metadata(4)));
        assert!(b.take().is_empty());
        assert_eq!(c.take(), vec![(1, Event::Metadata(3)), (1, Event::Metadata(4))]);
        a.redirect(1, &c, &a, metadata);
        a.push((1, Event::Input));
        assert_eq!(a.take(), vec![(1, Event::Input)]);
    }
    #[test]
    fn callbacks_after_the_destination_is_dropped_are_discarded() {
        let original = RoutedEvents::default();
        {
            let target = RoutedEvents::default();
            original.redirect(1, &original, &target, |_: &Event| true);
        }
        original.push((1, Event::Metadata(1)));
        assert!(original.take().is_empty());
    }
}
