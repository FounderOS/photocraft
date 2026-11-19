//! The .pcraft name a document with no file of its own saves as where the host keeps only those.

use super::native_file_name;

#[test]
fn the_whole_title_becomes_the_file_name() {
    for (title, file) in [
        ("Untitled-1", "Untitled-1.pcraft"),
        ("Banner v1.2", "Banner v1.2.pcraft"),
        ("Logo.final", "Logo.final.pcraft"),
        ("a/b", "a_b.pcraft"),
        ("a\\b\tc", "a_b_c.pcraft"),
        ("  Banner  ", "Banner.pcraft"),
        ("x.PCRAFT", "x.pcraft"),
        ("x.pcraft", "x.pcraft"),
        ("", "Untitled-1.pcraft"),
        ("   ", "Untitled-1.pcraft"),
        ("..", "Untitled-1.pcraft"),
        (".pcraft", "Untitled-1.pcraft"),
        ("Æøå", "Æøå.pcraft"),
    ] {
        assert_eq!(native_file_name(title), file, "{title:?}");
    }
}
