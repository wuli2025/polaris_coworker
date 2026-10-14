//! Native platform path construction preserves the Windows contract.
use super::*;

#[test]
fn rel_of_is_root_relative_slash_path() {
    let root = if cfg!(windows) { Path::new(r"C:\kb") } else { Path::new("/kb") };
    assert_eq!(rel_of(root, root), "");
    assert_eq!(rel_of(&root.join("a"), root), "a");
    assert_eq!(rel_of(&root.join("a").join("b"), root), "a/b");
}
