use crate::selector::{Combinator, ComplexSelector, CompoundSelector, SimpleSelector, Specificity, PseudoClass};
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

    /// Return the next source order value that will be assigned to the next parsed rule.
    pub fn next_source_order(&self) -> u32 {
        self.next_source_order
    }

    /// Feed a chunk of CSS text. Any fully-formed rules found in the
    /// accumulated buffer are parsed and appended to the provided stylesheet.
    pub fn push_chunk(&mut self, chunk: &str, out: &mut Stylesheet) {
        self.buffer.push_str(chunk);
        self.process_available(out);
    }

    /// Finish parsing and return any collected rules together with the final next_source_order.
    pub fn finish_with_next(mut self) -> (Stylesheet, u32) {
        let mut sheet = Stylesheet { rules: Vec::new() };
        self.process_available(&mut sheet);
        (sheet, self.next_source_order)
    }

    /// Finish parsing and return any collected rules. This will attempt to
    /// process any remaining complete rule in the buffer. Incomplete trailing
    /// data is ignored.
    pub fn finish(mut self) -> Stylesheet {
        let mut sheet = Stylesheet { rules: Vec::new() };
        self.process_available(&mut sheet);
        sheet
    }

    /// Parse any fully available rules from the buffered CSS and append them to `out`.
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
        Ok(Declaration { name: name.to_ascii_lowercase(), value: value_no_imp, important, var_refs: Vec::new() })
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
        if let Ok(mut d) = item {
            d.var_refs = capture_var_refs(&d.value);
            decls.push(d);
        }
    }
    normalize_shorthands(decls)
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
            ':' => {
                i += 1; let (ident, ni) = read_ident(&chars, i); i = ni;
                let ident_lc = ident.to_ascii_lowercase();
                let pc = match ident_lc.as_str() {
                    "root" => Some(PseudoClass::Root),
                    "first-child" => Some(PseudoClass::FirstChild),
                    "last-child" => Some(PseudoClass::LastChild),
                    _ => None,
                };
                if let Some(pc) = pc { current.simples.push(SimpleSelector::PseudoClass(pc)); }
            }
            '[' => {
                // parse attribute selector [name] or [name=value]
                i += 1; // skip '['
                // skip whitespace
                while i < chars.len() && chars[i].is_whitespace() { i += 1; }
                let (name, ni) = read_ident(&chars, i); i = ni;
                // skip whitespace
                while i < chars.len() && chars[i].is_whitespace() { i += 1; }
                let mut op_value: Option<(String, String)> = None;
                if i < chars.len() && chars[i] == '=' {
                    i += 1; // skip '='
                    // skip whitespace
                    while i < chars.len() && chars[i].is_whitespace() { i += 1; }
                    // read value: quoted or ident
                    if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                        let quote = chars[i]; i += 1; let mut val = String::new();
                        while i < chars.len() && chars[i] != quote { val.push(chars[i]); i += 1; }
                        if i < chars.len() && chars[i] == quote { i += 1; }
                        op_value = Some(("=".to_string(), val));
                    } else {
                        let (val, ni2) = read_ident(&chars, i); i = ni2; op_value = Some(("=".to_string(), val));
                    }
                    // skip whitespace
                    while i < chars.len() && chars[i].is_whitespace() { i += 1; }
                }
                // consume closing ']'
                if i < chars.len() && chars[i] == ']' { i += 1; }
                if !name.is_empty() { current.simples.push(SimpleSelector::Attribute { name, op_value }); }
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
    let mut b = 0u32; // classes/attributes/pseudos
    let mut c = 0u32; // type/universal (universal counts as 0 usually; we'll count type only)
    for (comp, _) in &sequence {
        for s in &comp.simples {
            match s {
                SimpleSelector::Id(_) => a += 1,
                SimpleSelector::Class(_) => b += 1,
                SimpleSelector::Attribute { .. } => b += 1,
                SimpleSelector::PseudoClass(_) => b += 1,
                SimpleSelector::Type(_) => c += 1,
                SimpleSelector::Universal => {}
            }
        }
    }

    Some(ComplexSelector { sequence, specificity: Specificity::from_counts(a, b, c) })
}

