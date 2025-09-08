use crate::selector::{Combinator, ComplexSelector, CompoundSelector, SimpleSelector, Specificity};
use crate::types::{Declaration, Origin, StyleRule, Stylesheet};
use cssparser::{AtRuleParser as CssAtRuleParser, BasicParseErrorKind, CowRcStr, DeclarationParser as CssDeclarationParser, ParseError, Parser, ParserInput, ParserState, QualifiedRuleParser as CssQualifiedRuleParser, RuleBodyItemParser as CssRuleBodyItemParser, RuleBodyParser as CssRuleBodyParser, StyleSheetParser};
use std::borrow::Cow;

/// Streaming CSS parser that accepts chunks and emits rules as they are completed.
pub struct StylesheetStreamParser {
    origin: Origin,
    next_source_order: u32,
    buffer: String,
    scan_pos: usize,
}

impl StylesheetStreamParser {
    pub fn new(origin: Origin, order_base: u32) -> Self {
        Self { origin, next_source_order: order_base, buffer: String::new(), scan_pos: 0 }
    }

    /// Feed a chunk of CSS text. Any fully-formed rules found in the
    /// accumulated buffer are parsed and appended to the provided stylesheet.
    pub fn push_chunk(&mut self, chunk: &str, out: &mut Stylesheet) {
        self.buffer.push_str(chunk);
        self.process_available(out);
    }

    /// Finish parsing and return any collected rules. This will attempt to
    /// process any remaining complete rule in the buffer. Incomplete trailing
    /// data is ignored.
    pub fn finish(mut self) -> Stylesheet {
        let mut sheet = Stylesheet { rules: Vec::new() };
        self.process_available(&mut sheet);
        sheet
    }

    fn process_available(&mut self, out: &mut Stylesheet) {
        let tail = &self.buffer[self.scan_pos..];
        if tail.is_empty() { return; }

        let mut last_consumed = 0usize;
        let mut next_source_order = self.next_source_order;
        {
            let mut input = ParserInput::new(tail);
            let mut parser = Parser::new(&mut input);
            let mut tl = TopLevelParser { origin: self.origin, next_source_order };
            let mut sheet = StyleSheetParser::new(&mut parser, &mut tl);

            while let Some(item) = sheet.next() {
                match item {
                    Ok(rule) => {
                        last_consumed = sheet.input.position().byte_index();
                        out.rules.push(rule);
                        // next_source_order is advanced inside TopLevelParser when building a rule
                    }
                    Err((_e, _slice)) => {
                        // Likely invalid or incomplete rule; stop here and wait for more data.
                        break;
                    }
                }
            }
            next_source_order = tl.next_source_order;
        }

        // Update our next_source_order from the top-level parser state
        self.next_source_order = next_source_order;

        // Advance scan_pos by the number of bytes actually consumed.
        self.scan_pos += last_consumed;
        self.maybe_compact_buffer();
    }

    fn maybe_compact_buffer(&mut self) {
        // If the processed prefix is large, drop it to cap memory.
        if self.scan_pos > 64 * 1024 {
            let tail = self.buffer[self.scan_pos..].to_string();
            self.buffer = tail;
            self.scan_pos = 0;
        }
    }
}

// Top-level rule parser used by StyleSheetParser to build StyleRule items.
struct TopLevelParser {
    origin: Origin,
    next_source_order: u32,
}

impl<'i> CssAtRuleParser<'i> for TopLevelParser {
    type Prelude = ();
    type AtRule = StyleRule;
    type Error = ();
}

impl<'i> CssQualifiedRuleParser<'i> for TopLevelParser {
    type Prelude = String; // raw selector prelude text
    type QualifiedRule = StyleRule;
    type Error = ();

    fn parse_prelude<'t>(&mut self, input: &mut Parser<'i, 't>) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        let start = input.state();
        // Consume the whole prelude provided by the delimited parser
        while input.next_including_whitespace_and_comments().is_ok() {}
        let full = input.slice_from(start.position());
        Ok(full.trim().to_string())
    }

    fn parse_block<'t>(&mut self, prelude: Self::Prelude, _start: &ParserState, input: &mut Parser<'i, 't>) -> Result<Self::QualifiedRule, ParseError<'i, Self::Error>> {
        // The provided `input` is already scoped to the contents of the block.
        let decls: Vec<Declaration> = parse_declarations_from_block(input);
        let selectors = parse_selector_list(&prelude);
        if !selectors.is_empty() && !decls.is_empty() {
            let rule = StyleRule {
                selectors,
                declarations: decls,
                source_order: self.next_source_order,
                origin: self.origin,
            };
            self.next_source_order = self.next_source_order.saturating_add(1);
            Ok(rule)
        } else {
            Err(input.new_error(BasicParseErrorKind::QualifiedRuleInvalid))
        }
    }
}

// Body declaration/parser used inside blocks to extract declarations only.
struct BodyDeclParser;

