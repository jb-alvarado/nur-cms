pub trait StrCompare {
    fn is_equal_to_str(&self, other: &str) -> bool;
}

pub trait ColumnCounter {
    fn total_count(&self) -> i64;
}