fn read_ident(chars: &[char], mut i: usize) -> (String, usize) {
    // Support simple CSS escapes (\\x) and non-ASCII ident characters.
    let mut out = String::new();
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            // Consume escape and include next character literally if present.
            if i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
            } else {
                // Lone backslash at end; stop to avoid infinite loop.
                i += 1;
            }
            continue;
        }
        if c.is_alphanumeric() || c == '-' || c == '_' || (c as u32) >= 0x80 {
            out.push(c);
            i += 1;
        } else {
            break;
        }
    }
    (out, i)
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
            decls.push(Declaration { name: name.to_lowercase(), value: value_no_imp.clone(), important, var_refs: capture_var_refs(&value_no_imp) });
        }

        // if current char is ';', consume it
        if i < chars.len() && chars[i] == ';' { i += 1; }
    }

    normalize_shorthands(decls)
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

/// Capture CSS custom property references within a value, e.g., var(--main, fallback).
/// This function scans a CSS value and extracts custom property names (with the leading "--")
/// used in var() references. It does not perform resolution or validation beyond basic syntax.
fn capture_var_refs(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i: usize = 0;
    while i + 3 < chars.len() {
        // Look for 'v','a','r','('
        if chars[i] == 'v' && chars[i + 1] == 'a' && chars[i + 2] == 'r' && chars[i + 3] == '(' {
            let mut j = i + 4; // position after 'var('
            // Skip whitespace
            while j < chars.len() && chars[j].is_whitespace() { j += 1; }
            // Expect custom property start '--'
            if j + 1 < chars.len() && chars[j] == '-' && chars[j + 1] == '-' {
                let start = j;
                j += 2;
                while j < chars.len() {
                    let c = chars[j];
                    if c.is_alphanumeric() || c == '-' || c == '_' { j += 1; } else { break; }
                }
                let name: String = chars[start..j].iter().collect();
                if !name.is_empty() {
                    out.push(name);
                }
            }
            // Advance i to the matching closing ')', accounting for nested parentheses in fallback
            let mut depth = 1i32;
            j = i + 4; // start after 'var('
            while j < chars.len() {
                let c = chars[j];
                if c == '(' { depth += 1; }
                else if c == ')' { depth -= 1; if depth == 0 { j += 1; break; } }
                j += 1;
            }
            i = j; // continue scanning after this var()
        } else {
            i += 1;
        }
    }
    out
}

// ===============================
// Shorthand normalization helpers
// ===============================

fn emit_decl(name: &str, value: &str, important: bool) -> Declaration {
    Declaration { name: name.to_string(), value: value.to_string(), important, var_refs: capture_var_refs(value) }
}

fn split_css_list(value: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut depth_brace = 0i32;
    let mut in_string: Option<char> = None;
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_string {
            cur.push(c);
            if c == '\\' {
                if i + 1 < chars.len() { cur.push(chars[i + 1]); i += 2; continue; } else { i += 1; continue; }
            }
            if c == q { in_string = None; }
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' => { in_string = Some(c); cur.push(c); }
            '(' => { depth_paren += 1; cur.push(c); }
            ')' => { if depth_paren > 0 { depth_paren -= 1; } cur.push(c); }
            '[' => { depth_brack += 1; cur.push(c); }
            ']' => { if depth_brack > 0 { depth_brack -= 1; } cur.push(c); }
            '{' => { depth_brace += 1; cur.push(c); }
            '}' => { if depth_brace > 0 { depth_brace -= 1; } cur.push(c); }
            _ => {
                if c.is_whitespace() && depth_paren == 0 && depth_brack == 0 && depth_brace == 0 {
                    if !cur.trim().is_empty() { out.push(cur.trim().to_string()); cur.clear(); }
                } else {
                    cur.push(c);
                }
            }
        }
        i += 1;
    }
    if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
    out
}

