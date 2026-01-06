# Move event creation inside the for loop
/let event = crate::bevy_events::OnClick {/,/};/ {
    # Delete the event creation block
    d
}
# Add event creation inside the loop, right after "for handler_entity"
/for handler_entity in handler_entities {/a\            let event = crate::bevy_events::OnClick {\n                node: js::NodeKey::ROOT,\n                position: (x, y),\n                button,\n                entity: handler_entity,\n            };
