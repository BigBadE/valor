//! Grid template parsing using cssparser.

use css_grid::{
    GridAxisTracks, GridTrack, GridTrackSize, TrackBreadth, TrackListType, TrackRepeat,
};
use cssparser::{ParseError, Parser, ParserInput, Token};
use std::iter::repeat_n;

/// Parse grid template (grid-template-columns or grid-template-rows).
pub fn parse_grid_template(template: &str, gap: f32) -> GridAxisTracks {
    let mut input = ParserInput::new(template.trim());
    let mut parser = Parser::new(&mut input);

    let mut tracks = Vec::new();
    let mut auto_repeat = None;

    loop {
        // Skip whitespace - failures are expected and intentional
        drop(parser.try_parse(cssparser::Parser::expect_whitespace));

        // Check if we've reached the end
        if parser.is_exhausted() {
            break;
        }

        // Try to parse next track size
        let token = match parser.next() {
            Ok(tok) => tok.clone(),
            Err(_) => break,
        };

        if !parse_track_token(&mut parser, &token, &mut tracks, &mut auto_repeat) {
            break;
        }
    }

    if let Some(repeat) = auto_repeat {
        GridAxisTracks::with_auto_repeat(tracks, gap, repeat)
    } else {
        GridAxisTracks::new(tracks, gap)
    }
}

/// Parse a single track token and add it to the tracks list.
/// Returns `false` if parsing should stop.
fn parse_track_token<'input>(
    parser: &mut Parser<'input, '_>,
    token: &Token<'input>,
    tracks: &mut Vec<GridTrack>,
    auto_repeat: &mut Option<TrackRepeat>,
) -> bool {
    match token {
        Token::Function(name) if name.eq_ignore_ascii_case("repeat") => {
            parse_repeat_function(parser, tracks, auto_repeat)
        }
        Token::Function(name) if name.eq_ignore_ascii_case("minmax") => {
            parse_minmax_function(parser, tracks)
        }
        Token::Dimension { value, unit, .. } => parse_dimension_token(*value, unit, tracks),
        Token::Percentage { unit_value, .. } => {
            tracks.push(GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Percentage(*unit_value)),
                track_type: TrackListType::Explicit,
            });
            true
        }
        Token::Number { value: 0.0, .. } => {
            tracks.push(GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Length(0.0)),
                track_type: TrackListType::Explicit,
            });
            true
        }
        Token::Ident(name) => parse_ident_token(name, tracks),
        _ => {
            // Unknown token, stop parsing
            false
        }
    }
}

/// Parse a `repeat()` function.
/// Returns `false` if parsing should stop.
fn parse_repeat_function(
    parser: &mut Parser<'_, '_>,
    tracks: &mut Vec<GridTrack>,
    auto_repeat: &mut Option<TrackRepeat>,
) -> bool {
    match parser.parse_nested_block(|parser| {
        Ok::<Option<RepeatResult>, ParseError<'_, ()>>(parse_repeat(parser))
    }) {
        Ok(Some(result)) => match result {
            RepeatResult::Explicit(count, track_size) => {
                tracks.extend(repeat_n(
                    GridTrack {
                        size: track_size,
                        track_type: TrackListType::Explicit,
                    },
                    count,
                ));
                true
            }
            RepeatResult::Auto(repeat) => {
                *auto_repeat = Some(repeat);
                true
            }
        },
        Ok(None) | Err(_) => {
            // Invalid repeat syntax or parse error, stop parsing
            false
        }
    }
}

/// Parse a `minmax()` function.
/// Returns `false` if parsing should stop.
fn parse_minmax_function(parser: &mut Parser<'_, '_>, tracks: &mut Vec<GridTrack>) -> bool {
    parser
        .parse_nested_block(|parser| parse_minmax(parser))
        .is_ok_and(|track_size| {
            tracks.push(GridTrack {
                size: track_size,
                track_type: TrackListType::Explicit,
            });
            true
        })
}

/// Parse a dimension token (e.g., "200px" or "1fr").
/// Returns `false` if parsing should stop.
fn parse_dimension_token(value: f32, unit: &str, tracks: &mut Vec<GridTrack>) -> bool {
    let unit_lower = unit.to_ascii_lowercase();
    match unit_lower.as_str() {
        "px" => {
            tracks.push(GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Length(value)),
                track_type: TrackListType::Explicit,
            });
            true
        }
        "fr" => {
            tracks.push(GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Flex(value)),
                track_type: TrackListType::Explicit,
            });
            true
        }
        _ => {
            // Unknown unit, stop parsing
            false
        }
    }
}

