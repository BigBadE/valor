# Fix Trigger -> On
s/Trigger<OnClick>/On<OnClick>/g
s/use bevy::prelude::\*;/use bevy::prelude::*;\nuse bevy::ecs::observer::On;/g
