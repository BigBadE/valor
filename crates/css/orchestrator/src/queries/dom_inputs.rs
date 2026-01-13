//! Input queries for DOM data.
//!
//! These queries represent data from outside the style system (the DOM tree).
//! When DOM data changes, these inputs are updated and dependent style queries
//! are automatically invalidated.

use js::NodeKey;
use std::collections::HashMap;
use valor_query::InputQuery;

/// Input: Element tag name.
pub struct DomTagInput;

impl InputQuery for DomTagInput {
    type Key = NodeKey;
    type Value = String;

    fn default_value() -> Self::Value {
        String::new()
    }

    fn name() -> &'static str {
        "DomTagInput"
    }
}

/// Input: Element ID attribute.
pub struct DomIdInput;

impl InputQuery for DomIdInput {
    type Key = NodeKey;
    type Value = Option<String>;

    fn default_value() -> Self::Value {
        None
    }

    fn name() -> &'static str {
        "DomIdInput"
    }
}

/// Input: Element classes.
pub struct DomClassesInput;

impl InputQuery for DomClassesInput {
    type Key = NodeKey;
    type Value = Vec<String>;

    fn default_value() -> Self::Value {
        Vec::new()
    }

    fn name() -> &'static str {
        "DomClassesInput"
    }
}

/// Input: Element attributes.
pub struct DomAttributesInput;

impl InputQuery for DomAttributesInput {
    type Key = NodeKey;
    type Value = HashMap<String, String>;

    fn default_value() -> Self::Value {
        HashMap::new()
    }

    fn name() -> &'static str {
        "DomAttributesInput"
    }
}

/// Input: Parent node.
pub struct DomParentInput;

impl InputQuery for DomParentInput {
    type Key = NodeKey;
    type Value = Option<NodeKey>;

    fn default_value() -> Self::Value {
        None
    }

    fn name() -> &'static str {
        "DomParentInput"
    }
}

/// Input: Children nodes.
pub struct DomChildrenInput;

impl InputQuery for DomChildrenInput {
    type Key = NodeKey;
    type Value = Vec<NodeKey>;

    fn default_value() -> Self::Value {
        Vec::new()
    }

    fn name() -> &'static str {
        "DomChildrenInput"
    }
}

/// Input: Text content for text nodes.
pub struct DomTextInput;

impl InputQuery for DomTextInput {
    type Key = NodeKey;
    type Value = Option<String>;

    fn default_value() -> Self::Value {
        None
    }

    fn name() -> &'static str {
        "DomTextInput"
    }
}
