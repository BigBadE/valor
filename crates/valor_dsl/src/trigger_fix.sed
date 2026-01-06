# Change Trigger to On (the new name in Bevy 0.17)
s/bevy::ecs::observer::Trigger/bevy::ecs::observer::On/g
# The entity is accessed via the first type parameter entity
s/trigger\.entity()/trigger.target/g
