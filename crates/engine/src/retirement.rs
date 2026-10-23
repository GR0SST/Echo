//! Shared, CEF-free continuation decisions. Native ownership stays in Lifecycle.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadRetirementPreparation {
    /// Original GPUI parents are safe to destroy. Document quiescence may still
    /// be pending in independently owned native hosts; inspect status separately.
    Ready,
    Pending,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadRetirementStatus {
    pub native_browser: i32,
    pub provenance: crate::DownloadProvenance,
    pub parked: bool,
    pub unfinished: usize,
    pub waiting_for_association: bool,
    pub blank_requested: bool,
    pub blank_verified: bool,
    pub close_requested: bool,
    pub closed: bool,
    pub host_released: bool,
    pub failure: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Transfers {
    pub associated: bool,
    pub unfinished: usize,
    pub destinations_ready: bool,
}
impl Default for Transfers {
    fn default() -> Self {
        Self { associated: false, unfinished: 0, destinations_ready: true }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Retirement {
    pub parked: bool,
    pub blank_requested: bool,
    pub blank_verified: bool,
    pub close_requested: bool,
    pub failure: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action { Park, Wait, Blank, Close, Failed }

impl Retirement {
    /// Collection MUST precede this decision. A loading, unassociated initiator
    /// must survive; no timeout can prove that its future item will not arrive.
    pub fn next(&self, transfers: Transfers, loading: bool, blank: bool) -> Action {
        if self.failure.is_some() { return Action::Failed; }
        if self.close_requested { return Action::Wait; }
        if transfers.unfinished == 0 && (transfers.associated || !loading) {
            return Action::Close;
        }
        if !self.parked { return Action::Park; }
        if !transfers.associated || !transfers.destinations_ready { return Action::Wait; }
        if !self.blank_requested { return Action::Blank; }
        if !blank || loading { return Action::Wait; }
        Action::Wait
    }

    pub fn status(&self, native_browser: i32, provenance: crate::DownloadProvenance, transfers: Transfers) -> DownloadRetirementStatus {
        DownloadRetirementStatus {
            native_browser, provenance, parked: self.parked, unfinished: transfers.unfinished,
            waiting_for_association: !transfers.associated && !self.close_requested,
            blank_requested: self.blank_requested, blank_verified: self.blank_verified,
            close_requested: self.close_requested, closed: false, host_released: false,
            failure: self.failure.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_initiator_and_destination_never_trigger_navigation_or_close() {
        let mut retirement = Retirement::default();
        assert_eq!(retirement.next(Transfers::default(), true, false), Action::Park);
        retirement.parked = true;
        assert_eq!(retirement.next(Transfers::default(), true, false), Action::Wait);
        let mut transfers = Transfers { associated: true, unfinished: 2, destinations_ready: false };
        assert_eq!(retirement.next(transfers, false, false), Action::Wait);
        transfers.destinations_ready = true;
        assert_eq!(retirement.next(transfers, false, false), Action::Blank);
        retirement.blank_requested = true;
        assert_eq!(retirement.next(transfers, true, false), Action::Wait);
        transfers.unfinished = 1;
        assert_eq!(retirement.next(transfers, false, true), Action::Wait);
        transfers.unfinished = 0;
        assert_eq!(retirement.next(transfers, false, true), Action::Close);
        retirement.close_requested = true;
        assert_eq!(retirement.next(transfers, false, true), Action::Wait);
    }
    #[test]
    fn zero_download_close_is_prompt_and_host_failure_never_falls_back_to_cancel() {
        let mut retirement = Retirement::default();
        assert_eq!(retirement.next(Transfers::default(), false, false), Action::Close);
        retirement.failure = Some("host handoff failed".into());
        assert_eq!(retirement.next(Transfers::default(), false, false), Action::Failed);
        assert!(!retirement.close_requested);
    }
}
