//! Atomic creation from a window-local draft. Register with `pub mod space_creation;`.
use crate::{Browser, SpaceId, UserProfileId};

#[derive(Clone, Debug, PartialEq)]
pub struct SpaceCreation {
    pub origin: SpaceId,
    pub name: String,
    pub user_profile_id: UserProfileId,
    pub custom_icon: Option<String>,
    pub hue: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceCreationError {
    WindowLocalModel,
    UnknownOrigin,
    UnknownProfile,
    EmptyName,
    EmptyIcon,
    InvalidHue,
    IdentityExhausted,
}

impl Browser {
    /// Read-only validation on the authoritative workspace. Call BEFORE any
    /// wrapper that applies a window selection; even a failed edit_in_window
    /// closure otherwise changes the canonical serialized selection.
    pub fn validate_space_creation_request(
        &self,
        request: &SpaceCreation,
    ) -> Result<(), SpaceCreationError> {
        if self.is_blank() || self.is_private() {
            return Err(SpaceCreationError::WindowLocalModel);
        }
        self.spaces
            .iter()
            .find(|space| space.id == request.origin)
            .ok_or(SpaceCreationError::UnknownOrigin)?;
        if self.user_profile(request.user_profile_id).is_none() {
            return Err(SpaceCreationError::UnknownProfile);
        }
        if request.name.trim().is_empty() {
            return Err(SpaceCreationError::EmptyName);
        }
        if request
            .custom_icon
            .as_ref()
            .is_some_and(|icon| icon.trim().is_empty())
        {
            return Err(SpaceCreationError::EmptyIcon);
        }
        if !request.hue.is_finite() || !(0.0..=360.0).contains(&request.hue) {
            return Err(SpaceCreationError::InvalidHue);
        }
        if self.next_id == u64::MAX {
            return Err(SpaceCreationError::IdentityExhausted);
        }
        Ok(())
    }

    /// Production window seam: preflight before edit_in_window can mutate
    /// canonical selection. Keep this synchronous, with no callback/await/CEF
    /// pump between preflight and mutation. No whole-model rollback is needed.
    pub fn create_space_after_in_window(
        &mut self,
        selection: &mut crate::windows::WindowSelection,
        request: &SpaceCreation,
    ) -> Result<SpaceId, SpaceCreationError> {
        self.validate_space_creation_request(request)?;
        self.edit_in_window(selection, |browser| browser.create_space_after(request))
    }

    /// Validate everything before consuming an identity or changing selection.
    /// Insert immediately after the origin's *current* position, then select
    /// the empty Space. Profile data is referenced, never copied. This does
    /// not open a tab, modify browsing history, or request a Command Bar.
    /// Use create_space_after_in_window for a window-specific edit.
    pub fn create_space_after(
        &mut self,
        request: &SpaceCreation,
    ) -> Result<SpaceId, SpaceCreationError> {
        self.validate_space_creation_request(request)?;
        let origin = self
            .spaces
            .iter()
            .position(|space| space.id == request.origin)
            .expect("origin validated without intervening mutation");
        let id = self.add_space(request.name.trim(), request.hue);
        let mut space = self.spaces.pop().expect("add_space appends one Space");
        space.user_profile_id = request.user_profile_id;
        space.custom_icon = request.custom_icon.clone();
        self.spaces.insert(origin + 1, space);
        self.active_space = origin + 1;
        Ok(id)
    }
}

#[cfg(test)]
#[path = "space_creation_tests.rs"]
mod tests;
