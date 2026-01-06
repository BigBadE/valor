# Fix trigger_targets API (now takes only event, targets handled differently in 0.17)
s/world\.trigger_targets(\(.*\), handler_entity)/world.trigger(\1)/g
