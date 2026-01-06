# Add entity field to OnClick struct
/pub struct OnClick {/,/^}/ {
    /pub button: u8,/a\    /// The entity that was clicked.\n    pub entity: bevy::prelude::Entity,
}
# Fix EntityEvent implementation to use self.entity
s/bevy::prelude::Entity::PLACEHOLDER/self.entity/
