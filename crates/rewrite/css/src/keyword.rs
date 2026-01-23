/// CSS keyword values.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CssKeyword {
    // Global
    Initial,
    Inherit,
    Unset,
    Revert,

    // Auto/None
    Auto,
    None,
    Normal,

    // Display
    Block,
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
    Table,
    TableRow,
    TableCell,
    ListItem,
    Contents,
    FlowRoot,

    // Position
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,

    // Float
    Left,
    Right,

    // Clear
    Both,

    // Overflow
    Visible,
    Hidden,
    Scroll,
    Clip,

    // Box Sizing
    ContentBox,
    BorderBox,

    // Flex Direction
    Row,
    RowReverse,
    Column,
    ColumnReverse,

    // Flex Wrap
    Nowrap,
    Wrap,
    WrapReverse,

    // Justify Content / Align Items
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
    Baseline,

    // Text Align
    Start,
    End,
    Justify,

    // Font Weight
    Thin,
    ExtraLight,
    Light,
    Regular,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,

    // Font Style
    Italic,
    Oblique,

    // Text Transform
    Uppercase,
    Lowercase,
    Capitalize,

    // White Space
    Pre,
    PreWrap,
    PreLine,

    // Word Break
    BreakAll,
    KeepAll,
    BreakWord,

    // Vertical Align
    Top,
    Middle,
    Bottom,
    TextTop,
    TextBottom,
    Sub,
    Super,

    // Cursor
    Pointer,
    Default,
    Text,
    Move,
    NotAllowed,
    Grab,
    Grabbing,

    // Visibility
    Collapse,

    // Border Style
    Solid,
    Dashed,
    Dotted,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,

    // Background Size
    Cover,
    Contain,

    // Background Repeat
    Repeat,
    RepeatX,
    RepeatY,
    NoRepeat,

    // Object Fit
    Fill,
    ScaleDown,

    // Mix Blend Mode
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,

    // Writing Mode
    HorizontalTb,
    VerticalRl,
    VerticalLr,

    // Other common keywords
    Transparent,
    CurrentColor,
    Min,
    Max,
    FitContent,
    MinContent,
    MaxContent,
}