/// Parse an identifier token (e.g., "auto", "min-content", "max-content").
/// Returns `false` if parsing should stop.
fn parse_ident_token(name: &str, tracks: &mut Vec<GridTrack>) -> bool {
    let name_lower = name.to_ascii_lowercase();
    match name_lower.as_str() {
        "auto" => {
            tracks.push(GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Auto),
                track_type: TrackListType::Explicit,
            });
            true
        }
        "min-content" => {
            tracks.push(GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::MinContent),
                track_type: TrackListType::Explicit,
            });
            true
        }
        "max-content" => {
            tracks.push(GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::MaxContent),
                track_type: TrackListType::Explicit,
            });
            true
        }
        _ => {
            // Unknown identifier, stop parsing
            false
        }
    }
}

/// Result of parsing `repeat()` function.
enum RepeatResult {
    Explicit(usize, GridTrackSize),
    Auto(TrackRepeat),
}

/// Parse `repeat()` function content.
fn parse_repeat(parser: &mut Parser) -> Option<RepeatResult> {
    // First argument: count or auto-fit/auto-fill
    let token = parser.next().ok()?;

    match token {
        Token::Number {
            int_value: Some(count),
            ..
        } if *count > 0 => {
            // Explicit count: repeat(3, 200px)
            let count_val = *count as usize;
            parser.expect_comma().ok()?;
            let track_size = parse_track_size(parser)?;
            Some(RepeatResult::Explicit(count_val, track_size))
        }
        Token::Ident(name) if name.eq_ignore_ascii_case("auto-fit") => {
            // Auto-fit: repeat(auto-fit, minmax(200px, 1fr))
            parser.expect_comma().ok()?;
            let track_sizes = parse_track_list(parser)?;
            Some(RepeatResult::Auto(TrackRepeat::AutoFit(track_sizes)))
        }
        Token::Ident(name) if name.eq_ignore_ascii_case("auto-fill") => {
            // Auto-fill: repeat(auto-fill, minmax(200px, 1fr))
            parser.expect_comma().ok()?;
            let track_sizes = parse_track_list(parser)?;
            Some(RepeatResult::Auto(TrackRepeat::AutoFill(track_sizes)))
        }
        _ => None,
    }
}

/// Parse a track size (can be breadth, minmax, or fit-content).
fn parse_track_size(parser: &mut Parser) -> Option<GridTrackSize> {
    let start = parser.state();
    let token = parser.next().ok()?;

    match token {
        Token::Function(name) if name.eq_ignore_ascii_case("minmax") => {
            parser.parse_nested_block(parse_minmax).ok()
        }
        _ => {
            parser.reset(&start);
            let breadth = parse_track_breadth(parser)?;
            Some(GridTrackSize::Breadth(breadth))
        }
    }
}

/// Parse `minmax()` function.
///
/// # Errors
/// Returns an error if the `minmax()` syntax is invalid.
fn parse_minmax<'input>(
    parser: &mut Parser<'input, '_>,
) -> Result<GridTrackSize, ParseError<'input, ()>> {
    let min = parse_track_breadth(parser).ok_or_else(|| parser.new_custom_error(()))?;
    parser.expect_comma()?;
    let max = parse_track_breadth(parser).ok_or_else(|| parser.new_custom_error(()))?;
    Ok(GridTrackSize::MinMax(min, max))
}

/// Parse a list of track sizes for `repeat()`.
fn parse_track_list(parser: &mut Parser) -> Option<Vec<GridTrackSize>> {
    let mut tracks = Vec::new();

    loop {
        drop(parser.try_parse(cssparser::Parser::expect_whitespace));

        if let Some(track_size) = parse_track_size(parser) {
            tracks.push(track_size);
        } else {
            break;
        }
    }

    if tracks.is_empty() {
        None
    } else {
        Some(tracks)
    }
}

/// Parse a track breadth value.
fn parse_track_breadth(parser: &mut Parser) -> Option<TrackBreadth> {
    let token = parser.next().ok()?;

    match token {
        Token::Dimension { value, unit, .. } => {
            let unit_lower = unit.as_ref().to_ascii_lowercase();
            match unit_lower.as_str() {
                "px" => Some(TrackBreadth::Length(*value)),
                "fr" => Some(TrackBreadth::Flex(*value)),
                _ => None,
            }
        }
        Token::Percentage { unit_value, .. } => Some(TrackBreadth::Percentage(*unit_value)),
        Token::Number { value: 0.0, .. } => Some(TrackBreadth::Length(0.0)),
        Token::Ident(name) => {
            let name_lower = name.as_ref().to_ascii_lowercase();
            match name_lower.as_str() {
                "auto" => Some(TrackBreadth::Auto),
                "min-content" => Some(TrackBreadth::MinContent),
                "max-content" => Some(TrackBreadth::MaxContent),
                _ => None,
            }
        }
        _ => None,
    }
}
