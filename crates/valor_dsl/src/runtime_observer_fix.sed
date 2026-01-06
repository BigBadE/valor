# Fix handle_click_events signature to match Bevy 0.17 observer pattern
# In Bevy 0.17, observers receive (Trigger<Event>, ...) where Trigger contains entity info
/^fn handle_click_events<T: Component>(/,/^) {$/ {
    s/trigger: On<OnClick>/trigger: bevy::ecs::observer::Trigger<OnClick>/
}
# Replace trigger.target with trigger.entity()
s/trigger\.target/trigger.entity()/g