impl<'i> CssDeclarationParser<'i> for BodyDeclParser {
    type Declaration = Declaration;
    type Error = ();

    fn parse_value<'t>(&mut self, name: CowRcStr<'i>, input: &mut Parser<'i, 't>, _decl_start: &ParserState) -> Result<Self::Declaration, ParseError<'i, Self::Error>> {
        let start_pos = input.position();
        // Consume the value until the end of the delimited parser
        while input.next_including_whitespace_and_comments().is_ok() {}
        let full = input.slice_from(start_pos);
        let value_trim = full.trim();
        let (value_no_imp, important) = if let Some(pos) = value_trim.rfind("!important") {
            (value_trim[..pos].trim_end().to_string(), true)
        } else {
            (value_trim.to_string(), false)
        };
        Ok(Declaration { name: name.to_ascii_lowercase(), value: value_no_imp, important })
    }
}

impl<'i> CssAtRuleParser<'i> for BodyDeclParser {
    type Prelude = ();
    type AtRule = Declaration; // Ignored; we won’t produce any
    type Error = ();
}

impl<'i> CssQualifiedRuleParser<'i> for BodyDeclParser {
    type Prelude = ();
    type QualifiedRule = Declaration; // Not used
    type Error = ();
}

impl<'i> CssRuleBodyItemParser<'i, Declaration, ()> for BodyDeclParser {
    fn parse_declarations(&self) -> bool { true }
    fn parse_qualified(&self) -> bool { false }
}

fn parse_declarations_from_block(block: &mut Parser) -> Vec<Declaration> {
    let mut decls = Vec::new();
    let mut parser = BodyDeclParser;
    for item in CssRuleBodyParser::new(block, &mut parser) {
        if let Ok(d) = item { decls.push(d); }
    }
    decls
}

pub fn parse_stylesheet(css: &str, origin: Origin, order_base: u32) -> Stylesheet {
    let css = strip_comments(css);
    let mut rules = Vec::new();
    let mut i = 0usize;
    let mut source_order = order_base;
    let bytes = css.as_bytes();

    while let Some(open) = find_next(bytes, i, b'{') {
        // prelude is between i..open
        let prelude = css[i..open].trim();
        if prelude.is_empty() {
            // Skip stray blocks
            if let Some(close) = find_matching_brace(bytes, open) { i = close + 1; continue; }
        }
        // find matching '}'
        if let Some(close) = find_matching_brace(bytes, open) {
            let body = &css[open + 1..close];
            let selectors = parse_selector_list(prelude);
            let declarations = parse_declarations(body);
            if !selectors.is_empty() && !declarations.is_empty() {
                rules.push(StyleRule {
                    selectors,
                    declarations,
                    source_order,
                    origin,
                });
                source_order = source_order.saturating_add(1);
            }
            i = close + 1;
        } else {
            break;
        }
    }

    Stylesheet { rules }
}

fn strip_comments(input: &str) -> Cow<'_, str> {
    if !input.contains("/*") { return Cow::Borrowed(input); }
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') { i += 1; }
            i = (i + 2).min(bytes.len());
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    Cow::Owned(out)
}

fn find_next(bytes: &[u8], start: usize, needle: u8) -> Option<usize> {
    for i in start..bytes.len() {
        if bytes[i] == needle { return Some(i); }
    }
    None
}

fn find_matching_brace(bytes: &[u8], open_pos: usize) -> Option<usize> {
    // open_pos is at '{'
    let mut depth = 0i32;
    for i in open_pos..bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 { return Some(i); }
            }
            _ => {}
        }
    }
    None
}

fn parse_selector_list(list: &str) -> Vec<ComplexSelector> {
    let mut selectors = Vec::new();
    for part in list.split(',') {
        let s = part.trim();
        if s.is_empty() { continue; }
        if let Some(sel) = parse_complex_selector(s) {
            selectors.push(sel);
        }
    }
    selectors
}

