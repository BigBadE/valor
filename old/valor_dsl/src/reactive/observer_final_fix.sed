# In Bevy 0.17 observers, use Trigger (not On) and it has a .target() method
s/trigger: bevy::ecs::observer::On<OnClick>/trigger: bevy::ecs::observer::Trigger<OnClick>/g
s/trigger\.entity/trigger.target()/g
