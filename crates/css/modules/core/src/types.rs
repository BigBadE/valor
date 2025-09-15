#[derive(Clone, Copy, Debug)]
pub enum Origin {
    UserAgent,
    User,
    Author,
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
}
