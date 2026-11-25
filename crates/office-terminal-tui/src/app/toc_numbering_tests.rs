use crate::app::toc_numbering::tree_numbers;

fn nums(levels: &[usize]) -> Vec<String> {
    tree_numbers(levels).0
}

fn depths_of(levels: &[usize]) -> Vec<usize> {
    tree_numbers(levels).1
}

#[test]
fn numbers_follow_preorder_tree() {
    assert_eq!(nums(&[0, 1, 2, 1, 0]), vec!["1", "1.1", "1.1.1", "1.2", "2"]);
    assert_eq!(depths_of(&[0, 1, 2, 1, 0]), vec![0, 1, 2, 1, 0]);
}

#[test]
fn siblings_share_depth_and_reset_when_parent_changes() {
    assert_eq!(nums(&[0, 0, 1, 1, 0, 1]), vec!["1", "2", "2.1", "2.2", "3", "3.1"]);
    assert_eq!(depths_of(&[0, 0, 1, 1, 0, 1]), vec![0, 0, 1, 1, 0, 1]);
}

#[test]
fn skipped_levels_number_as_children_of_the_last_ancestor() {
    // 0 -> 2 skips level 1: both level-2 entries are children of "1".
    assert_eq!(nums(&[0, 2, 2]), vec!["1", "1.1", "1.2"]);
    assert_eq!(depths_of(&[0, 2, 2]), vec![0, 1, 1]);
}

#[test]
fn outlines_starting_below_level_zero_are_normalized() {
    assert_eq!(nums(&[1, 2]), vec!["1", "1.1"]);
    assert_eq!(depths_of(&[1, 2]), vec![0, 1]);
}

#[test]
fn empty_outline_yields_empty_columns() {
    assert!(nums(&[]).is_empty());
    assert!(depths_of(&[]).is_empty());
}

#[test]
fn deep_nesting_keeps_dotted_numbers_in_sync_with_depth() {
    let levels = [0, 1, 2, 3, 1];
    let numbers = nums(&levels);
    let depths = depths_of(&levels);
    assert_eq!(numbers, vec!["1", "1.1", "1.1.1", "1.1.1.1", "1.2"]);
    for (number, depth) in numbers.iter().zip(depths) {
        assert_eq!(number.matches('.').count(), depth);
    }
}
