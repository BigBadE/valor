// Temporary test file to debug grid layout issue

use valor::crates::css::modules::grid::{
    layout_grid, GridAxisTracks, GridContainerInputs, GridItem, GridTrack, GridTrackSize,
    TrackBreadth, TrackListType,
};

fn main() {
    // Reproduce grid_03_grid_with_gap.html:
    // grid-template-columns: 90px 90px
    // grid-template-rows: 45px 45px
    // gap: 10px

    let items = vec![
        GridItem::new(0), // item a
        GridItem::new(1), // item b
        GridItem::new(2), // item c
        GridItem::new(3), // item d
    ];

    let row_tracks = GridAxisTracks::new(
        vec![
            GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Length(45.0)),
                track_type: TrackListType::Explicit,
            },
            GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Length(45.0)),
                track_type: TrackListType::Explicit,
            },
        ],
        10.0, // gap
    );

    let col_tracks = GridAxisTracks::new(
        vec![
            GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Length(90.0)),
                track_type: TrackListType::Explicit,
            },
            GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Length(90.0)),
                track_type: TrackListType::Explicit,
            },
        ],
        10.0, // gap
    );

    let inputs = GridContainerInputs::new(row_tracks, col_tracks, 180.0, 90.0);

    match layout_grid(&items, &inputs) {
        Ok(result) => {
            println!("Grid layout succeeded!");
            println!("Total: {}x{}", result.total_width, result.total_height);
            println!("\nItems:");
            for (idx, item) in result.items.iter().enumerate() {
                println!(
                    "  Item {}: pos=({}, {}), size=({}, {}), area=({},{} to {},{})",
                    idx,
                    item.x,
                    item.y,
                    item.width,
                    item.height,
                    item.area.col_start,
                    item.area.row_start,
                    item.area.col_end,
                    item.area.row_end
                );
            }

            // Expected:
            println!("\nExpected:");
            println!("  Item 0 (a): pos=(0, 0), size=(90, 45)");
            println!("  Item 1 (b): pos=(100, 0), size=(90, 45)");
            println!("  Item 2 (c): pos=(0, 55), size=(90, 45)");
            println!("  Item 3 (d): pos=(100, 55), size=(90, 45)");
        }
        Err(e) => {
            eprintln!("Grid layout failed: {}", e);
        }
    }
}
