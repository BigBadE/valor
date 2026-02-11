# Add event_target_mut to EntityEvent implementation
/impl bevy::ecs::event::EntityEvent for OnClick {/,/^}/ {
    /fn target/a\    fn event_target_mut(&mut self) -> &mut bevy::prelude::Entity {\n        &mut self.entity\n    }
}