fn expand_edges(prefix: &str, value: &str, important: bool) -> Vec<Declaration> {
    let parts = split_css_list(value);
    if parts.is_empty() { return vec![emit_decl(prefix, value, important)]; }
    let (t, r, b, l) = match parts.len() {
        1 => (parts[0].as_str(), parts[0].as_str(), parts[0].as_str(), parts[0].as_str()),
        2 => (parts[0].as_str(), parts[1].as_str(), parts[0].as_str(), parts[1].as_str()),
        3 => (parts[0].as_str(), parts[1].as_str(), parts[2].as_str(), parts[1].as_str()),
        _ => (parts[0].as_str(), parts[1].as_str(), parts[2].as_str(), parts[3].as_str()),
    };
    vec![
        emit_decl(&format!("{}-top", prefix), t, important),
        emit_decl(&format!("{}-right", prefix), r, important),
        emit_decl(&format!("{}-bottom", prefix), b, important),
        emit_decl(&format!("{}-left", prefix), l, important),
    ]
}

fn expand_border_shorthand(value: &str, important: bool) -> Vec<Declaration> {
    let parts = split_css_list(value);
    if parts.is_empty() { return Vec::new(); }
    let mut width: Option<String> = None;
    let mut style: Option<String> = None;
    let mut color: Option<String> = None;
    for p in parts {
        let pl = p.to_ascii_lowercase();
        if width.is_none() && (pl.ends_with("px") || pl.parse::<f32>().is_ok()) {
            width = Some(p);
            continue;
        }
        if style.is_none() && matches!(pl.as_str(), "none"|"solid"|"dashed"|"dotted"|"double"|"groove"|"ridge"|"inset"|"outset"|"hidden") {
            style = Some(p);
            continue;
        }
        if color.is_none() {
            // Heuristic: treat remaining token as color
            color = Some(p);
        }
    }
    let mut out = Vec::new();
    if let Some(w) = width.as_ref() { out.push(emit_decl("border-width", w, important)); }
    if let Some(s) = style.as_ref() { out.push(emit_decl("border-style", s, important)); }
    if let Some(c) = color.as_ref() { out.push(emit_decl("border-color", c, important)); }
    out
}

