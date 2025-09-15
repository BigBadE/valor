use css::CSSMirror;
use js::{DOMSubscriber as _, DOMUpdate, NodeKey};

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Result, bail, ensure};

    /// Verifies that removing a <style> node retracts all its rules from the stylesheet.
    ///
    /// # Errors
    /// Returns an error if DOM mirroring operations fail or if the rules are not retracted.
    #[test]
    fn style_removal_retracts_rules() -> Result<()> {
        let mut mirror = CSSMirror::new();
        let root = NodeKey::ROOT;
        let style_node = NodeKey(100);

        // <style>div { color: red }</style>
        mirror.apply_update(DOMUpdate::InsertElement {
            parent: root,
            node: style_node,
            tag: "style".into(),
            pos: 0,
        })?;
        mirror.apply_update(DOMUpdate::InsertText {
            parent: style_node,
            node: NodeKey(101),
            text: "div { color: red }".into(),
            pos: 0,
        })?;
        mirror.apply_update(DOMUpdate::EndOfDocument)?;

        let before = mirror.styles().clone();
        ensure!(
            !before.rules.is_empty(),
            "expected at least one rule before removal"
        );

        // Remove the style node
        mirror.apply_update(DOMUpdate::RemoveNode { node: style_node })?;
        let after = mirror.styles().clone();
        ensure!(
            after.rules.is_empty(),
            "expected rules to be retracted after removing <style> node"
        );
        Ok(())
    }

    /// Verifies that CSS rule `source_order` values are strictly increasing across
    /// interleaved <style> blocks.
    ///
    /// # Errors
    /// Returns an error if DOM mirroring operations fail or if the source order is not increasing.
    #[test]
    fn source_order_monotonic_interleaved() -> Result<()> {
        let mut mirror = CSSMirror::new();
        let root = NodeKey::ROOT;
        let style_one = NodeKey(200);
        let style_two = NodeKey(201);

        // <style>div{color:red}</style>
        mirror.apply_update(DOMUpdate::InsertElement {
            parent: root,
            node: style_one,
            tag: "style".into(),
            pos: 0,
        })?;
        mirror.apply_update(DOMUpdate::InsertText {
            parent: style_one,
            node: NodeKey(202),
            text: "div { color: red }".into(),
            pos: 0,
        })?;

        // <style>p{color:blue}</style>
        mirror.apply_update(DOMUpdate::InsertElement {
            parent: root,
            node: style_two,
            tag: "style".into(),
            pos: 1,
        })?;
        mirror.apply_update(DOMUpdate::InsertText {
            parent: style_two,
            node: NodeKey(203),
            text: "p { color: blue }".into(),
            pos: 0,
        })?;

        // finalize
        mirror.apply_update(DOMUpdate::EndOfDocument)?;

        let sheet = mirror.styles().clone();
        ensure!(
            sheet.rules.len() == 2,
            "expected two rules from two style blocks"
        );
        let order0 = match sheet.rules.first() {
            Some(rule_item) => rule_item.source_order,
            None => bail!("missing first rule"),
        };
        let order1 = match sheet.rules.get(1) {
            Some(rule_item) => rule_item.source_order,
            None => bail!("missing second rule"),
        };
        ensure!(
            order0 < order1,
            "expected strictly increasing source_order, got {order0:?} then {order1:?}"
        );
        Ok(())
    }
}
