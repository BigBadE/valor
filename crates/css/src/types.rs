//! Public types used by the css orchestrator.

#[derive(Clone, Copy, Debug)]
pub enum Origin {
    UserAgent,
    User,
    Author,
}

#[derive(Clone, Debug)]
pub struct Declaration {
    pub name: String,
    pub value: String,
    pub important: bool,
}

#[derive(Clone, Debug)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    pub origin: Origin,
}
impl Stylesheet {
    #[inline]
    pub const fn with_origin(origin: Origin) -> Self {
        Self {
            rules: Vec::new(),
            origin,
        }
    }
}
impl Default for Stylesheet {
    #[inline]
    fn default() -> Self {
        Self::with_origin(Origin::Author)
    }
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub origin: Origin,
    pub source_order: u32,
    pub prelude: String,
    pub declarations: Vec<Declaration>,
}
