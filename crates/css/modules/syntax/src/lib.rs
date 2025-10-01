//! CSS Syntax Module Level 3 — Parsing and tokenization.
//! Spec: <https://www.w3.org/TR/css-syntax-3/>
use cssparser::AtRuleParser as CssAtRuleParser;
use cssparser::BasicParseErrorKind;
use cssparser::CowRcStr;
use cssparser::DeclarationParser as CssDeclarationParser;
use cssparser::ParseError;
use cssparser::Parser;
use cssparser::ParserInput;
use cssparser::ParserState;
use cssparser::QualifiedRuleParser as CssQualifiedRuleParser;
use cssparser::RuleBodyItemParser as CssRuleBodyItemParser;
use cssparser::RuleBodyParser as CssRuleBodyParser;
use cssparser::StyleSheetParser;

/// A single CSS declaration (property: value [!important]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    /// Lowercased property name.
    pub name: String,
    /// Raw value text (without trailing !important).
    pub value: String,
    /// Whether the declaration was marked as `!important`.
    pub important: bool,
}

/// A single style rule with a raw prelude and parsed declarations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleRule {
    /// Raw prelude text (typically the selector list).
    pub prelude: String,
    /// Declarations within the rule block.
    pub declarations: Vec<Declaration>,
}

/// A parsed stylesheet consisting of style rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stylesheet {
    /// Top-level style rules in source order.
    pub rules: Vec<StyleRule>,
}

/// Parse `!important` at the end of a value, returning (`value_without_important`, `important_flag`).
fn split_important_tail(value: &str) -> (String, bool) {
    let trimmed = value.trim();
    if let Some(pos) = trimmed.rfind("!important")
        && let Some(prefix) = trimmed.get(..pos)
    {
        let head = prefix.trim_end();
        return (head.to_owned(), true);
    }
    (trimmed.to_owned(), false)
}

/// A declaration parser that records property name and its raw value.
struct BodyDeclParser;

impl CssDeclarationParser<'_> for BodyDeclParser {
    type Declaration = Declaration;
    type Error = ();

    fn parse_value<'input>(
        &mut self,
        name: CowRcStr<'input>,
        input: &mut Parser<'input, '_>,
        _decl_start: &ParserState,
    ) -> Result<Self::Declaration, ParseError<'input, Self::Error>> {
        let start = input.position();
        // Consume until end of the declaration item.
        while input.next_including_whitespace_and_comments().is_ok() {}
        let raw = input.slice_from(start);
        let (value, important) = split_important_tail(raw);
        Ok(Declaration {
            name: name.to_ascii_lowercase(),
            value,
            important,
        })
    }
}

impl CssAtRuleParser<'_> for BodyDeclParser {
    type Prelude = ();
    type AtRule = Declaration; // Not produced
    type Error = ();

    #[inline]
    fn parse_prelude<'input>(
        &mut self,
        _name: CowRcStr<'input>,
        _input: &mut Parser<'input, '_>,
    ) -> Result<Self::Prelude, ParseError<'input, Self::Error>> {
        Ok(())
    }

    #[inline]
    fn parse_block<'input>(
        &mut self,
        _prelude: Self::Prelude,
        _state: &ParserState,
        input: &mut Parser<'input, '_>,
    ) -> Result<Self::AtRule, ParseError<'input, Self::Error>> {
        // Not produced by this parser
        Err(input.new_error(BasicParseErrorKind::AtRuleBodyInvalid))
    }

    #[inline]
    fn rule_without_block(
        &mut self,
        _prelude: Self::Prelude,
        _state: &ParserState,
    ) -> Result<Self::AtRule, Self::Error> {
        Err(())
    }
}

impl CssQualifiedRuleParser<'_> for BodyDeclParser {
    type Prelude = ();
    type QualifiedRule = Declaration; // Not produced
    type Error = ();

    #[inline]
    fn parse_prelude<'input>(
        &mut self,
        input: &mut Parser<'input, '_>,
    ) -> Result<Self::Prelude, ParseError<'input, Self::Error>> {
        Err(input.new_error(BasicParseErrorKind::QualifiedRuleInvalid))
    }

    #[inline]
    fn parse_block<'input>(
        &mut self,
        _prelude: Self::Prelude,
        _state: &ParserState,
        input: &mut Parser<'input, '_>,
    ) -> Result<Self::QualifiedRule, ParseError<'input, Self::Error>> {
        Err(input.new_error(BasicParseErrorKind::QualifiedRuleInvalid))
    }
}

impl CssRuleBodyItemParser<'_, Declaration, ()> for BodyDeclParser {
    fn parse_declarations(&self) -> bool {
        true
    }
    fn parse_qualified(&self) -> bool {
        false
    }
}

/// Top-level parser that builds `StyleRule` items for qualified rules.
struct TopLevelParser;

impl CssAtRuleParser<'_> for TopLevelParser {
    type Prelude = ();
    type AtRule = StyleRule;
    type Error = ();

    #[inline]
    fn parse_prelude<'input>(
        &mut self,
        _name: CowRcStr<'input>,
        _input: &mut Parser<'input, '_>,
    ) -> Result<Self::Prelude, ParseError<'input, Self::Error>> {
        Ok(())
    }

    #[inline]
    fn parse_block<'input>(
        &mut self,
        _prelude: Self::Prelude,
        _state: &ParserState,
        input: &mut Parser<'input, '_>,
    ) -> Result<Self::AtRule, ParseError<'input, Self::Error>> {
        // For now we skip at-rules entirely by returning an error.
        Err(input.new_error(BasicParseErrorKind::AtRuleBodyInvalid))
    }

    #[inline]
    fn rule_without_block(
        &mut self,
        _prelude: Self::Prelude,
        _state: &ParserState,
    ) -> Result<Self::AtRule, Self::Error> {
        // Reject at-rules without blocks for MVP.
        Err(())
    }
}

impl CssQualifiedRuleParser<'_> for TopLevelParser {
    type Prelude = String; // raw selector/prelude
    type QualifiedRule = StyleRule;
    type Error = ();

    #[inline]
    fn parse_prelude<'input>(
        &mut self,
        input: &mut Parser<'input, '_>,
    ) -> Result<Self::Prelude, ParseError<'input, Self::Error>> {
        let start = input.state();
        while input.next_including_whitespace_and_comments().is_ok() {}
        Ok(input.slice_from(start.position()).trim().to_owned())
    }

    #[inline]
    fn parse_block<'input>(
        &mut self,
        prelude: Self::Prelude,
        _state: &ParserState,
        input: &mut Parser<'input, '_>,
    ) -> Result<Self::QualifiedRule, ParseError<'input, Self::Error>> {
        let decls = parse_declarations_from_block(input);
        Ok(StyleRule {
            prelude,
            declarations: decls,
        })
    }
}

/// Parse declarations from a rule block using `cssparser` body parser.
fn parse_declarations_from_block(block: &mut Parser) -> Vec<Declaration> {
    let mut out: Vec<Declaration> = Vec::new();
    let mut body = BodyDeclParser;
    for decl in CssRuleBodyParser::new(block, &mut body).flatten() {
        out.push(decl);
    }
    out
}

/// Parse a full stylesheet into a `Stylesheet` using cssparser.
pub fn parse_stylesheet(css: &str) -> Stylesheet {
    let mut input = ParserInput::new(css);
    let mut parser = Parser::new(&mut input);
    let mut top = TopLevelParser;
    let mut sheet = Stylesheet::default();
    for rule in StyleSheetParser::new(&mut parser, &mut top).flatten() {
        sheet.rules.push(rule);
    }
    sheet
}
