//! Flex Containers — container properties and enums
//! Spec: <https://www.w3.org/TR/css-flexbox-1/#flex-containers>

/// Spec: <https://www.w3.org/TR/css-flexbox-1/#flex-direction-property>
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

/// Spec: <https://www.w3.org/TR/css-flexbox-1/#flex-wrap-property>
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum FlexWrap {
    Nowrap,
    Wrap,
    WrapReverse,
}
