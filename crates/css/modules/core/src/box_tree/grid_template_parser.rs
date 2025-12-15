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
        // Skip whitespace
        let _ = parser.try_parse(cssparser::Parser::expect_whitespace);

        // Check if we've reached the end
        if parser.is_exhausted() {
            break;
        }

        // Try to parse next track size
        let Ok(token) = parser.next() else {
            break;
        };

        match token {
            Token::Function(name) if name.eq_ignore_ascii_case("repeat") => {
                match parser.parse_nested_block(|parser| {
                    Ok::<Option<RepeatResult>, ParseError<'_, ()>>(parse_repeat(parser))
                }) {
                    Ok(Some(result)) => match result {
                        RepeatResult::Explicit(count, track_size) => {
                            tracks.extend(repeat_n(
                                GridTrack {
                                    size: track_size.clone(),
                                    track_type: TrackListType::Explicit,
                                },
                                count,
                            ));
                        }
                        RepeatResult::Auto(repeat) => {
                            auto_repeat = Some(repeat);
                        }
                    },
                    Ok(None) => {
                        // Invalid repeat syntax, stop parsing
                        break;
                    }
                    Err(_) => {
                        // Parse error in repeat(), stop parsing
                        break;
                    }
                }
            }
            Token::Function(name) if name.eq_ignore_ascii_case("minmax") => {
                match parser.parse_nested_block(|parser| parse_minmax(parser)) {
                    Ok(track_size) => {
                        tracks.push(GridTrack {
                            size: track_size,
                            track_type: TrackListType::Explicit,
                        });
                    }
                    Err(_) => {
                        // Parse error in minmax(), stop parsing
                        break;
                    }
                }
            }
            Token::Dimension { value, unit, .. } => {
                let unit_lower = unit.as_ref().to_ascii_lowercase();
                match unit_lower.as_str() {
                    "px" => {
                        tracks.push(GridTrack {
                            size: GridTrackSize::Breadth(TrackBreadth::Length(*value)),
                            track_type: TrackListType::Explicit,
                        });
                    }
                    "fr" => {
                        tracks.push(GridTrack {
                            size: GridTrackSize::Breadth(TrackBreadth::Flex(*value)),
                            track_type: TrackListType::Explicit,
                        });
                    }
                    _ => {
                        // Unknown unit, stop parsing
                        break;
                    }
                }
            }
            Token::Percentage { unit_value, .. } => {
                tracks.push(GridTrack {
                    size: GridTrackSize::Breadth(TrackBreadth::Percentage(*unit_value)),
                    track_type: TrackListType::Explicit,
                });
            }
            Token::Number { value: 0.0, .. } => {
                tracks.push(GridTrack {
                    size: GridTrackSize::Breadth(TrackBreadth::Length(0.0)),
                    track_type: TrackListType::Explicit,
                });
            }
            Token::Ident(name) => {
                let name_lower = name.as_ref().to_ascii_lowercase();
                match name_lower.as_str() {
                    "auto" => {
                        tracks.push(GridTrack {
                            size: GridTrackSize::Breadth(TrackBreadth::Auto),
                            track_type: TrackListType::Explicit,
                        });
                    }
                    "min-content" => {
                        tracks.push(GridTrack {
                            size: GridTrackSize::Breadth(TrackBreadth::MinContent),
                            track_type: TrackListType::Explicit,
                        });
                    }
                    "max-content" => {
                        tracks.push(GridTrack {
                            size: GridTrackSize::Breadth(TrackBreadth::MaxContent),
                            track_type: TrackListType::Explicit,
                        });
                    }
                    _ => {
                        // Unknown identifier, stop parsing
                        break;
                    }
                }
            }
            _ => {
                // Unknown token, stop parsing
                break;
            }
        }
    }

    if let Some(repeat) = auto_repeat {
        GridAxisTracks::with_auto_repeat(tracks, gap, repeat)
    } else {
        GridAxisTracks::new(tracks, gap)
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
