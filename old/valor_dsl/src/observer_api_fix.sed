# Remove the Entity parameter - access entity through trigger.entity_id() or similar
/^fn handle_click_events/,/)$/ {
    s/entity: Entity,//
}
# Need to access entity from the trigger - try trigger.entity_id()
s/\<entity\>/trigger.entity_id()/g
