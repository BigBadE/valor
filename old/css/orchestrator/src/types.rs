#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

impl Default for Stylesheet {
    #[inline]
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            origin: Origin::Author,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub origin: Origin,
    pub source_order: u32,
    pub prelude: String,
    pub declarations: Vec<Declaration>,
}