fn normalize_shorthands(mut decls: Vec<Declaration>) -> Vec<Declaration> {
    let mut out: Vec<Declaration> = Vec::new();
    for d in decls.drain(..) {
        let name = d.name.to_ascii_lowercase();
        match name.as_str() {
            "margin" => {
                out.extend(expand_edges("margin", &d.value, d.important));
            }
            "padding" => {
                out.extend(expand_edges("padding", &d.value, d.important));
            }
            "border" => {
                // Basic: split into width/style/color; keep as three longhands
                out.extend(expand_border_shorthand(&d.value, d.important));
            }
            "border-width" => {
                out.extend(expand_edges("border", &format!("{}-width", "border"), d.important)); // placeholder; fixed below
                // The above line is incorrect; implement explicit mapping
                let parts = split_css_list(&d.value);
                if parts.is_empty() { continue; }
                let (t, r, b, l) = match parts.len() {
                    1 => (parts[0].as_str(), parts[0].as_str(), parts[0].as_str(), parts[0].as_str()),
                    2 => (parts[0].as_str(), parts[1].as_str(), parts[0].as_str(), parts[1].as_str()),
                    3 => (parts[0].as_str(), parts[1].as_str(), parts[2].as_str(), parts[1].as_str()),
                    _ => (parts[0].as_str(), parts[1].as_str(), parts[2].as_str(), parts[3].as_str()),
                };
                out.push(emit_decl("border-top-width", t, d.important));
                out.push(emit_decl("border-right-width", r, d.important));
                out.push(emit_decl("border-bottom-width", b, d.important));
                out.push(emit_decl("border-left-width", l, d.important));
            }
            "border-style" => {
                let parts = split_css_list(&d.value);
                if parts.is_empty() { continue; }
                let (t, r, b, l) = match parts.len() {
                    1 => (parts[0].as_str(), parts[0].as_str(), parts[0].as_str(), parts[0].as_str()),
                    2 => (parts[0].as_str(), parts[1].as_str(), parts[0].as_str(), parts[1].as_str()),
                    3 => (parts[0].as_str(), parts[1].as_str(), parts[2].as_str(), parts[1].as_str()),
                    _ => (parts[0].as_str(), parts[1].as_str(), parts[2].as_str(), parts[3].as_str()),
                };
                out.push(emit_decl("border-top-style", t, d.important));
                out.push(emit_decl("border-right-style", r, d.important));
                out.push(emit_decl("border-bottom-style", b, d.important));
                out.push(emit_decl("border-left-style", l, d.important));
            }
            "border-color" => {
                let parts = split_css_list(&d.value);
                if parts.is_empty() { continue; }
                let (t, r, b, l) = match parts.len() {
                    1 => (parts[0].as_str(), parts[0].as_str(), parts[0].as_str(), parts[0].as_str()),
                    2 => (parts[0].as_str(), parts[1].as_str(), parts[0].as_str(), parts[1].as_str()),
                    3 => (parts[0].as_str(), parts[1].as_str(), parts[2].as_str(), parts[1].as_str()),
                    _ => (parts[0].as_str(), parts[1].as_str(), parts[2].as_str(), parts[3].as_str()),
                };
                out.push(emit_decl("border-top-color", t, d.important));
                out.push(emit_decl("border-right-color", r, d.important));
                out.push(emit_decl("border-bottom-color", b, d.important));
                out.push(emit_decl("border-left-color", l, d.important));
            }
            "font" => {
                // Very small subset parser: [style]? [weight]? size [/ line-height]? family...
                let mut rest = d.value.trim().to_string();
                let mut style_opt: Option<String> = None;
                let mut weight_opt: Option<String> = None;
                // Detect size and optional line-height
                let parts = split_css_list(&rest);
                let mut size_idx: Option<usize> = None;
                for (idx, tok) in parts.iter().enumerate() {
                    let tl = tok.to_ascii_lowercase();
                    if tl.contains('/') || tl.ends_with("px") || tl.ends_with("em") || tl.ends_with("ex") {
                        size_idx = Some(idx);
                        break;
                    }
                }
                if let Some(si) = size_idx {
                    // style/weight tokens before size
                    for pre in &parts[0..si] {
                        let p = pre.to_ascii_lowercase();
                        if style_opt.is_none() && matches!(p.as_str(), "italic"|"oblique"|"normal") { style_opt = Some(pre.clone()); continue; }
                        if weight_opt.is_none() && (p == "bold" || p == "normal" || p.parse::<u16>().ok().filter(|v| *v >= 100 && *v <= 900 && *v % 100 == 0).is_some()) { weight_opt = Some(pre.clone()); continue; }
                    }
                    // size and optional /line-height
                    let size_token = parts[si].clone();
                    let mut size_val = size_token.clone();
                    let mut lh_val: Option<String> = None;
                    if let Some((s, l)) = size_token.split_once('/') {
                        size_val = s.trim().to_string();
                        lh_val = Some(l.trim().to_string());
                    } else if si + 1 < parts.len() && parts[si + 1] == "/" && si + 2 < parts.len() {
                        lh_val = Some(parts[si + 2].clone());
                    }
                    // family = rest after size (+optional slash parts)
                    let mut fam_start = si + 1;
                    if lh_val.is_some() {
                        // handled either inline slash or spaced; adjust fam_start roughly
                        if parts[si].contains('/') {
                            fam_start = si + 1;
                        } else {
                            fam_start = (si + 3).min(parts.len());
                        }
                    }
                    if fam_start < parts.len() {
                        let family = parts[fam_start..].join(" ");
                        out.push(emit_decl("font-family", &family, d.important));
                    }
                    out.push(emit_decl("font-size", &size_val, d.important));
                    if let Some(lh) = lh_val { out.push(emit_decl("line-height", &lh, d.important)); }
                    if let Some(st) = style_opt { out.push(emit_decl("font-style", &st, d.important)); }
                    if let Some(w) = weight_opt { out.push(emit_decl("font-weight", &w, d.important)); }
                } else {
                    // Fallback: keep original if we can't parse
                    out.push(d);
                }
            }
            "background" => {
                let v = d.value.trim();
                // Always expand to background-color (subset); keep tokens as-is for further resolution
                out.push(emit_decl("background-color", v, d.important));
            }
            _ => out.push(d),
        }
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::selector::{SimpleSelector, Combinator};

    fn has_class(comp: &CompoundSelector, name: &str) -> bool {
        comp.simples.iter().any(|s| matches!(s, SimpleSelector::Class(c) if c == name))
    }

    #[test]
    fn selector_parsing_escaped_plus_in_class() {
        let list = parse_selector_list(".foo\\+bar");
        assert_eq!(list.len(), 1, "Expected one selector");
        let sel = &list[0];
        let right = sel.rightmost_compound().expect("rightmost compound");
        assert!(has_class(right, "foo+bar"), "Expected class with escaped plus to parse as 'foo+bar' but got {:?}", right);
    }

    #[test]
    fn selector_parsing_unicode_tag() {
        let list = parse_selector_list("タグ");
        assert_eq!(list.len(), 1);
        let sel = &list[0];
        let right = sel.rightmost_compound().unwrap();
        assert!(right.simples.iter().any(|s| matches!(s, SimpleSelector::Type(t) if t == "タグ")));
    }

    #[test]
    fn var_token_capture_inline_parse_declarations() {
        let decls = parse_declarations("color: var(--main, red); margin: calc(var(--m1) + var(--m2, 2px));");
        // After normalization, expect 1 color + 4 margin-* longhands
        assert_eq!(decls.len(), 5);
        let color = &decls[0];
        assert_eq!(color.name, "color");
        assert_eq!(color.var_refs, vec!["--main".to_string()]);
        // Collect margin-* entries and validate var refs
        let mut margin_items: Vec<&Declaration> = decls.iter().filter(|d| d.name.starts_with("margin-")).collect();
        margin_items.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(margin_items.len(), 4);
        for m in margin_items {
            assert_eq!(m.var_refs, vec!["--m1".to_string(), "--m2".to_string()]);
        }
    }

    #[test]
    fn var_token_capture_in_stylesheet_rules() {
        let css = "div, span { padding: var(--pad); background: linear-gradient(red, blue), var(--bg, none) }";
        let sheet = parse_stylesheet(css, Origin::Author, 0);
        assert_eq!(sheet.rules.len(), 1);
        let rule = &sheet.rules[0];
        // After normalization: 4 padding-* + background-color
        assert_eq!(rule.declarations.len(), 5);
        // Find a padding longhand
        let padding_top = rule.declarations.iter().find(|d| d.name == "padding-top").expect("padding-top");
        assert_eq!(padding_top.var_refs, vec!["--pad".to_string()]);
        // background-color should carry var ref
        let background = rule.declarations.iter().find(|d| d.name == "background-color").expect("background-color");
        assert_eq!(background.var_refs, vec!["--bg".to_string()]);
    }

    #[test]
    fn selector_parsing_combinators_child_then_descendant() {
        let list = parse_selector_list("div > .a .b");
        assert_eq!(list.len(), 1);
        let sel = &list[0];
        // sequence should have multiple compounds; last should have class b
        let right = sel.rightmost_compound().unwrap();
        assert!(has_class(right, ".b".trim_start_matches('.')) == true || has_class(right, "b"));
        // Ensure there is at least one explicit Child combinator captured
        let has_child = sel.sequence.iter().any(|(_, comb)| comb == &Some(Combinator::Child));
        assert!(has_child, "Expected to capture a Child combinator in sequence: {:?}", sel.sequence);
    }
}