fn parse_complex_selector(input: &str) -> Option<ComplexSelector> {
    // Tokenize by combinators while keeping whitespace combinator semantics
    let mut sequence: Vec<(CompoundSelector, Option<Combinator>)> = Vec::new();
    let mut current = CompoundSelector { simples: Vec::new() };

    let mut i = 0usize;
    let chars: Vec<char> = input.chars().collect();

    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\n' | '\r' => { i += 1; }
            '>' | '+' | '~' => {
                // Push current compound if not empty
                if !current.simples.is_empty() {
                    sequence.push((current, None));
                    current = CompoundSelector { simples: Vec::new() };
                }
                // Mark that the previous step has combinator to next
                if let Some(last) = sequence.last_mut() {
                    last.1 = Some(match c { '>' => Combinator::Child, '+' => Combinator::NextSibling, _ => Combinator::SubsequentSibling });
                }
                i += 1;
            }
            '*' => { current.simples.push(SimpleSelector::Universal); i += 1; }
            '#' => {
                i += 1; let (ident, ni) = read_ident(&chars, i); i = ni; if !ident.is_empty() { current.simples.push(SimpleSelector::Id(ident)); }
            }
            '.' => {
                i += 1; let (ident, ni) = read_ident(&chars, i); i = ni; if !ident.is_empty() { current.simples.push(SimpleSelector::Class(ident)); }
            }
            _ => {
                // type selector (ident)
                let (ident, ni) = read_ident(&chars, i);
                if !ident.is_empty() { current.simples.push(SimpleSelector::Type(ident)); }
                i = ni;
            }
        }
    }

    // push last compound
    if !current.simples.is_empty() {
        sequence.push((current, None));
    }

    // Insert implicit descendant combinators between adjacent compounds without explicit combinator
    for i in 0..sequence.len().saturating_sub(1) {
        if sequence[i].1.is_none() {
            sequence[i].1 = Some(Combinator::Descendant);
        }
    }

    if sequence.is_empty() { return None; }

    // compute specificity
    let mut a = 0u32; // ids
    let mut b = 0u32; // classes/attributes/pseudos (we only do class)
    let mut c = 0u32; // type/universal (universal counts as 0 usually; we'll count type only)
    for (comp, _) in &sequence {
        for s in &comp.simples {
            match s {
                SimpleSelector::Id(_) => a += 1,
                SimpleSelector::Class(_) => b += 1,
                SimpleSelector::Type(_) => c += 1,
                SimpleSelector::Universal => {}
            }
        }
    }

    Some(ComplexSelector { sequence, specificity: Specificity::from_counts(a, b, c) })
}

fn read_ident(chars: &[char], mut i: usize) -> (String, usize) {
    let start = i;
    while i < chars.len() {
        let c = chars[i];
        if c.is_alphanumeric() || c == '-' || c == '_' { i += 1; } else { break; }
    }
    (chars[start..i].iter().collect(), i)
}

pub fn parse_declarations(block: &str) -> Vec<Declaration> {
    let mut decls = Vec::new();
    let mut i = 0usize;
    let chars: Vec<char> = block.chars().collect();

    while i < chars.len() {
        // skip whitespace and semicolons
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ';') { i += 1; }
        if i >= chars.len() { break; }

        // read property name
        let name_start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != ':' && chars[i] != ';' && chars[i] != '}' { i += 1; }
        let name: String = chars[name_start..i].iter().collect::<String>().trim().to_string();
        // skip whitespace
        while i < chars.len() && chars[i].is_whitespace() { i += 1; }
        if i >= chars.len() || chars[i] != ':' { // invalid; skip to next semicolon
            i = skip_to_semicolon(&chars, i);
            continue;
        }
        i += 1; // skip ':'

        // read value until ';' or end, respecting (), [], {}, quotes
        let (value, ni) = read_css_value(&chars, i);
        i = ni;

        // parse !important if present at end
        let value_trim = value.trim_end().to_string();
        let (value_no_imp, important) = if let Some(pos) = value_trim.rfind("!important") {
            let before = &value_trim[..pos];
            (before.trim_end().to_string(), true)
        } else {
            (value_trim, false)
        };

        if !name.is_empty() && !value_no_imp.is_empty() {
            decls.push(Declaration { name: name.to_lowercase(), value: value_no_imp, important });
        }

        // if current char is ';', consume it
        if i < chars.len() && chars[i] == ';' { i += 1; }
    }

    decls
}

fn skip_to_semicolon(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i] != ';' { i += 1; }
    if i < chars.len() { i + 1 } else { i }
}

fn read_css_value(chars: &[char], mut i: usize) -> (String, usize) {
    let mut out = String::new();
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut depth_brace = 0i32;
    let mut in_string: Option<char> = None;

    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_string {
            out.push(c);
            if c == '\\' { // escape, include next char
                if i + 1 < chars.len() { out.push(chars[i + 1]); i += 2; continue; } else { i += 1; continue; }
            }
            if c == q { in_string = None; }
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' => { in_string = Some(c); out.push(c); i += 1; }
            '(' => { depth_paren += 1; out.push(c); i += 1; }
            ')' => { if depth_paren > 0 { depth_paren -= 1; } out.push(c); i += 1; }
            '[' => { depth_brack += 1; out.push(c); i += 1; }
            ']' => { if depth_brack > 0 { depth_brack -= 1; } out.push(c); i += 1; }
            '{' => { depth_brace += 1; out.push(c); i += 1; }
            '}' => { if depth_brace > 0 { depth_brace -= 1; out.push(c); i += 1; } else { break; } }
            ';' => { if depth_paren == 0 && depth_brack == 0 && depth_brace == 0 { break; } else { out.push(c); i += 1; } }
            _ => { out.push(c); i += 1; }
        }
    }
    (out, i)
}
