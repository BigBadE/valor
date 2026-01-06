# In Bevy 0.17, observers receive (Trigger<Event>, Entity, ...) as parameters
# Change the signature to include entity parameter
/^fn handle_click_events/,/)$/ {
    s/trigger: bevy::ecs::observer::On<OnClick>,$/trigger: bevy::ecs::observer::On<OnClick>,\n    entity: Entity,/
}
# Replace trigger.target with entity
s/trigger\.target/entity/g
